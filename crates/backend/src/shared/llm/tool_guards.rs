//! Гарды на границе вызова инструмента: единая точка, где решается судьба вызова.
//!
//! Зачем отдельный модуль, а не проверки по месту: до этого запреты были
//! размазаны внутри `tool_executor::execute_tool_call` (мутации артефактов,
//! мутации данных), а зацикливание модели гасилось молча — memo отдавал
//! кэшированный результат, и модель не узнавала, что повторяется. В итоге ни
//! одно срабатывание не было видно ни в трассе, ни на дашборде: «сколько раз
//! система удержала агента от глупости» — величина, которой не существовало.
//!
//! Разделение обязанностей с исполнителем: гард решает **можно ли звать**,
//! исполнитель — **как исполнить**. Проверки прав в исполнителе остаются
//! (он вызывается и из фоновых сценариев, где цикла чата нет), но теперь это
//! вторая линия, а не единственная.
//!
//! Шов под подтверждение человеком: `before_tool` — единственное место, где
//! принимается решение о вызове. Approval добавляется сюда третьим вариантом
//! `GuardDecision`, и цикл чата при этом не меняется.

use std::collections::{HashMap, HashSet};

use super::skill_policy;

/// Сколько раз подряд модель может позвать инструмент с теми же аргументами,
/// прежде чем это будет признано петлёй.
///
/// Два повтора — нормальная работа (повтор после исправления соседнего шага),
/// третий одинаковый вызов уже ничего нового не приносит: аргументы те же,
/// данные внутри одного ответа не меняются.
const MAX_IDENTICAL_CALLS: usize = 3;

/// Решение гарда о вызове.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardDecision {
    Allow,
    /// Вызов не состоится; текст уходит модели вместо результата инструмента.
    Deny {
        code: &'static str,
        message: String,
    },
}

impl GuardDecision {
    pub fn is_denied(&self) -> bool {
        matches!(self, GuardDecision::Deny { .. })
    }
}

/// Права специализации, относящиеся к вызову. Снимок на ответ: матрица
/// доступа читается из БД один раз в начале, а не на каждый вызов.
#[derive(Debug, Clone, Copy)]
pub struct GuardCapabilities {
    pub artifact_publish_allowed: bool,
    pub data_repair_execute_allowed: bool,
}

/// Состояние гардов в пределах ОДНОГО ответа пользователю.
///
/// Живёт ровно столько же, сколько цикл инструментов: счётчик повторов между
/// разными ответами смысла не имеет — пользователь мог осознанно попросить
/// то же самое ещё раз.
#[derive(Debug)]
pub struct GuardState {
    capabilities: GuardCapabilities,
    /// (инструмент, аргументы) → сколько раз уже звали.
    call_counts: HashMap<(String, String), usize>,
    /// Классы отказов, по которым подсказка уже выдавалась.
    hinted_failures: HashSet<&'static str>,
}

impl GuardState {
    pub fn new(capabilities: GuardCapabilities) -> Self {
        Self {
            capabilities,
            call_counts: HashMap::new(),
            hinted_failures: HashSet::new(),
        }
    }

    /// Решение перед вызовом инструмента.
    ///
    /// Счётчик повторов увеличивается здесь же: гард — единственный, кто видит
    /// все вызовы подряд, включая обслуженные memo.
    pub fn before_tool(&mut self, tool: &str, arguments: &str) -> GuardDecision {
        if skill_policy::is_artifact_mutation(tool) && !self.capabilities.artifact_publish_allowed {
            return GuardDecision::Deny {
                code: "mutation_blocked",
                message: format!(
                    "Инструмент '{tool}' публикует артефакт, а у специализации нет права \
                     artifact_publish. Не повторяй вызов: право выдаётся администратором \
                     в матрице возможностей, а не подбором аргументов."
                ),
            };
        }

        if skill_policy::is_data_repair_mutation(tool)
            && !self.capabilities.data_repair_execute_allowed
        {
            return GuardDecision::Deny {
                code: "mutation_blocked",
                message: format!(
                    "Инструмент '{tool}' меняет боевые данные, а у специализации нет права \
                     data_repair_execute. Подготовь изменение и передай его человеку на \
                     подтверждение — самостоятельно выполнить его нельзя."
                ),
            };
        }

        let key = (tool.to_string(), arguments.to_string());
        let seen = self.call_counts.entry(key).or_insert(0);
        *seen += 1;
        if *seen >= MAX_IDENTICAL_CALLS {
            return GuardDecision::Deny {
                code: "tool_loop",
                message: format!(
                    "Вызов '{tool}' с теми же аргументами повторяется {seen}-й раз — результат \
                     не изменится. Измени аргументы (период, фильтр, сущность) или переходи \
                     к следующему шагу; если данных нет, так и скажи пользователю."
                ),
            };
        }

        GuardDecision::Allow
    }

    /// Подсказка после неуспешного вызова — один раз на класс отказа за ответ.
    ///
    /// Повторять одно и то же после каждой ошибки бессмысленно: подсказка
    /// вытесняет контекст, а модель уже её видела.
    pub fn after_tool(&mut self, tool: &str, result: &serde_json::Value) -> Option<String> {
        let error = result.get("error").and_then(serde_json::Value::as_str)?;
        let class = classify_failure(tool, error)?;
        if !self.hinted_failures.insert(class.code) {
            return None;
        }
        Some(class.hint.to_string())
    }

    /// Число зафиксированных повторов — для тестов и диагностики.
    #[cfg(test)]
    fn calls(&self, tool: &str, arguments: &str) -> usize {
        self.call_counts
            .get(&(tool.to_string(), arguments.to_string()))
            .copied()
            .unwrap_or(0)
    }
}

struct FailureClass {
    code: &'static str,
    hint: &'static str,
}

/// Классификация отказа по тексту ошибки.
///
/// Разбор по подстроке — сознательный компромисс: инструменты возвращают
/// свободный текст ошибки, и вводить сквозной код ошибки ради подсказки
/// пришлось бы во все пятнадцать семейств сразу.
fn classify_failure(tool: &str, error: &str) -> Option<FailureClass> {
    let lower = error.to_lowercase();

    if lower.contains("no such column")
        || lower.contains("no such table")
        || lower.contains("syntax error")
    {
        return Some(FailureClass {
            code: "sql_error",
            hint: "SQL не сошёлся со схемой. Не угадывай имена колонок: вызови \
                   get_entity_schema для нужной сущности и собери запрос по её полям. \
                   Логические поля документов лежат внутри JSON-колонок — к ним \
                   обращайся через json_extract, отдельных колонок для них нет.",
        });
    }

    if lower.contains("не активен в текущем наборе") || lower.contains("unknown tool")
    {
        return Some(FailureClass {
            code: "tool_unavailable",
            hint: "Инструмент недоступен в текущем наборе. Посмотри list_skills() и \
                   активируй нужный навык через use_skill(\"<id>\") — перебирать имена \
                   инструментов бесполезно.",
        });
    }

    if tool.starts_with("plugin_") && lower.contains("sql") {
        return Some(FailureClass {
            code: "plugin_sql_guard",
            hint: "SQL плагина не прошёл проверку: разрешён один SELECT/WITH без \
                   комментариев, без SELECT * и без защищённых полей (api_key, *_token, \
                   secret). Перепиши запрос под эти ограничения.",
        });
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn state() -> GuardState {
        GuardState::new(GuardCapabilities {
            artifact_publish_allowed: true,
            data_repair_execute_allowed: true,
        })
    }

    #[test]
    fn third_identical_call_is_denied() {
        let mut guards = state();
        assert_eq!(
            guards.before_tool("execute_query", "{\"sql\":\"x\"}"),
            GuardDecision::Allow
        );
        assert_eq!(
            guards.before_tool("execute_query", "{\"sql\":\"x\"}"),
            GuardDecision::Allow
        );
        let decision = guards.before_tool("execute_query", "{\"sql\":\"x\"}");
        assert!(decision.is_denied());
        match decision {
            GuardDecision::Deny { code, .. } => assert_eq!(code, "tool_loop"),
            GuardDecision::Allow => unreachable!(),
        }
    }

    #[test]
    fn different_arguments_are_not_a_loop() {
        let mut guards = state();
        for i in 0..6 {
            let args = format!("{{\"sql\":\"select {i}\"}}");
            assert_eq!(
                guards.before_tool("execute_query", &args),
                GuardDecision::Allow
            );
        }
        assert_eq!(guards.calls("execute_query", "{\"sql\":\"select 0\"}"), 1);
    }

    #[test]
    fn artifact_mutation_requires_capability() {
        let mut guards = GuardState::new(GuardCapabilities {
            artifact_publish_allowed: false,
            data_repair_execute_allowed: true,
        });
        match guards.before_tool("build_chart", "{}") {
            GuardDecision::Deny { code, .. } => assert_eq!(code, "mutation_blocked"),
            GuardDecision::Allow => panic!("публикация без права должна блокироваться"),
        }
        // Читающий инструмент того же навыка остаётся доступным.
        assert_eq!(
            guards.before_tool("preview_data", "{}"),
            GuardDecision::Allow
        );
    }

    #[test]
    fn data_repair_mutation_requires_capability() {
        let mut guards = GuardState::new(GuardCapabilities {
            artifact_publish_allowed: true,
            data_repair_execute_allowed: false,
        });
        assert!(guards
            .before_tool("execute_funnel_repair", "{}")
            .is_denied());
        // Подготовка починки — чтение, её блокировать нечем.
        assert_eq!(
            guards.before_tool("prepare_funnel_repair", "{}"),
            GuardDecision::Allow
        );
    }

    #[test]
    fn failure_hint_is_emitted_once_per_class() {
        let mut guards = state();
        let sql_error = json!({ "error": "no such column: revenue_total" });
        assert!(guards.after_tool("execute_query", &sql_error).is_some());
        assert!(guards.after_tool("execute_query", &sql_error).is_none());

        // Другой класс отказа всё ещё получает свою подсказку.
        let missing_tool =
            json!({ "error": "Инструмент 'build_chart' не активен в текущем наборе." });
        assert!(guards.after_tool("build_chart", &missing_tool).is_some());
    }

    #[test]
    fn successful_result_produces_no_hint() {
        let mut guards = state();
        assert!(guards
            .after_tool("execute_query", &json!({ "rows": [] }))
            .is_none());
        // Неклассифицированная ошибка тоже не порождает шума.
        assert!(guards
            .after_tool("execute_query", &json!({ "error": "что-то пошло не так" }))
            .is_none());
    }
}
