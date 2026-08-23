//! Проверка определения Этапа — статическая половина контракта.
//!
//! Рантайм проверяет фактический выход против схемы; здесь проверяется всё, что
//! можно узнать до запуска (ADR-0011 п.11). Смысл разделения простой: ошибку
//! автора Этапа надо ловить на публикации, а не на живом экземпляре процесса.

use anyhow::Result;
use contracts::processes::{StageDefinition, StageManifest};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

use crate::processes::actions;

/// Ошибка определения. Отдельный тип, а не строка: список нарушений
/// показывается автору целиком, а не по одному за публикацию.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub problems: Vec<String>,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "определение Этапа некорректно: {}",
            self.problems.join("; ")
        )
    }
}

impl std::error::Error for ValidationError {}

/// Проверить определение целиком.
pub fn validate_definition(definition: &StageDefinition) -> Result<()> {
    let manifest = &definition.manifest;
    let mut problems = Vec::new();

    if !is_valid_stage_code(&manifest.code) {
        problems.push(format!(
            "код '{}' не по форме stNNNN (четыре цифры)",
            manifest.code
        ));
    }
    if manifest.title.trim().is_empty() {
        problems.push("пустой заголовок".to_string());
    }
    if definition.script.trim().is_empty() {
        problems.push("пустой код Этапа".to_string());
    }
    if manifest.export.trim().is_empty() {
        problems.push("не указан экспорт".to_string());
    }

    // Этап без выходов не может стоять в графе: из него некуда идти.
    if manifest.outputs.is_empty() {
        problems.push("не объявлено ни одного выхода".to_string());
    }
    let mut seen = HashSet::new();
    for output in &manifest.outputs {
        if output.name.trim().is_empty() {
            problems.push("выход с пустым именем".to_string());
        } else if !seen.insert(output.name.as_str()) {
            problems.push(format!("выход '{}' объявлен дважды", output.name));
        }
        if let Some(schema) = &output.data_schema {
            if let Err(error) = jsonschema::validator_for(schema) {
                problems.push(format!(
                    "схема данных выхода '{}' не компилируется: {error}",
                    output.name
                ));
            }
        }
    }

    if let Some(schema) = &manifest.input_schema {
        if let Err(error) = jsonschema::validator_for(schema) {
            problems.push(format!("схема входа не компилируется: {error}"));
        }
    }

    // `action:<name>` без существующего Действия — опечатка, а не право.
    // Промолчать здесь означало бы отдать в работу Этап, который упадёт на
    // первом же вызове несуществующего метода.
    for capability in &manifest.capabilities {
        let raw = capability.trim();
        if let Some(name) = raw.strip_prefix("action:") {
            let name = name.trim();
            if name.is_empty() || name == "*" {
                problems.push(format!(
                    "право '{raw}' не выдаётся: эффекты разрешаются поимённо"
                ));
            } else if !actions::exists(name) {
                problems.push(format!(
                    "право '{raw}' ссылается на несуществующее Действие"
                ));
            }
        }
    }

    if problems.is_empty() {
        Ok(())
    } else {
        Err(ValidationError { problems }.into())
    }
}

/// Проверить вход прогона против схемы манифеста.
pub fn validate_input(manifest: &StageManifest, input: &Value) -> Result<()> {
    let Some(schema) = &manifest.input_schema else {
        return Ok(());
    };
    jsonschema::validator_for(schema)
        .map_err(|error| anyhow::anyhow!("схема входа Этапа не компилируется: {error}"))?
        .validate(input)
        .map_err(|error| anyhow::anyhow!("вход Этапа '{}' не по схеме: {error}", manifest.code))?;
    Ok(())
}

fn is_valid_stage_code(code: &str) -> bool {
    let code = code.trim();
    code.len() == 6
        && code.starts_with("st")
        && code[2..]
            .chars()
            .all(|character| character.is_ascii_digit())
}

/// Отпечаток определения: манифест плюс код.
///
/// Экземпляр процесса пинит версии Этапов, поэтому «тот же код» должен
/// опознаваться дёшево и однозначно — сравнением строки, а не разбором.
pub fn digest(definition: &StageDefinition) -> String {
    let manifest = serde_json::to_string(&definition.manifest).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(manifest.as_bytes());
    hasher.update(b"\n");
    hasher.update(definition.script.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use contracts::processes::{StageManifest, StageOutput};
    use serde_json::json;

    fn definition(capabilities: Vec<String>) -> StageDefinition {
        StageDefinition {
            manifest: StageManifest {
                code: "st0002".into(),
                title: "Сверить с ГК".into(),
                description: String::new(),
                entrypoint: "stage.mjs".into(),
                export: "run".into(),
                input_schema: None,
                outputs: vec![StageOutput {
                    name: "сходится".into(),
                    description: String::new(),
                    data_schema: None,
                }],
                capabilities,
            },
            script: "export async function run() { return { outcome: 'сходится' }; }".into(),
            digest: String::new(),
        }
    }

    #[test]
    fn accepts_a_well_formed_stage() {
        assert!(validate_definition(&definition(vec!["db:read:*".into()])).is_ok());
    }

    #[test]
    fn rejects_capability_for_unknown_action() {
        let error = validate_definition(&definition(vec!["action:нет_такого".into()]))
            .expect_err("несуществующее Действие обязано быть отклонено");
        assert!(
            error.to_string().contains("несуществующее Действие"),
            "{error}"
        );
    }

    /// `action:*` не выдаётся: чтение обратимо, проведение документа — нет.
    #[test]
    fn rejects_action_wildcard() {
        let error = validate_definition(&definition(vec!["action:*".into()]))
            .expect_err("action:* обязано быть отклонено");
        assert!(error.to_string().contains("поимённо"), "{error}");
    }

    #[test]
    fn rejects_stage_without_outputs() {
        let mut def = definition(vec![]);
        def.manifest.outputs.clear();
        let error = validate_definition(&def).expect_err("Этап без выходов недопустим");
        assert!(error.to_string().contains("ни одного выхода"), "{error}");
    }

    #[test]
    fn rejects_duplicate_output_names() {
        let mut def = definition(vec![]);
        def.manifest.outputs.push(StageOutput {
            name: "сходится".into(),
            description: String::new(),
            data_schema: None,
        });
        let error = validate_definition(&def).expect_err("дубль выхода недопустим");
        assert!(error.to_string().contains("объявлен дважды"), "{error}");
    }

    #[test]
    fn rejects_malformed_code() {
        let mut def = definition(vec![]);
        def.manifest.code = "stage-1".into();
        let error = validate_definition(&def).expect_err("код не по форме");
        assert!(error.to_string().contains("stNNNN"), "{error}");
    }

    #[test]
    fn input_is_checked_against_schema() {
        let mut def = definition(vec![]);
        def.manifest.input_schema = Some(json!({
            "type": "object",
            "required": ["business_date"],
            "properties": { "business_date": { "type": "string" } }
        }));
        assert!(validate_input(&def.manifest, &json!({ "business_date": "2026-08-21" })).is_ok());
        assert!(validate_input(&def.manifest, &json!({})).is_err());
    }

    /// Отпечаток обязан меняться и от кода, и от манифеста: версия Этапа — это
    /// пара, а не один только скрипт.
    #[test]
    fn digest_covers_manifest_and_script() {
        let base = definition(vec![]);
        let mut other_script = definition(vec![]);
        other_script.script.push_str("\n// правка");
        let mut other_manifest = definition(vec![]);
        other_manifest.manifest.title = "Другой заголовок".into();

        assert_ne!(digest(&base), digest(&other_script));
        assert_ne!(digest(&base), digest(&other_manifest));
        assert_eq!(digest(&base), digest(&definition(vec![])));
    }
}
