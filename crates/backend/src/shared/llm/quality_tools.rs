use serde_json::{json, Value};

use super::types::ToolDefinition;

pub const QUALITY_TOOL_NAMES: &[&str] = &[
    "list_quality_checks",
    "run_quality_check",
    "get_latest_quality_check",
    "quality_check_template",
    "quality_check_get",
    "quality_check_validate",
    "quality_check_upsert",
];

pub fn quality_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "list_quality_checks".into(),
            description: "Единый обзор проверок качества: цель и описание каждого правила, тип, digest и последний результат текущей версии. Сначала используй этот список, не дублируй диагностический SQL в навыке.".into(),
            parameters: json!({"type":"object","properties":{}}),
        },
        ToolDefinition {
            name: "run_quality_check".into(),
            description: "Запустить read-only quality check и получить стандартизированные метрики, нарушения и разрезы.".into(),
            parameters: json!({
                "type":"object",
                "properties":{
                    "check_id":{"type":"string"},
                    "input":{"type":"object"}
                },
                "required":["check_id"]
            }),
        },
        ToolDefinition {
            name: "get_latest_quality_check".into(),
            description: "Получить последний успешный сохранённый результат quality check без нового запуска.".into(),
            parameters: json!({
                "type":"object",
                "properties":{"check_id":{"type":"string"}},
                "required":["check_id"]
            }),
        },
        ToolDefinition {
            name: "quality_check_template".into(),
            description: "Минимальный шаблон внешней MJS-проверки. Используется разработчиком проверок перед quality_check_validate.".into(),
            parameters: json!({"type":"object","properties":{}}),
        },
        ToolDefinition {
            name: "quality_check_get".into(),
            description: "Получить активное определение проверки. Для MJS возвращает переносимый bundle; Rust-проверки помечаются как нередактируемые.".into(),
            parameters: json!({
                "type":"object",
                "properties":{"check_id":{"type":"string"}},
                "required":["check_id"]
            }),
        },
        ToolDefinition {
            name: "quality_check_validate".into(),
            description: "Проверить manifest, schema, capabilities и компиляцию MJS без сохранения и запуска.".into(),
            parameters: authoring_parameters(false),
        },
        ToolDefinition {
            name: "quality_check_upsert".into(),
            description: "Атомарно сохранить внешнюю MJS-проверку и перезагрузить каталог. Требует confirm=true после валидации и явного согласования с пользователем.".into(),
            parameters: authoring_parameters(true),
        },
    ]
}

fn authoring_parameters(require_confirm: bool) -> Value {
    let mut required = vec!["manifest", "script"];
    if require_confirm {
        required.push("confirm");
    }
    json!({
        "type":"object",
        "properties":{
            "manifest":{"type":"object"},
            "script":{"type":"string"},
            "schema":{"type":"object"},
            "confirm":{"type":"boolean"}
        },
        "required":required
    })
}

pub async fn execute_quality_tool(
    name: &str,
    arguments: &str,
    effect: &super::chat_effects::ChatEffect,
) -> Value {
    let args: Value = serde_json::from_str(arguments).unwrap_or_else(|_| json!({}));
    match name {
        "list_quality_checks" => {
            let snapshot = crate::quality::registry::snapshot();
            match crate::quality::list_check_overviews().await {
                Ok(checks) => json!({
                    "ok": true,
                    "purpose": "Центральный реестр одноцелевых read-only инвариантов качества данных. Результат относится к текущему digest определения.",
                    "generation": snapshot.generation,
                    "catalog_digest": snapshot.catalog_digest,
                    "checks": checks,
                }),
                Err(error) => json!({"ok":false,"error":error.to_string()}),
            }
        }
        // Прогон проверки — запись реестра Действий, а не вторая реализация:
        // и чат, и Этап зовут `actions::run`, поэтому получают одинаковую
        // проверку входа по схеме, ключ идемпотентности и строку в журнале
        // эффектов. Раньше здесь стоял прямой вызов `run_check_with_input`, и
        // пропущенный `check_id` молча превращался в пустую строку.
        "run_quality_check" => {
            let mut input = json!({});
            for field in ["check_id", "input"] {
                if let Some(value) = args.get(field) {
                    input[field] = value.clone();
                }
            }
            let suffix = args
                .get("check_id")
                .and_then(Value::as_str)
                .unwrap_or("-")
                .to_string();
            effect.run("run_quality_check", input, &suffix).await
        }
        "get_latest_quality_check" => {
            let check_id = args
                .get("check_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let Some(definition) = crate::quality::registry::definition(check_id) else {
                return json!({"ok":false,"error":format!("Unknown quality check: {check_id}")});
            };
            match crate::quality::history::latest_success_details_for_digest(
                check_id,
                &definition.digest,
            )
            .await
            {
                Ok(details) => json!({"ok":true,"details":details}),
                Err(error) => json!({"ok":false,"error":error.to_string()}),
            }
        }
        "quality_check_template" => json!({
            "manifest": {
                "id": "example_invariant",
                "code": "QC-999",
                "name": "Короткое название",
                "description": "Что является популяцией, какой инвариант проверяется и что означает нарушение.",
                "category": "Домен",
                "kind": "regular",
                "entrypoint": "check.mjs",
                "export": "run",
                "capabilities": ["db:read:table_name"],
                "default_input": {}
            },
            "script": "export async function run(input, host) {\n  const rows = await host.db.query(\"SELECT COUNT(*) population, SUM(CASE WHEN condition THEN 0 ELSE 1 END) violations FROM table_name\", []);\n  return { metrics: [{ label: \"Инвариант\", population: Number(rows[0].population || 0), violations: Number(rows[0].violations || 0), unit: \"строк\" }], violations: [], breakdowns: [], sources: [] };\n}\n",
            "guidance": "Одна проверка — одна цель и небольшой набор однотипных инвариантов. Не объединяйте разные бизнес-процессы только потому, что они используют одну таблицу."
        }),
        "quality_check_get" => {
            let id = args
                .get("check_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            match crate::quality::registry::authoring_bundle(id) {
                Some(bundle) => json!({"ok":true,"bundle":bundle}),
                None => json!({"ok":false,"error":format!("Unknown quality check: {id}")}),
            }
        }
        "quality_check_validate" => {
            let manifest = args.get("manifest").cloned().unwrap_or(Value::Null);
            let script = args
                .get("script")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let schema = args.get("schema");
            match crate::quality::registry::validate_authoring_bundle(&manifest, script, schema)
                .await
            {
                Ok(definition) => json!({
                    "ok":true,
                    "check_id":definition.info.id,
                    "code":definition.info.code,
                    "digest":definition.digest,
                }),
                Err(errors) => json!({"ok":false,"errors":errors}),
            }
        }
        "quality_check_upsert" => {
            if args.get("confirm").and_then(Value::as_bool) != Some(true) {
                return json!({"ok":false,"error":"Перед сохранением нужна валидация, явное согласование пользователя и confirm=true"});
            }
            let manifest = args.get("manifest").cloned().unwrap_or(Value::Null);
            let script = args
                .get("script")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let schema = args.get("schema").cloned();
            match crate::quality::registry::upsert_authoring_bundle(manifest, script, schema).await
            {
                Ok(report) => json!({"ok":true,"reload":report}),
                Err(errors) => json!({"ok":false,"errors":errors}),
            }
        }
        _ => json!({"ok":false,"error":format!("Unknown quality tool: {name}")}),
    }
}
