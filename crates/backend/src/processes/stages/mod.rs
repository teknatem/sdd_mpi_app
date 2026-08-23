//! Исполнение Этапа.
//!
//! Этап — mjs-модуль, который читает данные, при необходимости вызывает
//! Действия и возвращает **один из объявленных выходов**. Рантайм тот же, что у
//! плагинов и quality-проверок (`plugins::engine`), но поверхность `host` шире
//! на ветку `actions`, и эта ветка выдаётся только здесь.
//!
//! Главное, что делает этот модуль помимо запуска, — **разделяет три класса
//! исхода** (ADR-0011 п.10):
//!
//! - штатный выход графа — бизнес-решение Этапа;
//! - упавшее Действие — временный сбой, Этап повторяем;
//! - исключение в mjs или выход не по контракту — дефект, в карантин.
//!
//! Разделение механическое, а не по соглашению: автор Этапа не может объявить
//! выход `error` и увести дефект в штатную ветку.

pub mod validate;

use anyhow::Result;
use contracts::processes::{
    ActionActor, ActionCall, StageDefinition, StageOutcome, StageRun, StageRunContext, StageVerdict,
};
use sea_orm::DatabaseConnection;
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::plugins::engine::{self, HostActionEntry, HostExtensions, ScriptExecutionLimits};
use crate::processes::actions;

pub use validate::{validate_definition, ValidationError};

/// Лимиты прогона Этапа.
///
/// Щедрее плагинных пяти секунд и ближе к quality-проверкам: Этап ждёт Действий,
/// а перепроведение идёт минутами. Память и стек — как у всех: разница только во
/// времени, потому что различается только характер работы.
fn stage_limits() -> ScriptExecutionLimits {
    ScriptExecutionLimits {
        timeout: std::time::Duration::from_secs(300),
        ..ScriptExecutionLimits::default()
    }
}

/// Действия, разрешённые манифестом Этапа.
///
/// Право выдаётся поимённо: то, чего нет в списке, не появится в `host.actions`
/// вообще. Отсутствие метода — первая линия, но не единственная: сырой
/// `__hostActionCall` остаётся достижимым глобалом, поэтому этот же список
/// движок сверяет на каждом вызове (`engine::HostActionEntry`).
fn granted_actions(capabilities: &[String]) -> Vec<HostActionEntry> {
    capabilities
        .iter()
        .filter_map(|raw| raw.trim().strip_prefix("action:"))
        .map(str::trim)
        .filter(|name| !name.is_empty() && *name != "*")
        .filter_map(|name| {
            actions::list()
                .into_iter()
                .find(|info| info.name == name)
                .map(|info| HostActionEntry {
                    name: info.name.to_string(),
                    method: info.method.to_string(),
                })
        })
        .collect()
}

/// Собрать ключ идемпотентности вызова.
///
/// Ключ обязан воспроизводиться при повторе Этапа после падения, иначе повтор
/// сделает эффект вторым. Поэтому он выводится из контекста прогона, а не
/// генерируется: автор mjs может добавить смысловой суффикс (`options.key`),
/// и тогда ключ переживёт даже изменение порядка вызовов внутри Этапа.
///
/// **Номер захода обязателен.** В графе бывают циклы: в пилоте st0004 «Позвать
/// человека» возвращает в st0001 «Пересчитать день». Без номера захода второй
/// проход собрал бы тот же ключ, получил бы `replayed` и **ничего не
/// пересчитал** — ровно того, ради чего в цикл и возвращались. Смысловой
/// `options.key` (дата) здесь не спасает: он тоже не меняется.
fn idempotency_key(
    context: &StageRunContext,
    stage_code: &str,
    action: &str,
    options: &Value,
    ordinal: usize,
) -> String {
    let scope = context.instance_id.as_deref().unwrap_or("manual");
    let suffix = options
        .get("key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("#{ordinal}"));
    format!(
        "{scope}:{stage_code}@{}:{action}:{suffix}",
        context.visit.max(0)
    )
}

/// Прогнать Этап.
///
/// Соединение требуется `'static`, и это не придирка: обработчик `host.actions`
/// живёт в замыкании, которое переживает вызов и уходит в рантайм QuickJS.
/// Явный `'static` в сигнатуре честнее, чем тихое обращение к синглтону изнутри:
/// зависимость видно в месте вызова (`db::get_connection()`).
pub async fn run(
    db: &'static DatabaseConnection,
    definition: &StageDefinition,
    input: Value,
    context: &StageRunContext,
) -> Result<StageRun> {
    let started = Instant::now();
    let manifest = &definition.manifest;

    validate::validate_definition(definition)?;
    validate::validate_input(manifest, &input)?;

    // Наблюдение за прогоном: что записали в журнал эффектов и не упало ли
    // Действие. Второе — единственный способ отличить временный сбой от дефекта
    // Этапа, не разбирая текст исключения.
    let effect_ids: Arc<Mutex<Vec<String>>> = Arc::default();
    let action_failure: Arc<Mutex<Option<String>>> = Arc::default();
    let ordinal = Arc::new(AtomicUsize::new(0));

    let entries = granted_actions(&manifest.capabilities);
    let extensions = if entries.is_empty() {
        HostExtensions::default()
    } else {
        let handler = {
            let stage_code = manifest.code.clone();
            let context = context.clone();
            let effect_ids = effect_ids.clone();
            let action_failure = action_failure.clone();
            let ordinal = ordinal.clone();
            Arc::new(move |name: String, input: Value, options: Value| {
                let stage_code = stage_code.clone();
                let context = context.clone();
                let effect_ids = effect_ids.clone();
                let action_failure = action_failure.clone();
                let position = ordinal.fetch_add(1, Ordering::SeqCst);
                Box::pin(async move {
                    let call = ActionCall {
                        idempotency_key: idempotency_key(
                            &context,
                            &stage_code,
                            &name,
                            &options,
                            position,
                        ),
                        action: name.clone(),
                        input,
                        // Режим задаётся снаружи и внутрь не прокидывается:
                        // сухой прогон Этапа не может исполнить настоящий эффект,
                        // что бы ни написал автор mjs.
                        mode: context.mode,
                        actor: match context.instance_id.as_deref() {
                            Some(instance_id) => ActionActor::Process {
                                instance_id: instance_id.to_string(),
                                stage_code: stage_code.clone(),
                            },
                            None => ActionActor::Manual,
                        },
                    };
                    match actions::run(db, &call).await {
                        Ok(outcome) => {
                            effect_ids.lock().unwrap().push(outcome.effect_id.clone());
                            Ok(serde_json::json!({
                                "effect_id": outcome.effect_id,
                                "status": outcome.status,
                                "replayed": outcome.replayed,
                                "result": outcome.result,
                            }))
                        }
                        Err(error) => {
                            let text = error.to_string();
                            *action_failure.lock().unwrap() = Some(text.clone());
                            Err(text)
                        }
                    }
                })
                    as std::pin::Pin<
                        Box<dyn std::future::Future<Output = Result<Value, String>> + Send>,
                    >
            })
        };
        HostExtensions {
            actions: Some((entries, handler)),
        }
    };

    let outcome = engine::invoke_ephemeral_server_script_with_extensions(
        definition.script.clone(),
        manifest.export.clone(),
        input,
        manifest.capabilities.clone(),
        stage_limits(),
        extensions,
    )
    .await;

    let duration_ms = started.elapsed().as_millis() as i64;
    let effect_ids = effect_ids.lock().unwrap().clone();

    let (verdict, logs) = match outcome {
        Ok((value, logs)) => (classify_output(definition, value), logs),
        Err(error) => {
            // Скрипт не дошёл до возврата. Три случая, и различать их надо
            // фактом, а не разбором текста:
            //
            // 1. по пути упало Действие — временный сбой внешнего мира;
            // 2. прогон прерван по лимиту времени — тоже временный сбой, а не
            //    дефект: прерывание QuickJS срабатывает между операциями
            //    байткода, поэтому долгое Действие (перепроведение идёт
            //    минутами) доходит до конца, а лимит бьёт уже на возврате.
            //    Считать это дефектом значило бы отправлять в карантин при
            //    состоявшемся и успешном эффекте;
            // 3. всё остальное — сломан сам Этап.
            let verdict = match action_failure.lock().unwrap().clone() {
                Some(message) => StageVerdict::TemporaryFailure { message },
                None if is_timeout(&error) => StageVerdict::TemporaryFailure {
                    message: error.to_string(),
                },
                None => StageVerdict::Defect {
                    message: error.to_string(),
                },
            };
            (verdict, Vec::new())
        }
    };

    Ok(StageRun {
        stage_code: manifest.code.clone(),
        verdict,
        logs,
        duration_ms,
        effect_ids,
    })
}

/// Прерван ли прогон по лимиту времени.
///
/// Движок помечает такую ошибку этапом `timeout` (`engine::relabel_timeout`),
/// поэтому отличие видно фактом, а не по тексту сообщения.
fn is_timeout(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<contracts::plugins::PluginError>()
        .is_some_and(|detail| detail.stage == "timeout")
}

/// Проверить возвращённое значение против объявленных выходов.
///
/// Несовпадение — дефект, а не «ну почти»: Процесс выбирает следующий Этап по
/// имени выхода, и незаявленное имя означает, что граф дальше не читается.
fn classify_output(definition: &StageDefinition, value: Value) -> StageVerdict {
    let manifest = &definition.manifest;

    let Some(object) = value.as_object() else {
        return StageVerdict::Defect {
            message: format!(
                "Этап вернул не объект, а {}: ожидался {{ outcome, data }}",
                type_name(&value)
            ),
        };
    };
    let Some(name) = object.get("outcome").and_then(Value::as_str) else {
        return StageVerdict::Defect {
            message: "Этап не вернул поле 'outcome' со строкой — имя выхода обязательно"
                .to_string(),
        };
    };
    let Some(declared) = manifest.output(name) else {
        return StageVerdict::Defect {
            message: format!(
                "Этап вернул выход '{name}', которого нет в манифесте; объявлены: {}",
                manifest.output_names().join(", ")
            ),
        };
    };

    let data = object.get("data").cloned().unwrap_or(Value::Null);
    if let Some(schema) = &declared.data_schema {
        match jsonschema::validator_for(schema) {
            Ok(validator) => {
                if let Err(error) = validator.validate(&data) {
                    return StageVerdict::Defect {
                        message: format!("данные выхода '{name}' не по схеме: {error}"),
                    };
                }
            }
            Err(error) => {
                return StageVerdict::Defect {
                    message: format!("схема выхода '{name}' не компилируется: {error}"),
                };
            }
        }
    }

    StageVerdict::Outcome(StageOutcome {
        outcome: name.to_string(),
        data,
    })
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "число",
        Value::String(_) => "строка",
        Value::Array(_) => "массив",
        Value::Object(_) => "объект",
    }
}
