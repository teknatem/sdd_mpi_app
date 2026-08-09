use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static EXT_API_KEY: OnceLock<String> = OnceLock::new();
static SCHEDULER_CONFIG_ENABLED: OnceLock<bool> = OnceLock::new();
static MAIL_CONFIG: OnceLock<MailConfig> = OnceLock::new();
static BITRIX24_CONFIG: OnceLock<Bitrix24Config> = OnceLock::new();

/// Store the mail configuration once at application startup, so LLM mail tools
/// (which run without access to the loaded `Config`) can read it.
pub fn set_mail_config(cfg: MailConfig) {
    let _ = MAIL_CONFIG.set(cfg);
}

/// Returns the configured mail settings, or a disabled default if never set.
pub fn get_mail_config() -> &'static MailConfig {
    static DISABLED: OnceLock<MailConfig> = OnceLock::new();
    MAIL_CONFIG
        .get()
        .unwrap_or_else(|| DISABLED.get_or_init(MailConfig::default))
}

pub fn set_bitrix24_config(cfg: Bitrix24Config) {
    let _ = BITRIX24_CONFIG.set(cfg);
}

pub fn get_bitrix24_config() -> &'static Bitrix24Config {
    static DISABLED: OnceLock<Bitrix24Config> = OnceLock::new();
    BITRIX24_CONFIG
        .get()
        .unwrap_or_else(|| DISABLED.get_or_init(Bitrix24Config::default))
}

/// Set the external API key once at application startup.
pub fn set_ext_api_key(key: String) {
    let _ = EXT_API_KEY.set(key);
}

/// Remember the config.toml `[scheduled_tasks].enabled` value at startup,
/// so the UI can warn when the scheduler is disabled at the config level.
pub fn set_scheduler_config_enabled(enabled: bool) {
    let _ = SCHEDULER_CONFIG_ENABLED.set(enabled);
}

/// Returns whether the scheduler is enabled in config.toml. Defaults to `true`
/// if it was never set (e.g. config failed to load).
pub fn get_scheduler_config_enabled() -> bool {
    *SCHEDULER_CONFIG_ENABLED.get().unwrap_or(&true)
}

/// Returns the configured external API key, or `None` if not set or empty.
pub fn get_ext_api_key() -> Option<&'static str> {
    EXT_API_KEY
        .get()
        .map(|s| s.as_str())
        .filter(|s| !s.is_empty())
}

#[derive(Debug, Deserialize, Clone)]
pub struct ExternalApiConfig {
    /// Статический API-ключ для внешних интеграций (1С и др.).
    /// Передаётся клиентом в заголовке X-Api-Key.
    /// Пустая строка = внешний API отключён.
    #[serde(default)]
    pub api_key: String,
}

impl Default for ExternalApiConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub database: DatabaseConfig,
    #[serde(default)]
    pub scheduled_tasks: ScheduledTasksConfig,
    #[serde(default)]
    pub llm: LlmConfig,
    #[serde(default)]
    pub quality: QualityConfig,
    #[serde(default)]
    pub external_api: ExternalApiConfig,
    #[serde(default)]
    pub s3: S3Config,
    #[serde(default)]
    pub mail: MailConfig,
    #[serde(default)]
    pub bitrix24: Bitrix24Config,
}

#[derive(Debug, Deserialize, Clone)]
pub struct QualityConfig {
    /// External quality-check packages. A package with the same id overrides
    /// the embedded definition after an atomic reload.
    #[serde(default = "default_quality_checks_path")]
    pub checks_path: String,
}

fn default_quality_checks_path() -> String {
    "quality_checks".to_string()
}

impl Default for QualityConfig {
    fn default() -> Self {
        Self {
            checks_path: default_quality_checks_path(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Bitrix24Config {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub webhook_url: String,
    #[serde(default = "default_bitrix_group_id")]
    pub group_id: i64,
    #[serde(default)]
    pub responsible_id: i64,
    #[serde(default = "default_true")]
    pub sync_comments: bool,
}

impl Default for Bitrix24Config {
    fn default() -> Self {
        Self {
            enabled: false,
            webhook_url: String::new(),
            group_id: default_bitrix_group_id(),
            responsible_id: 0,
            sync_comments: true,
        }
    }
}

impl Bitrix24Config {
    pub fn validate_ready(&self) -> anyhow::Result<()> {
        if !self.enabled {
            return Err(anyhow::anyhow!(
                "Bitrix24 is disabled ([bitrix24].enabled=false)"
            ));
        }
        if !self.webhook_url.trim().starts_with("https://") {
            return Err(anyhow::anyhow!(
                "[bitrix24].webhook_url must be a non-empty HTTPS URL"
            ));
        }
        if self.group_id <= 0 {
            return Err(anyhow::anyhow!("[bitrix24].group_id must be positive"));
        }
        if self.responsible_id <= 0 {
            return Err(anyhow::anyhow!(
                "[bitrix24].responsible_id must be positive"
            ));
        }
        Ok(())
    }
}

fn default_bitrix_group_id() -> i64 {
    5
}

/// Настройки почтового ящика для LLM (приём по IMAP, отправка по SMTP).
/// Секреты хранятся только в config.toml (в .gitignore), не в примере.
#[derive(Debug, Deserialize, Clone)]
pub struct MailConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_mail_imap_host")]
    pub imap_host: String,
    #[serde(default = "default_mail_imap_port")]
    pub imap_port: u16,
    #[serde(default = "default_mail_smtp_host")]
    pub smtp_host: String,
    #[serde(default = "default_mail_smtp_port")]
    pub smtp_port: u16,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub from_address: String,
    #[serde(default)]
    pub from_name: String,
    /// Лимит отправки: сколько писем можно отправить за окно `send_rate_window_secs`.
    #[serde(default = "default_mail_send_rate_limit")]
    pub send_rate_limit: usize,
    /// Ширина окна rate-limit в секундах (по умолчанию 1 час).
    #[serde(default = "default_mail_send_rate_window_secs")]
    pub send_rate_window_secs: u64,
    /// Базовый URL приложения для deep-link на чат/артефакт в ответных письмах.
    /// Пусто — ссылка не добавляется.
    #[serde(default)]
    pub base_url: String,
}

impl Default for MailConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            imap_host: default_mail_imap_host(),
            imap_port: default_mail_imap_port(),
            smtp_host: default_mail_smtp_host(),
            smtp_port: default_mail_smtp_port(),
            username: String::new(),
            password: String::new(),
            from_address: String::new(),
            from_name: String::new(),
            send_rate_limit: default_mail_send_rate_limit(),
            send_rate_window_secs: default_mail_send_rate_window_secs(),
            base_url: String::new(),
        }
    }
}

impl MailConfig {
    /// Проверяет, что почта включена и минимально сконфигурирована.
    pub fn validate_ready(&self) -> anyhow::Result<()> {
        if !self.enabled {
            return Err(anyhow::anyhow!(
                "Mail is disabled in config.toml ([mail].enabled = false)"
            ));
        }
        if self.username.trim().is_empty() {
            return Err(anyhow::anyhow!("[mail].username must be set"));
        }
        if self.password.trim().is_empty() {
            return Err(anyhow::anyhow!("[mail].password must be set"));
        }
        Ok(())
    }

    /// Адрес отправителя: берётся from_address, иначе username.
    pub fn sender_address(&self) -> &str {
        if self.from_address.trim().is_empty() {
            self.username.trim()
        } else {
            self.from_address.trim()
        }
    }
}

fn default_mail_imap_host() -> String {
    "mail.hosting.reg.ru".to_string()
}
fn default_mail_imap_port() -> u16 {
    993
}
fn default_mail_smtp_host() -> String {
    "mail.hosting.reg.ru".to_string()
}
fn default_mail_smtp_port() -> u16 {
    465
}
fn default_mail_send_rate_limit() -> usize {
    20
}
fn default_mail_send_rate_window_secs() -> u64 {
    3600
}

#[derive(Debug, Deserialize, Clone)]
pub struct S3Config {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_s3_endpoint")]
    pub endpoint: String,
    #[serde(default = "default_s3_region")]
    pub region: String,
    #[serde(default)]
    pub bucket: String,
    #[serde(default)]
    pub access_key_id: String,
    #[serde(default)]
    pub secret_access_key: String,
    #[serde(default = "default_s3_max_upload_mb")]
    pub max_upload_mb: u64,
}

impl Default for S3Config {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: default_s3_endpoint(),
            region: default_s3_region(),
            bucket: String::new(),
            access_key_id: String::new(),
            secret_access_key: String::new(),
            max_upload_mb: default_s3_max_upload_mb(),
        }
    }
}

impl S3Config {
    pub fn validate_ready(&self) -> anyhow::Result<()> {
        if !self.enabled {
            return Err(anyhow::anyhow!("S3 storage is disabled in config.toml"));
        }
        if self.bucket.trim().is_empty() {
            return Err(anyhow::anyhow!("[s3].bucket must be set"));
        }
        if self.access_key_id.trim().is_empty() {
            return Err(anyhow::anyhow!("[s3].access_key_id must be set"));
        }
        if self.secret_access_key.trim().is_empty() {
            return Err(anyhow::anyhow!("[s3].secret_access_key must be set"));
        }
        Ok(())
    }

    pub fn max_upload_bytes(&self) -> u64 {
        self.max_upload_mb.saturating_mul(1024).saturating_mul(1024)
    }
}

fn default_s3_endpoint() -> String {
    "https://storage.yandexcloud.net".to_string()
}

fn default_s3_region() -> String {
    "ru-central1".to_string()
}

fn default_s3_max_upload_mb() -> u64 {
    512
}

#[derive(Debug, Deserialize, Clone)]
pub struct LlmConfig {
    /// Путь к директории с MD-файлами базы знаний (Obsidian-формат).
    /// Относительный путь разрешается от директории бинарника.
    pub knowledge_base_path: String,
    /// Путь к директории с внешним каталогом навыков (`*.md` с frontmatter).
    /// Эти файлы дополняют/переопределяют встроенный набор навыков по `id`.
    /// Относительный путь разрешается от директории бинарника.
    #[serde(default = "default_skills_path")]
    pub skills_path: String,
    /// Путь к корню рабочих каталогов чатов (`<chat_id>/<NNN-активность>/…`).
    /// Здесь живут анкеты, планы и журнал шагов — состояние, которое должно
    /// переживать компакцию истории.
    /// Относительный путь разрешается от директории бинарника.
    #[serde(default = "default_chat_files_path")]
    pub chat_files_path: String,
    /// Путь к каталогу эталонных кейсов («голден-сет»): `<case-id>.md` с frontmatter.
    /// Прогоняется отдельной задачей после правок промптов и навыков, чтобы
    /// изменение качества было видно на неизменном наборе вопросов.
    /// Относительный путь разрешается от директории бинарника.
    #[serde(default = "default_golden_set_path")]
    pub golden_set_path: String,
}

fn default_skills_path() -> String {
    "skills".to_string()
}

fn default_chat_files_path() -> String {
    "chat_files".to_string()
}

fn default_golden_set_path() -> String {
    "golden_set".to_string()
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            knowledge_base_path: "data/knowledge".to_string(),
            skills_path: default_skills_path(),
            chat_files_path: default_chat_files_path(),
            golden_set_path: default_golden_set_path(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    #[serde(deserialize_with = "normalize_path")]
    pub path: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ScheduledTasksConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for ScheduledTasksConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// Нормализует пути Windows: конвертирует обратные слеши в прямые
fn normalize_path<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let path = String::deserialize(deserializer)?;

    // Проверяем, является ли это Windows путем (содержит двоеточие и слеши)
    if path.len() >= 3 && path.chars().nth(1) == Some(':') {
        // Это Windows абсолютный путь (C:\... или C:/...)
        Ok(path.replace('\\', "/"))
    } else {
        Ok(path)
    }
}

const CONFIG_FILE_NAME: &str = "config.toml";

fn default_true() -> bool {
    true
}

pub fn get_config_path() -> anyhow::Result<PathBuf> {
    let exe_path = std::env::current_exe()
        .map_err(|e| anyhow::anyhow!("Cannot determine executable path: {}", e))?;
    let exe_dir = exe_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine executable directory"))?;
    let config_path = exe_dir.join(CONFIG_FILE_NAME);

    if !config_path.exists() {
        return Err(anyhow::anyhow!(
            "Required config file not found: {}. Database path must be configured explicitly in config.toml.",
            config_path.display()
        ));
    }

    Ok(config_path)
}

fn validate_config(config: &Config) -> anyhow::Result<()> {
    let raw_db_path = config.database.path.trim();
    if raw_db_path.is_empty() {
        return Err(anyhow::anyhow!(
            "[database].path must be set in config.toml"
        ));
    }

    let db_path = Path::new(raw_db_path);
    if !db_path.is_absolute() {
        return Err(anyhow::anyhow!(
            "[database].path must be an absolute path. Got '{}'.",
            raw_db_path
        ));
    }

    Ok(())
}

/// Load configuration from required config.toml next to the executable.
pub fn load_config() -> anyhow::Result<Config> {
    let config_path = get_config_path()?;
    let contents = std::fs::read_to_string(&config_path)
        .map_err(|e| anyhow::anyhow!("Cannot read config file {}: {}", config_path.display(), e))?;
    let config: Config = toml::from_str(&contents)
        .map_err(|e| anyhow::anyhow!("Invalid TOML in {}: {}", config_path.display(), e))?;

    validate_config(&config)?;

    let database_path = get_database_path(&config)?;
    println!("\n========================================");
    println!("  CONFIGURATION LOADING DIAGNOSTICS");
    println!("========================================");
    println!("✓ Config file: {}", config_path.display());
    println!("✓ Database path: {}", database_path.display());
    println!(
        "✓ Scheduled task worker enabled: {}",
        config.scheduled_tasks.enabled
    );
    println!("========================================\n");

    tracing::info!("Config loaded from: {}", config_path.display());
    tracing::info!("Resolved database path: {}", database_path.display());
    Ok(config)
}

/// Get the database file path from configuration
pub fn get_database_path(config: &Config) -> anyhow::Result<PathBuf> {
    let db_path_str = &config.database.path;
    let db_path = Path::new(db_path_str);

    if !db_path.is_absolute() {
        return Err(anyhow::anyhow!(
            "[database].path must be absolute, got '{}'",
            db_path_str
        ));
    }

    Ok(db_path.to_path_buf())
}

/// Get the knowledge base directory path from configuration.
/// Resolves relative paths relative to the executable directory.
pub fn get_knowledge_base_path(config: &Config) -> PathBuf {
    resolve_relative_to_exe(&config.llm.knowledge_base_path)
}

/// Get the external skills catalog directory path from configuration.
/// Resolves relative paths relative to the executable directory (mirrors KB path).
pub fn get_skills_path(config: &Config) -> PathBuf {
    resolve_relative_to_exe(&config.llm.skills_path)
}

/// Каталог эталонных кейсов для регрессии качества LLM.
/// Resolves relative paths relative to the executable directory (mirrors KB path).
pub fn get_golden_set_path(config: &Config) -> PathBuf {
    resolve_relative_to_exe(&config.llm.golden_set_path)
}

/// Корень рабочих каталогов чатов (анкеты, планы, журнал шагов).
/// Resolves relative paths relative to the executable directory (mirrors KB path).
pub fn get_chat_files_path(config: &Config) -> PathBuf {
    resolve_relative_to_exe(&config.llm.chat_files_path)
}

/// External quality-check package root.
pub fn get_quality_checks_path(config: &Config) -> PathBuf {
    resolve_relative_to_exe(&config.quality.checks_path)
}

/// Абсолютный путь — как есть; относительный — от директории бинарника.
fn resolve_relative_to_exe(raw: &str) -> PathBuf {
    let p = Path::new(raw);
    if p.is_absolute() {
        return p.to_path_buf();
    }
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            return exe_dir.join(p);
        }
    }
    PathBuf::from(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_database_path_is_accepted() {
        let config: Config = toml::from_str(
            r#"
[database]
path = "C:/tmp/mpi-test/app.db"

[scheduled_tasks]
enabled = true

[llm]
knowledge_base_path = "data/knowledge"
"#,
        )
        .unwrap();

        assert!(validate_config(&config).is_ok());
        assert_eq!(
            get_database_path(&config).unwrap(),
            PathBuf::from("C:/tmp/mpi-test/app.db")
        );
        assert!(config.scheduled_tasks.enabled);
    }

    #[test]
    fn relative_database_path_is_rejected() {
        let config: Config = toml::from_str(
            r#"
[database]
path = "target/db/app.db"

[scheduled_tasks]
enabled = true

[llm]
knowledge_base_path = "data/knowledge"
"#,
        )
        .unwrap();

        assert!(validate_config(&config).is_err());
        assert!(get_database_path(&config).is_err());
    }
}
