//! Структуры самодостаточного бандла плагина и хранимого определения.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const MAX_PLUGIN_CODE_LEN: usize = 96;
pub const MAX_RESOURCE_NAME_LEN: usize = 80;
pub const MAX_SCRIPT_BYTES: usize = 256 * 1024;
pub const MAX_STYLES_BYTES: usize = 128 * 1024;
pub const MAX_SQL_RESOURCES: usize = 64;
pub const MAX_SQL_RESOURCE_BYTES: usize = 128 * 1024;
pub const MAX_ASSETS: usize = 64;
pub const MAX_ASSET_BYTES: usize = 512 * 1024;
pub const MAX_TOTAL_ASSET_BYTES: usize = 2 * 1024 * 1024;

const SUPPORTED_API_VERSIONS: &[&str] = &["1", "2"];

// ============================================================================
// Runtime — место исполнения кода плагина
// ============================================================================

/// Где исполняется код плагина (универсальная замена «Report/Processor»).
///
/// - `Client` — JavaScript-логика и рендер целиком в браузере.
/// - `Server` — JavaScript-логика на бэкенде через `/api/plugin/:id/invoke`.
/// - `Hybrid` — есть и `client_script`, и `server_script`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginRuntime {
    Client,
    Server,
    Hybrid,
}

impl PluginRuntime {
    pub fn as_str(&self) -> &'static str {
        match self {
            PluginRuntime::Client => "client",
            PluginRuntime::Server => "server",
            PluginRuntime::Hybrid => "hybrid",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "server" => PluginRuntime::Server,
            "hybrid" => PluginRuntime::Hybrid,
            _ => PluginRuntime::Client,
        }
    }

    /// Нужно ли исполнять код на сервере.
    pub fn runs_on_server(&self) -> bool {
        matches!(self, PluginRuntime::Server | PluginRuntime::Hybrid)
    }

    /// Нужно ли исполнять код в браузере.
    pub fn runs_on_client(&self) -> bool {
        matches!(self, PluginRuntime::Client | PluginRuntime::Hybrid)
    }
}

// ============================================================================
// Status
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginStatus {
    Draft,
    Active,
    Disabled,
}

impl PluginStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            PluginStatus::Draft => "draft",
            PluginStatus::Active => "active",
            PluginStatus::Disabled => "disabled",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "active" => PluginStatus::Active,
            "disabled" => PluginStatus::Disabled,
            _ => PluginStatus::Draft,
        }
    }
}

// ============================================================================
// Params — типизированная форма параметров (привязывается к FilterBar)
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamType {
    Date,
    DateRange,
    String,
    Integer,
    Float,
    Boolean,
    /// Ссылка/мультивыбор (напр. кабинеты МП).
    Ref,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamSpec {
    /// Уникальный ключ параметра.
    pub key: String,
    pub param_type: ParamType,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    #[serde(default)]
    pub required: bool,
    /// Привязка к глобальному фильтру дашборда (date_from / date_to / connection_ids).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub global_filter_key: Option<String>,
}

// ============================================================================
// DataBinding — декларативная привязка к DataView
// ============================================================================

/// Режим получения данных декларативного плагина. Старые hybrid bundle без этих
/// полей продолжают исполняться через `server_script`/`sql_resources`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginDataMode {
    #[default]
    Live,
    Snapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginSchemaAggregate {
    Sum,
    Count,
    Avg,
    Min,
    Max,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSchemaMetric {
    pub field_id: String,
    pub aggregate: PluginSchemaAggregate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginSchemaFilterOperator {
    Eq,
    NotEq,
    Lt,
    Lte,
    Gt,
    Gte,
    Between,
    In,
    NotIn,
    Contains,
    IsNull,
    IsNotNull,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSchemaFilter {
    pub field_id: String,
    pub operator: PluginSchemaFilterOperator,
    #[serde(default)]
    pub value: Option<serde_json::Value>,
    #[serde(default)]
    pub values: Vec<serde_json::Value>,
    #[serde(default)]
    pub from: Option<serde_json::Value>,
    #[serde(default)]
    pub to: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginSchemaSortDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSchemaSortRule {
    pub field_id: String,
    pub direction: PluginSchemaSortDirection,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginDataViewContext {
    pub date_from: String,
    pub date_to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period2_from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period2_to: Option<String>,
    #[serde(default)]
    pub connection_mp_refs: Vec<String>,
    #[serde(default)]
    pub params: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PluginDataSource {
    Schema {
        schema_id: String,
        #[serde(default)]
        fields: Vec<String>,
        #[serde(default)]
        group_by: Vec<String>,
        #[serde(default)]
        metrics: Vec<PluginSchemaMetric>,
        #[serde(default)]
        filters: Vec<PluginSchemaFilter>,
        #[serde(default)]
        sort: Vec<PluginSchemaSortRule>,
    },
    Dataview {
        view_id: String,
        #[serde(default)]
        metric_ids: Vec<String>,
        group_by: String,
        context: PluginDataViewContext,
    },
    Sql {
        sql: String,
        #[serde(default)]
        params: Vec<serde_json::Value>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSnapshotMeta {
    pub plugin_version: i32,
    pub created_at: DateTime<Utc>,
    pub row_count: usize,
    pub size_bytes: usize,
    pub source_hash: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DataBinding {
    /// ID DataView (напр. "dv001_revenue") для compute/drilldown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view_id: Option<String>,
    /// Метрика внутри DataView.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric_id: Option<String>,
    /// Разрез по умолчанию для табличного вывода.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<PluginDataSource>,
    #[serde(default)]
    pub default_mode: PluginDataMode,
}

// ============================================================================
// ViewSpec / Widget — описание вывода (совместимо по смыслу с a024 view_spec)
// ============================================================================
//
// ⚠️ RESERVED / не основной путь. `ViewSpec`/`Widget`/`custom_html` сейчас не
// рендерятся хостом (всюду `ViewSpec::default()`), серверная санитизация HTML не
// реализована. Канонический путь вывода — `client_script` строит UI сам. Не
// заполняй эти поля без явной необходимости.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WidgetKind {
    Table,
    Cards,
    Chart,
    Html,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Widget {
    pub kind: WidgetKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Произвольная конфигурация блока (колонки таблицы, оси графика, html-шаблон …).
    #[serde(default)]
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ViewSpec {
    #[serde(default)]
    pub widgets: Vec<Widget>,
    /// Опциональный пользовательский HTML-шаблон (санитизируется на сервере).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_html: Option<String>,
}

// ============================================================================
// Manifest — шапка плагина
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Свободный человекочитаемый код-идентификатор (как имя внешнего отчёта).
    pub code: String,
    pub title: String,
    pub runtime: PluginRuntime,
    /// Версия API движка плагинов, на которую рассчитан бандл.
    #[serde(default = "default_api_version")]
    pub api_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// ⚠️ RESERVED. Декларация capabilities (defense-in-depth) — пока не
    /// enforced'ится движком; носит справочный характер.
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Номер миграции БД, на который рассчитан плагин (для ручной сверки
    /// с текущей миграцией инстанса). Не валидируется и не блокирует запуск.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub built_for_migration: Option<i64>,
}

fn default_api_version() -> String {
    "1".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum PluginCapability {
    DbReadAll,
    DbRead(String),
    NetworkNone,
    AssetsRead,
    PluginInvoke,
    /// Право вызвать Действие механизма Процессов: `action:<name>`.
    ///
    /// Единственная capability с побочным эффектом. Намеренно без варианта
    /// «все Действия»: `db:read:*` допустим, потому что чтение обратимо, а
    /// проведение документа — нет, поэтому право выдаётся поимённо.
    Action(String),
    Unknown(String),
}

impl PluginCapability {
    pub fn parse(raw: &str) -> Self {
        let value = raw.trim();
        match value {
            "db:read:*" | "data:read" => Self::DbReadAll,
            "network:none" => Self::NetworkNone,
            "assets:read" => Self::AssetsRead,
            "plugin:invoke" => Self::PluginInvoke,
            _ => value
                .strip_prefix("db:read:")
                .filter(|scope| !scope.trim().is_empty())
                .map(|scope| Self::DbRead(scope.trim().to_ascii_lowercase()))
                .or_else(|| {
                    value
                        .strip_prefix("action:")
                        .map(|name| name.trim())
                        .filter(|name| !name.is_empty() && *name != "*")
                        .map(|name| Self::Action(name.to_ascii_lowercase()))
                })
                .unwrap_or_else(|| Self::Unknown(value.to_string())),
        }
    }

    pub fn canonical(&self) -> String {
        match self {
            Self::DbReadAll => "db:read:*".to_string(),
            Self::DbRead(scope) => format!("db:read:{scope}"),
            Self::NetworkNone => "network:none".to_string(),
            Self::AssetsRead => "assets:read".to_string(),
            Self::PluginInvoke => "plugin:invoke".to_string(),
            Self::Action(name) => format!("action:{name}"),
            Self::Unknown(value) => value.clone(),
        }
    }
}

impl PluginManifest {
    pub fn parsed_capabilities(&self) -> Vec<PluginCapability> {
        self.capabilities
            .iter()
            .map(|value| PluginCapability::parse(value))
            .collect()
    }
}

// ============================================================================
// Bundle — самодостаточный артефакт
// ============================================================================

/// Самодостаточный переносимый артефакт плагина.
///
/// **Канонический путь** (используй его): `client_script` (UI в iframe) +
/// `server_script` (логика в QuickJS) + `sql_resources` (именованные SELECT) +
/// `styles`. Поля `data` (DataBinding), `view_spec`, `manifest.capabilities` —
/// RESERVED и в основном потоке не задействованы.
///
/// Единица переноса между экземплярами — именно `PluginBundle`; идентичность —
/// `manifest.code`. Локальное состояние живёт в [`PluginDefinition`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginBundle {
    pub manifest: PluginManifest,
    #[serde(default)]
    pub params: Vec<ParamSpec>,
    /// ⚠️ RESERVED — см. [`DataBinding`].
    #[serde(default)]
    pub data: DataBinding,
    /// ES-модуль, исполняемый в изолированном iframe (для `Client`/`Hybrid`).
    /// Должен экспортировать `mount(root, host)`; `unmount()` опционален.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_script: Option<String>,
    /// ES-модуль, исполняемый QuickJS на сервере (для `Server`/`Hybrid`).
    /// Экспортированные функции вызываются через `/api/plugin/:id/invoke`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_script: Option<String>,
    /// ⚠️ RESERVED — см. [`ViewSpec`] (хостом не рендерится).
    #[serde(default)]
    pub view_spec: ViewSpec,
    /// CSS плагина (scoped под `.plugin-<code>` при инжекте).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub styles: Option<String>,
    /// Именованные SQL-запросы серверной части.
    ///
    /// Скрипт получает к ним доступ через `host.db.queryResource(name, params)`.
    #[serde(default)]
    pub sql_resources: HashMap<String, String>,
    /// Вложения (имя → содержимое / data-URL).
    #[serde(default)]
    pub assets: HashMap<String, String>,
}

/// Является ли SQL читающим (только `SELECT`/`WITH`).
///
/// Та же проверка применяется движком в `host.db.query`/`queryResource`; держим её
/// в контракте, чтобы запрещённый SQL отсекался ещё до сохранения бандла.
pub fn is_read_only_sql(sql: &str) -> bool {
    let trimmed = sql.trim();
    if trimmed.is_empty()
        || trimmed.starts_with("--")
        || trimmed.starts_with("/*")
        || trimmed.contains("--")
        || trimmed.contains("/*")
        || trimmed.contains("*/")
    {
        return false;
    }

    let statement = trimmed.strip_suffix(';').unwrap_or(trimmed).trim_end();
    if statement.contains(';') {
        return false;
    }

    let mut tokens = sql_tokens(statement);
    let Some(first) = tokens.next() else {
        return false;
    };
    if first != "SELECT" && first != "WITH" {
        return false;
    }

    const FORBIDDEN: &[&str] = &[
        "PRAGMA", "ATTACH", "DETACH", "INSERT", "UPDATE", "DELETE", "DROP", "ALTER", "CREATE",
        "REPLACE", "VACUUM", "REINDEX",
    ];
    !std::iter::once(first)
        .chain(tokens)
        .any(|token| FORBIDDEN.contains(&token.as_str()))
}

pub fn is_valid_plugin_code(code: &str) -> bool {
    let code = code.trim();
    !code.is_empty()
        && code.len() <= MAX_PLUGIN_CODE_LEN
        && code
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

pub fn is_valid_resource_name(name: &str) -> bool {
    let name = name.trim();
    !name.is_empty()
        && name.len() <= MAX_RESOURCE_NAME_LEN
        && !name.contains("..")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn sql_tokens(sql: &str) -> impl Iterator<Item = String> + '_ {
    sql.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_uppercase())
}

fn text_len(value: Option<&String>) -> usize {
    value.map(|s| s.as_bytes().len()).unwrap_or(0)
}

fn has_non_empty_text(value: Option<&String>) -> bool {
    value.map(|s| !s.trim().is_empty()).unwrap_or(false)
}

impl PluginBundle {
    /// Базовая валидация бандла (без сохранения).
    pub fn validate(&self) -> Result<(), String> {
        if !is_valid_plugin_code(&self.manifest.code) {
            return Err("Код плагина не может быть пустым".into());
        }
        if self.manifest.title.trim().is_empty() {
            return Err("Название плагина не может быть пустым".into());
        }
        if !SUPPORTED_API_VERSIONS.contains(&self.manifest.api_version.trim()) {
            return Err(format!(
                "Unsupported plugin api_version '{}'",
                self.manifest.api_version
            ));
        }
        if self.manifest.runtime.runs_on_client()
            && !has_non_empty_text(self.client_script.as_ref())
        {
            return Err("Runtime client/hybrid требует client_script".into());
        }
        if self.manifest.runtime.runs_on_server()
            && !has_non_empty_text(self.server_script.as_ref())
        {
            return Err("Runtime server/hybrid требует server_script".into());
        }
        if text_len(self.client_script.as_ref()) > MAX_SCRIPT_BYTES
            || text_len(self.server_script.as_ref()) > MAX_SCRIPT_BYTES
        {
            return Err("Plugin script is too large".into());
        }
        if text_len(self.styles.as_ref()) > MAX_STYLES_BYTES {
            return Err("Plugin styles are too large".into());
        }
        if self.sql_resources.len() > MAX_SQL_RESOURCES {
            return Err("Plugin has too many SQL resources".into());
        }
        if self.assets.len() > MAX_ASSETS {
            return Err("Plugin has too many assets".into());
        }
        let total_asset_bytes: usize = self
            .assets
            .values()
            .map(|asset| asset.as_bytes().len())
            .sum();
        if total_asset_bytes > MAX_TOTAL_ASSET_BYTES {
            return Err("Plugin assets are too large".into());
        }
        for (name, sql) in &self.sql_resources {
            if !is_valid_resource_name(name) || sql.trim().is_empty() {
                return Err("SQL resource names and text must be non-empty and safe".into());
            }
            if sql.as_bytes().len() > MAX_SQL_RESOURCE_BYTES {
                return Err(format!("SQL resource '{name}' is too large"));
            }
        }
        for (name, asset) in &self.assets {
            if !is_valid_resource_name(name) {
                return Err("Plugin asset names must be safe".into());
            }
            if asset.as_bytes().len() > MAX_ASSET_BYTES {
                return Err(format!("Plugin asset '{name}' is too large"));
            }
        }
        if self
            .sql_resources
            .iter()
            .any(|(name, sql)| name.trim().is_empty() || sql.trim().is_empty())
        {
            return Err("Имя и текст SQL-ресурса не могут быть пустыми".into());
        }
        if let Some((name, _)) = self
            .sql_resources
            .iter()
            .find(|(_, sql)| !is_read_only_sql(sql))
        {
            return Err(format!(
                "SQL-ресурс '{name}' должен начинаться с SELECT или WITH (разрешено только чтение)"
            ));
        }
        Ok(())
    }
}

// ============================================================================
// Ошибки исполнения / отчёт валидации (для движка, фронта и LLM-инструментов)
// ============================================================================

/// Структурированная ошибка исполнения плагина.
///
/// `stage` указывает этап сбоя для самоисправления (в т.ч. LLM-агентом):
/// `module_eval` | `missing_export` | `invoke` | `runtime` | `sql` | `deserialize` | `timeout`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginError {
    pub stage: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack: Option<String>,
}

impl PluginError {
    pub fn new(stage: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            stage: stage.into(),
            message: message.into(),
            stack: None,
        }
    }

    pub fn with_stack(mut self, stack: Option<String>) -> Self {
        self.stack = stack.filter(|s| !s.trim().is_empty());
        self
    }
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.stage, self.message)
    }
}

impl std::error::Error for PluginError {}

/// Результат проверки бандла без сохранения и без вызова функций плагина.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginValidateReport {
    pub ok: bool,
    /// Имена функций, экспортированных серверным ES-модулем.
    #[serde(default)]
    pub server_exports: Vec<String>,
    /// Имена, экспортированные клиентским ES-модулем (ожидается `mount`).
    #[serde(default)]
    pub client_exports: Vec<String>,
    #[serde(default)]
    pub errors: Vec<PluginError>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server_bundle(sql_name: &str, sql: &str) -> PluginBundle {
        PluginBundle {
            manifest: PluginManifest {
                code: "T".into(),
                title: "T".into(),
                runtime: PluginRuntime::Server,
                api_version: "2".into(),
                description: None,
                capabilities: vec![],
                built_for_migration: None,
            },
            params: vec![],
            data: DataBinding::default(),
            client_script: None,
            server_script: Some("export function run() {}".into()),
            view_spec: ViewSpec::default(),
            styles: None,
            sql_resources: [(sql_name.to_string(), sql.to_string())]
                .into_iter()
                .collect(),
            assets: HashMap::new(),
        }
    }

    #[test]
    fn accepts_read_only_sql() {
        assert!(server_bundle("ok", "WITH x AS (SELECT 1) SELECT * FROM x")
            .validate()
            .is_ok());
        assert!(server_bundle("ok", "SELECT 1;").validate().is_ok());
    }

    #[test]
    fn rejects_non_select_resource() {
        let error = server_bundle("danger", "DELETE FROM a004_nomenclature")
            .validate()
            .unwrap_err();
        assert!(error.contains("danger"), "got: {error}");
    }

    #[test]
    fn old_bundle_and_invoke_payloads_default_to_live() {
        let binding: DataBinding = serde_json::from_str("{}").expect("legacy data binding");
        assert!(binding.source.is_none());
        assert_eq!(binding.default_mode, PluginDataMode::Live);

        let request: PluginInvokeRequest = serde_json::from_value(serde_json::json!({
            "method": "data",
            "args": {}
        }))
        .expect("legacy invoke request");
        assert_eq!(request.data_mode, PluginDataMode::Live);
    }

    #[test]
    fn rejects_write_tokens_inside_cte() {
        assert!(!is_read_only_sql(
            "WITH gone AS (DELETE FROM a004_nomenclature RETURNING id) SELECT * FROM gone"
        ));
    }

    #[test]
    fn rejects_multi_statement_and_sqlite_meta_commands() {
        assert!(!is_read_only_sql("SELECT 1; SELECT 2"));
        assert!(!is_read_only_sql("-- comment\nSELECT 1"));
        assert!(!is_read_only_sql("PRAGMA table_info(plugin)"));
        assert!(!is_read_only_sql("ATTACH DATABASE 'x' AS x"));
    }

    #[test]
    fn rejects_empty_runtime_scripts() {
        let mut bundle = server_bundle("ok", "SELECT 1");
        bundle.server_script = Some("  ".into());
        let error = bundle.validate().unwrap_err();
        assert!(error.contains("server_script"), "got: {error}");
    }

    #[test]
    fn rejects_unsafe_names_and_codes() {
        let bundle = server_bundle("../bad", "SELECT 1");
        let error = bundle.validate().unwrap_err();
        assert!(error.contains("SQL resource"), "got: {error}");

        let mut bundle = server_bundle("ok", "SELECT 1");
        bundle.manifest.code = "bad code".into();
        assert!(bundle.validate().is_err());
    }

    #[test]
    fn parses_plugin_capabilities() {
        assert_eq!(
            PluginCapability::parse("data:read"),
            PluginCapability::DbReadAll
        );
        assert_eq!(
            PluginCapability::parse("db:read:wb"),
            PluginCapability::DbRead("wb".into())
        );
        assert_eq!(
            PluginCapability::DbRead("ref".into()).canonical(),
            "db:read:ref"
        );
    }

    #[test]
    fn parses_action_capabilities() {
        assert_eq!(
            PluginCapability::parse("action:post_document"),
            PluginCapability::Action("post_document".into())
        );
        assert_eq!(
            PluginCapability::Action("post_document".into()).canonical(),
            "action:post_document"
        );
    }

    /// `action:*` не должен разбираться в право: эффекты выдаются поимённо,
    /// иначе один невнимательный манифест получает всё, что система умеет
    /// делать с миром.
    #[test]
    fn action_wildcard_is_not_a_capability() {
        assert_eq!(
            PluginCapability::parse("action:*"),
            PluginCapability::Unknown("action:*".into())
        );
        assert_eq!(
            PluginCapability::parse("action:"),
            PluginCapability::Unknown("action:".into())
        );
    }
}

// ============================================================================
// Definition — хранимое определение (бандл + локальное состояние)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDefinition {
    pub id: String,
    pub bundle: PluginBundle,
    pub status: PluginStatus,
    pub is_enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_user_id: Option<String>,
    /// Если плагин создан LLM-агентом — его id (a017).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by_agent_id: Option<String>,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Пользовательская оценка плагина (1..5; None — не оценён).
    #[serde(default)]
    pub rating: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<PluginSnapshotMeta>,
    /// Версия, опубликованная в S3 (последняя успешная публикация этой записи).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub s3_published_version: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub s3_published_at: Option<DateTime<Utc>>,
}

// ============================================================================
// DTO для списка / меню
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginListItem {
    pub id: String,
    pub code: String,
    pub title: String,
    pub runtime: String,
    pub status: String,
    pub is_enabled: bool,
    /// Текущая (локальная) версия плагина.
    #[serde(default)]
    pub version: i32,
    pub updated_at: DateTime<Utc>,
    /// Пользовательская оценка плагина (1..5; None — не оценён).
    #[serde(default)]
    pub rating: Option<i32>,
}

impl From<&PluginDefinition> for PluginListItem {
    fn from(def: &PluginDefinition) -> Self {
        Self {
            id: def.id.clone(),
            code: def.bundle.manifest.code.clone(),
            title: def.bundle.manifest.title.clone(),
            runtime: def.bundle.manifest.runtime.as_str().to_string(),
            status: def.status.as_str().to_string(),
            is_enabled: def.is_enabled,
            version: def.version,
            updated_at: def.updated_at,
            rating: def.rating,
        }
    }
}

// ============================================================================
// DTO для создания/обновления через API (и через LLM-инструменты)
// ============================================================================

// ============================================================================
// Контекст запуска плагина (параметры формы + период + кабинеты)
// ============================================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginRunContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_to: Option<String>,
    #[serde(default)]
    pub connection_mp_refs: Vec<String>,
    /// Разрез табличного вывода (перекрывает DataBinding.group_by).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_by: Option<String>,
    /// Прочие параметры формы (key → value).
    #[serde(default)]
    pub params: HashMap<String, String>,
}

/// Вызов экспортированной функции серверного ES-модуля плагина.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInvokeRequest {
    pub method: String,
    #[serde(default)]
    pub args: serde_json::Value,
    #[serde(default)]
    pub context: PluginRunContext,
    #[serde(default)]
    pub data_mode: PluginDataMode,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginSmokeMethod {
    pub method: String,
    #[serde(default)]
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginSmokeRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle: Option<PluginBundle>,
    #[serde(default)]
    pub context: PluginRunContext,
    #[serde(default)]
    pub methods: Vec<PluginSmokeMethod>,
    #[serde(default)]
    pub render: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSmokeFailure {
    pub stage: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_hint: Option<String>,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginSmokeReport {
    pub ok: bool,
    pub validate: PluginValidateReport,
    #[serde(default)]
    pub server_exports: Vec<String>,
    #[serde(default)]
    pub client_exports: Vec<String>,
    #[serde(default)]
    pub client_invokes: Vec<String>,
    #[serde(default)]
    pub failures: Vec<PluginSmokeFailure>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_next_step: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginUpsert {
    /// None — создание; Some — обновление.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub bundle: PluginBundle,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by_agent_id: Option<String>,
    /// Ожидаемая версия для оптимистичной блокировки (опц.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<i32>,
    /// Capture the resolved declarative `data` result together with this version.
    #[serde(default)]
    pub capture_snapshot: bool,
    /// Permit publication when snapshot limits are exceeded or capture fails.
    #[serde(default)]
    pub allow_live_only: bool,
}
