//! Метрики кода, вшитые в бинарь на сборке.
//!
//! Почему не считаем в рантайме: рядом с боевым `backend.exe` исходников нет —
//! он живёт отдельно от репозитория, — а там, где они есть, обход 1700 файлов
//! всё равно нарушил бы условие «никаких сканирований». Поэтому цифры собирает
//! `tools/gen_code_metrics.ps1` (его дёргает pre-commit hook), кладёт в
//! `codebase_metrics.json` в корне репозитория, а `include_str!` вшивает файл в
//! исполняемый файл. Метрики описывают **этот** бинарь, и между пересборками
//! честно стоят на месте.
//!
//! Формат намеренно плоский — `{"metrics": {"<ключ>": <число>}}`. Ключи те же,
//! что в `catalog.rs`, поэтому Rust-структуры, повторяющей разделы JSON, здесь
//! нет: генератор и каталог договариваются через один список имён, а не через
//! две параллельные схемы, которые разъезжаются.

use contracts::system::metrics::DetailTable;
use once_cell::sync::Lazy;
use serde::Deserialize;
use std::collections::BTreeMap;

/// Содержимое `codebase_metrics.json` на момент сборки.
///
/// Файл обязан существовать в репозитории — он закоммичен, и сборка без него
/// падает намеренно: молча отдать страницу без половины метрик хуже, чем не
/// собраться.
const RAW: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../codebase_metrics.json"
));

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CodebaseMetrics {
    /// Когда сгенерирован файл, RFC 3339.
    #[serde(default)]
    pub generated_at: Option<String>,
    #[serde(default)]
    pub git_head: Option<String>,
    #[serde(default)]
    pub metrics: BTreeMap<String, f64>,
    #[serde(default)]
    pub details: Vec<DetailTable>,
}

static PARSED: Lazy<CodebaseMetrics> = Lazy::new(|| match serde_json::from_str(RAW) {
    Ok(parsed) => parsed,
    Err(error) => {
        // Битый JSON не должен ронять старт: рантайм-половина метрик от него не
        // зависит и остаётся полезной.
        tracing::error!("[metrics] codebase_metrics.json не разобран: {error}");
        CodebaseMetrics::default()
    }
});

pub fn codebase() -> &'static CodebaseMetrics {
    &PARSED
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Файл в репозитории должен разбираться — иначе сборка тихо потеряет
    /// половину страницы, а `Lazy` подменит его пустышкой.
    #[test]
    fn embedded_json_parses() {
        let parsed: CodebaseMetrics =
            serde_json::from_str(RAW).expect("codebase_metrics.json не разобран");
        assert!(
            !parsed.metrics.is_empty(),
            "codebase_metrics.json без метрик — генератор не отработал"
        );
    }

    /// Каждый ключ из файла должен быть описан в каталоге, иначе измерение
    /// собирается и никому не показывается.
    #[test]
    fn every_embedded_key_is_described_in_catalog() {
        let parsed: CodebaseMetrics = serde_json::from_str(RAW).unwrap_or_default();
        let unknown: Vec<&str> = parsed
            .metrics
            .keys()
            .map(String::as_str)
            .filter(|key| super::super::catalog::find(key).is_none())
            .collect();
        assert!(
            unknown.is_empty(),
            "ключи есть в codebase_metrics.json, но нет в METRIC_CATALOG: {unknown:?}"
        );
    }
}
