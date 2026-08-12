use contracts::system::datasets::{InstanceEnv, PathOrigin};
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
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub scheduled_tasks: ScheduledTasksConfig,
    #[serde(default)]
    pub data: DataConfig,
    #[serde(default)]
    pub instance: InstanceConfig,
    #[serde(default)]
    pub datasets: DatasetsConfig,
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

/// Единый корень файловых данных инстанса. Когда задан, все каталоги данных
/// живут под ним с фиксированными именами (см. константы `SUBDIR_*`), что
/// делает раскладку одинаковой на всех инстансах и позволяет переносить наборы
/// между ними. Пустое значение сохраняет историческое поведение: каждый путь
/// берётся из своей секции конфига.
#[derive(Debug, Deserialize, Clone, Default)]
pub struct DataConfig {
    #[serde(default, deserialize_with = "normalize_path")]
    pub root: String,
}

/// Имена подкаталогов внутри `[data].root`. Фиксированы в коде: раскладка
/// должна совпадать на инстансе-доноре и инстансе-получателе.
pub const SUBDIR_DB: &str = "db";
pub const SUBDIR_KNOWLEDGE: &str = "knowledge";
pub const SUBDIR_SKILLS: &str = "skills";
pub const SUBDIR_CHATS: &str = "chats";
pub const SUBDIR_GOLDEN_SET: &str = "golden_set";
pub const SUBDIR_QUALITY_CHECKS: &str = "quality_checks";
pub const SUBDIR_ATTACHMENTS: &str = "attachments";
pub const SUBDIR_BACKUPS: &str = "backups";
pub const SUBDIR_TMP: &str = "tmp";

/// Имя файла базы внутри `<root>/db`, когда `[database].path` не задан.
pub const DB_FILE_NAME: &str = "app.db";

/// Идентичность экземпляра приложения. Попадает в манифест каждого снапшота,
/// чтобы на принимающей стороне было видно, откуда приехали данные.
#[derive(Debug, Deserialize, Clone, Default)]
pub struct InstanceConfig {
    /// Стабильный slug, попадает в S3-ключи. Пусто → hostname машины.
    #[serde(default)]
    pub id: String,
    /// Человекочитаемое имя для UI. Пусто → `id`.
    #[serde(default)]
    pub label: String,
    /// `production` | `staging` | `dev`. Нераспознанное → «не задан».
    #[serde(default)]
    pub env: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatasetsConfig {
    /// Сколько последних снапшотов ЭТОГО инстанса хранить в S3. Снапшоты чужих
    /// инстансов ротация не трогает никогда.
    #[serde(default = "default_datasets_keep")]
    pub keep_per_instance: usize,
    /// Потолок размера zip-бандла ФАЙЛОВЫХ наборов. Дополнительно ограничен
    /// `[s3].max_upload_mb` — этот объект по-прежнему собирается в памяти целиком.
    #[serde(default = "default_datasets_max_bundle_mb")]
    pub max_bundle_mb: u64,
    /// Отдельный потолок для снапшота БД. Под `max_bundle_mb` он не попадает:
    /// объект грузится потоком по частям, в память целиком не читается, и мерить
    /// его лимитом in-memory бандла бессмысленно — боевая база уже больше него.
    #[serde(default = "default_datasets_max_db_gb")]
    pub max_db_gb: u64,
    /// Разрешить включать вложения чатов в снапшоты. Выключено по умолчанию:
    /// единственный набор, способный вырасти до сотен мегабайт.
    #[serde(default)]
    pub attachments_enabled: bool,
}

fn default_datasets_keep() -> usize {
    10
}

fn default_datasets_max_bundle_mb() -> u64 {
    256
}

fn default_datasets_max_db_gb() -> u64 {
    20
}

impl Default for DatasetsConfig {
    fn default() -> Self {
        Self {
            keep_per_instance: default_datasets_keep(),
            max_bundle_mb: default_datasets_max_bundle_mb(),
            max_db_gb: default_datasets_max_db_gb(),
            attachments_enabled: false,
        }
    }
}

impl DatasetsConfig {
    /// Эффективный потолок zip-бандла файловых наборов с учётом лимита загрузки в S3.
    pub fn max_bundle_bytes(&self, s3: &S3Config) -> u64 {
        let own = self.max_bundle_mb.saturating_mul(1024).saturating_mul(1024);
        own.min(s3.max_upload_bytes())
    }

    /// Потолок для объекта снапшота БД (до сжатия).
    pub fn max_db_bytes(&self) -> u64 {
        self.max_db_gb
            .saturating_mul(1024)
            .saturating_mul(1024)
            .saturating_mul(1024)
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
    /// Пусто — `<[data].root>/knowledge`.
    #[serde(default = "default_knowledge_base_path")]
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
    /// Путь к корню файловых вложений чатов (`<chat_id>/<uuid>.<ext>`).
    /// Исторически вложения лежали в `./uploads/chat_attachments` относительно
    /// рабочего каталога процесса — то есть «переезжали» при запуске бэкенда из
    /// другой директории. Относительный путь разрешается от директории бинарника.
    #[serde(default = "default_attachments_path")]
    pub attachments_path: String,
}

fn default_knowledge_base_path() -> String {
    SUBDIR_KNOWLEDGE.to_string()
}

fn default_skills_path() -> String {
    SUBDIR_SKILLS.to_string()
}

fn default_chat_files_path() -> String {
    SUBDIR_CHATS.to_string()
}

fn default_golden_set_path() -> String {
    SUBDIR_GOLDEN_SET.to_string()
}

fn default_attachments_path() -> String {
    SUBDIR_ATTACHMENTS.to_string()
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            knowledge_base_path: default_knowledge_base_path(),
            skills_path: default_skills_path(),
            chat_files_path: default_chat_files_path(),
            golden_set_path: default_golden_set_path(),
            attachments_path: default_attachments_path(),
        }
    }
}

/// Путь к файлу БД. Пустое значение — основной режим: база живёт в
/// `<[data].root>/db/app.db`. Непустое значение обязано быть абсолютным и
/// выводит базу из-под общего корня (см. `resolve_database_path`).
#[derive(Debug, Deserialize, Clone, Default)]
pub struct DatabaseConfig {
    #[serde(default, deserialize_with = "normalize_path")]
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
    let raw_root = config.data.root.trim();
    if !raw_root.is_empty() && !Path::new(raw_root).is_absolute() {
        return Err(anyhow::anyhow!(
            "[data].root must be an absolute path. Got '{}'.",
            raw_root
        ));
    }

    // Путь к базе либо выводится из корня, либо задан явно и абсолютно.
    get_database_path(config)?;

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

    let database = resolve_database_path(&config)?;

    // Конфиг перечитывается с диска на каждый вызов (обработчики, задачи), а
    // раскладка за время работы процесса не меняется — печатаем её один раз.
    static DIAGNOSTICS_PRINTED: OnceLock<()> = OnceLock::new();
    if DIAGNOSTICS_PRINTED.set(()).is_ok() {
        println!("\n========================================");
        println!("  CONFIGURATION LOADING DIAGNOSTICS");
        println!("========================================");
        println!("✓ Config file: {}", config_path.display());
        println!(
            "✓ Data root: {}",
            get_data_root(&config)
                .map(|root| root.display().to_string())
                .unwrap_or_else(|| "(не задан)".to_string())
        );
        println!("✓ Database path: {}", database.path.display());
        // Каталоги, выведенные из корня, перечислять незачем — интерес
        // представляют только отклонения от основного режима.
        for (label, resolved) in resolved_data_paths(&config) {
            if resolved.origin != PathOrigin::DataRoot {
                println!(
                    "! {label}: {} ({})",
                    resolved.path.display(),
                    resolved.origin.label_ru()
                );
            }
        }
        println!(
            "✓ Scheduled task worker enabled: {}",
            config.scheduled_tasks.enabled
        );
        println!("========================================\n");

        tracing::info!("Config loaded from: {}", config_path.display());
        tracing::info!("Resolved database path: {}", database.path.display());
    }

    Ok(config)
}

/// Все каталоги данных с их источником — для диагностики при старте и в UI.
pub fn resolved_data_paths(config: &Config) -> Vec<(&'static str, ResolvedPath)> {
    let mut paths = vec![
        ("knowledge", resolve_knowledge_base_path(config)),
        ("skills", resolve_skills_path(config)),
        ("chats", resolve_chat_files_path(config)),
        ("golden_set", resolve_golden_set_path(config)),
        ("quality_checks", resolve_quality_checks_path(config)),
        ("attachments", resolve_attachments_path(config)),
    ];
    if let Ok(database) = resolve_database_path(config) {
        paths.insert(0, ("database", database));
    }
    paths
}

/// Путь к файлу БД вместе с тем, откуда он взялся.
///
/// Основной режим — база под общим корнем: `<[data].root>/db/app.db`. Явный
/// `[database].path` — отклонение: обязан быть абсолютным (иначе база «переезжает»
/// вслед за рабочим каталогом процесса) и уводит файл из-под корня, из-за чего
/// он выпадает из общей раскладки и переноса наборов.
pub fn resolve_database_path(config: &Config) -> anyhow::Result<ResolvedPath> {
    let raw = config.database.path.trim();

    if !raw.is_empty() {
        if !Path::new(raw).is_absolute() {
            return Err(anyhow::anyhow!(
                "[database].path must be absolute, got '{}'. Оставьте его пустым, \
                 чтобы база легла в <[data].root>/{}/{}.",
                raw,
                SUBDIR_DB,
                DB_FILE_NAME
            ));
        }
        return Ok(ResolvedPath {
            path: PathBuf::from(raw),
            origin: PathOrigin::LegacyOverride,
            raw: raw.to_string(),
        });
    }

    let root = get_data_root(config).ok_or_else(|| {
        anyhow::anyhow!(
            "Не задан ни [data].root, ни [database].path — непонятно, где лежит база. \
             Укажите [data].root (основной режим)."
        )
    })?;

    Ok(ResolvedPath {
        path: root.join(SUBDIR_DB).join(DB_FILE_NAME),
        origin: PathOrigin::DataRoot,
        raw: format!("{SUBDIR_DB}/{DB_FILE_NAME}"),
    })
}

/// Get the database file path from configuration
pub fn get_database_path(config: &Config) -> anyhow::Result<PathBuf> {
    resolve_database_path(config).map(|resolved| resolved.path)
}

/// Путь к каталогу данных вместе с тем, откуда он взялся. `origin` — часть
/// диагностики: в UI видно, живёт ли каталог под общим `[data].root` или
/// «выбивается» историческим абсолютным путём в своей секции конфига.
#[derive(Debug, Clone)]
pub struct ResolvedPath {
    pub path: PathBuf,
    pub origin: PathOrigin,
    /// Сырое значение из конфига, как его написал оператор.
    pub raw: String,
}

impl ResolvedPath {
    pub fn display_path(&self) -> String {
        self.path.display().to_string()
    }
}

/// Единый корень данных, если он задан.
pub fn get_data_root(config: &Config) -> Option<PathBuf> {
    let raw = config.data.root.trim();
    if raw.is_empty() {
        return None;
    }
    Some(PathBuf::from(raw))
}

/// Резолюция каталога данных в три уровня, в порядке убывания приоритета:
///
/// 1. **абсолютный** путь в своей секции конфига → как есть (`LegacyOverride`,
///    в UI — «индивидуальный путь»). Это отклонение от основного режима: каталог
///    выходит из-под общего корня. Ветка также сохраняет старые конфиги рабочими;
/// 2. задан `[data].root` → `<root>/<путь>` (`DataRoot`) — основной режим, где
///    `<путь>` — сырое значение из секции, а при пустом значении (норма)
///    фиксированное имя подкаталога;
/// 3. иначе → относительно директории бинарника (`ExeRelative`) — только для
///    конфигов без `[data].root`, которые старт уже отвергает по базе.
fn resolve_dataset_dir(config: &Config, raw: &str, default_subdir: &str) -> ResolvedPath {
    let trimmed = raw.trim();
    if !trimmed.is_empty() && Path::new(trimmed).is_absolute() {
        return ResolvedPath {
            path: PathBuf::from(trimmed),
            origin: PathOrigin::LegacyOverride,
            raw: trimmed.to_string(),
        };
    }

    let relative = if trimmed.is_empty() {
        default_subdir
    } else {
        trimmed
    };

    if let Some(root) = get_data_root(config) {
        return ResolvedPath {
            path: root.join(relative),
            origin: PathOrigin::DataRoot,
            raw: relative.to_string(),
        };
    }

    ResolvedPath {
        path: resolve_relative_to_exe(relative),
        origin: PathOrigin::ExeRelative,
        raw: relative.to_string(),
    }
}

pub fn resolve_knowledge_base_path(config: &Config) -> ResolvedPath {
    resolve_dataset_dir(config, &config.llm.knowledge_base_path, SUBDIR_KNOWLEDGE)
}

pub fn resolve_skills_path(config: &Config) -> ResolvedPath {
    resolve_dataset_dir(config, &config.llm.skills_path, SUBDIR_SKILLS)
}

pub fn resolve_golden_set_path(config: &Config) -> ResolvedPath {
    resolve_dataset_dir(config, &config.llm.golden_set_path, SUBDIR_GOLDEN_SET)
}

pub fn resolve_chat_files_path(config: &Config) -> ResolvedPath {
    resolve_dataset_dir(config, &config.llm.chat_files_path, SUBDIR_CHATS)
}

pub fn resolve_quality_checks_path(config: &Config) -> ResolvedPath {
    resolve_dataset_dir(config, &config.quality.checks_path, SUBDIR_QUALITY_CHECKS)
}

pub fn resolve_attachments_path(config: &Config) -> ResolvedPath {
    resolve_dataset_dir(config, &config.llm.attachments_path, SUBDIR_ATTACHMENTS)
}

/// Каталог локальных архивов (в т.ч. автоснапшотов перед восстановлением).
pub fn get_backups_path(config: &Config) -> PathBuf {
    resolve_dataset_dir(config, "", SUBDIR_BACKUPS).path
}

/// Рабочий каталог для промежуточных файлов подсистем данных.
pub fn get_tmp_path(config: &Config) -> PathBuf {
    resolve_dataset_dir(config, "", SUBDIR_TMP).path
}

/// Get the knowledge base directory path from configuration.
pub fn get_knowledge_base_path(config: &Config) -> PathBuf {
    resolve_knowledge_base_path(config).path
}

/// Get the external skills catalog directory path from configuration.
pub fn get_skills_path(config: &Config) -> PathBuf {
    resolve_skills_path(config).path
}

/// Каталог эталонных кейсов для регрессии качества LLM.
pub fn get_golden_set_path(config: &Config) -> PathBuf {
    resolve_golden_set_path(config).path
}

/// Корень рабочих каталогов чатов (анкеты, планы, журнал шагов).
pub fn get_chat_files_path(config: &Config) -> PathBuf {
    resolve_chat_files_path(config).path
}

/// External quality-check package root.
pub fn get_quality_checks_path(config: &Config) -> PathBuf {
    resolve_quality_checks_path(config).path
}

/// Корень файловых вложений чатов.
pub fn get_attachments_path(config: &Config) -> PathBuf {
    resolve_attachments_path(config).path
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

// ---------------------------------------------------------------------------
// Идентичность инстанса
// ---------------------------------------------------------------------------

/// Кто мы — попадает в манифест каждого снапшота наборов данных.
#[derive(Debug, Clone)]
pub struct InstanceIdentity {
    pub id: String,
    pub label: String,
    pub env: InstanceEnv,
    pub hostname: String,
}

/// Имя машины. Кешируется: `load_config()` перечитывает файл при каждом вызове,
/// и платить за внешний процесс на каждое обращение незачем.
pub fn hostname() -> &'static str {
    static HOSTNAME: OnceLock<String> = OnceLock::new();
    HOSTNAME.get_or_init(|| {
        if let Ok(name) = std::env::var("COMPUTERNAME") {
            if !name.trim().is_empty() {
                return name.trim().to_string();
            }
        }
        if let Ok(name) = std::env::var("HOSTNAME") {
            if !name.trim().is_empty() {
                return name.trim().to_string();
            }
        }
        std::process::Command::new("hostname")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "unknown".to_string())
    })
}

/// Приводит произвольную строку к безопасному для S3-ключа виду.
fn sanitize_instance_id(raw: &str) -> String {
    let slug: String = raw
        .trim()
        .to_lowercase()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "unknown".to_string()
    } else {
        slug.chars().take(64).collect()
    }
}

/// Идентичность инстанса с падением на hostname, если `[instance]` не заполнена.
pub fn instance_identity(config: &Config) -> InstanceIdentity {
    let host = hostname();
    let id = if config.instance.id.trim().is_empty() {
        sanitize_instance_id(host)
    } else {
        sanitize_instance_id(&config.instance.id)
    };
    let label = if config.instance.label.trim().is_empty() {
        id.clone()
    } else {
        config.instance.label.trim().to_string()
    };
    InstanceIdentity {
        id,
        label,
        env: InstanceEnv::from(config.instance.env.as_str()),
        hostname: host.to_string(),
    }
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
    fn database_path_is_derived_from_data_root() {
        // Основной режим: единственная настройка пути — [data].root.
        let config: Config = toml::from_str(
            r#"
[data]
root = "F:/data/app"
"#,
        )
        .unwrap();

        assert!(validate_config(&config).is_ok());
        let resolved = resolve_database_path(&config).unwrap();
        assert_eq!(resolved.path, PathBuf::from("F:/data/app/db/app.db"));
        assert_eq!(resolved.origin, PathOrigin::DataRoot);
    }

    #[test]
    fn explicit_database_path_overrides_data_root() {
        let config: Config = toml::from_str(
            r#"
[database]
path = "D:/legacy/app.db"

[data]
root = "F:/data/app"
"#,
        )
        .unwrap();

        let resolved = resolve_database_path(&config).unwrap();
        assert_eq!(resolved.path, PathBuf::from("D:/legacy/app.db"));
        assert_eq!(resolved.origin, PathOrigin::LegacyOverride);
    }

    #[test]
    fn config_without_root_and_without_database_path_is_rejected() {
        let config: Config = toml::from_str("[scheduled_tasks]\nenabled = true\n").unwrap();
        assert!(validate_config(&config).is_err());
        assert!(get_database_path(&config).is_err());
    }

    #[test]
    fn relative_data_root_is_rejected() {
        // Относительный корень «переезжал» бы вслед за рабочим каталогом процесса.
        let config: Config = toml::from_str("[data]\nroot = \"data\"\n").unwrap();
        assert!(validate_config(&config).is_err());
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

    fn parse(extra: &str) -> Config {
        let text = format!(
            r#"
[database]
path = "C:/tmp/mpi-test/app.db"

[llm]
knowledge_base_path = "{knowledge}"
{extra}
"#,
            knowledge = "",
            extra = extra
        );
        toml::from_str(&text).unwrap()
    }

    #[test]
    fn absolute_legacy_override_wins_over_data_root() {
        // Боевые конфиги с абсолютными путями обязаны продолжать работать
        // байт-в-байт после появления [data].root.
        let config = parse(
            r#"skills_path = "F:/legacy/skills"

[data]
root = "F:/data/app"
"#,
        );

        let resolved = resolve_skills_path(&config);
        assert_eq!(resolved.path, PathBuf::from("F:/legacy/skills"));
        assert_eq!(resolved.origin, PathOrigin::LegacyOverride);
    }

    #[test]
    fn data_root_supplies_fixed_subdirectories() {
        let config = parse(
            r#"
[data]
root = "F:/data/app"
"#,
        );

        for (resolved, subdir) in [
            (resolve_skills_path(&config), SUBDIR_SKILLS),
            (resolve_chat_files_path(&config), SUBDIR_CHATS),
            (resolve_golden_set_path(&config), SUBDIR_GOLDEN_SET),
            (resolve_attachments_path(&config), SUBDIR_ATTACHMENTS),
            (resolve_quality_checks_path(&config), SUBDIR_QUALITY_CHECKS),
            (resolve_knowledge_base_path(&config), SUBDIR_KNOWLEDGE),
        ] {
            assert_eq!(resolved.path, PathBuf::from("F:/data/app").join(subdir));
            assert_eq!(resolved.origin, PathOrigin::DataRoot);
        }
    }

    #[test]
    fn relative_override_is_kept_under_data_root() {
        let config = parse(
            r#"skills_path = "custom_skills"

[data]
root = "F:/data/app"
"#,
        );

        let resolved = resolve_skills_path(&config);
        assert_eq!(resolved.path, PathBuf::from("F:/data/app/custom_skills"));
        assert_eq!(resolved.origin, PathOrigin::DataRoot);
    }

    #[test]
    fn without_data_root_paths_stay_exe_relative() {
        let config = parse("");
        let resolved = resolve_skills_path(&config);
        assert_eq!(resolved.origin, PathOrigin::ExeRelative);
        assert!(resolved.path.ends_with(SUBDIR_SKILLS));
    }

    #[test]
    fn chat_files_default_points_at_the_live_directory() {
        // Исторический дефолт "chat_files" осиротел после переезда на "chats":
        // каталог на диске остался, но приложение в него больше не смотрит.
        assert_eq!(default_chat_files_path(), SUBDIR_CHATS);
    }

    #[test]
    fn instance_identity_falls_back_to_hostname() {
        let config = parse("");
        let identity = instance_identity(&config);
        assert_eq!(identity.id, sanitize_instance_id(hostname()));
        assert_eq!(identity.label, identity.id);
        assert_eq!(identity.env, InstanceEnv::Unknown);
    }

    #[test]
    fn instance_identity_uses_explicit_values() {
        let config = parse(
            r#"
[instance]
id = "Prod 01"
label = "Рабочий"
env = "production"
"#,
        );

        let identity = instance_identity(&config);
        assert_eq!(identity.id, "prod-01");
        assert_eq!(identity.label, "Рабочий");
        assert_eq!(identity.env, InstanceEnv::Production);
        // hostname пишется всегда: два инстанса с одинаковым id, но разными
        // машинами — признак скопированного и не поправленного config.toml.
        assert_eq!(identity.hostname, hostname());
    }

    #[test]
    fn max_bundle_bytes_is_capped_by_s3_upload_limit() {
        let datasets = DatasetsConfig {
            max_bundle_mb: 256,
            ..DatasetsConfig::default()
        };
        let s3 = S3Config {
            max_upload_mb: 64,
            ..S3Config::default()
        };
        assert_eq!(datasets.max_bundle_bytes(&s3), 64 * 1024 * 1024);
    }

    /// Лимит снимка БД живёт отдельно: он не должен подрезаться ни
    /// `max_bundle_mb`, ни `[s3].max_upload_mb` — объект грузится по частям.
    #[test]
    fn max_db_bytes_ignores_the_in_memory_bundle_limits() {
        let datasets = DatasetsConfig {
            max_bundle_mb: 256,
            max_db_gb: 20,
            ..DatasetsConfig::default()
        };
        assert_eq!(datasets.max_db_bytes(), 20 * 1024 * 1024 * 1024);
        assert!(datasets.max_db_bytes() > datasets.max_bundle_bytes(&S3Config::default()));
    }
}
