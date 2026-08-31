//! Реестр регистраторов — единственная точка правды о том, что система умеет
//! делать с документом, зная только его `registrator_type`.
//!
//! **Зачем.** До этого один и тот же ключ разбирался семью параллельными
//! `match` в четырёх файлах: `shared::representation` знал 16 типов,
//! `quality::checks::registrator_registry` — 8, `u508::dispatch_repost` — 9,
//! `api::handlers::dashboards` — 3. Списки уже разошлись, и добавление
//! проводимого документа требовало не забыть про все семь мест. Хуже того,
//! ключи приходили из **двух пространств**: Главная книга, `p909` и `p914`
//! пишут канонические `a012_wb_sales`, а `p904_sales_data` — исторические
//! `WB_Sales` / `YM_Order` / `OZON_FBS`. Соответствие между ними жило только
//! в головах.
//!
//! **Как теперь.** Срез объявляет один [`Registrator`] рядом со своим кодом
//! (роль `representation.rs`), перечисляет свои легаси-ключи в
//! [`Registrator::aliases`], а сборка списка живёт в `composition::registrators`.
//! Ядро больше не называет агрегаты по имени — это условие, при котором
//! маркетплейсы можно вынести в отдельный крейт.
//!
//! **Регистрация обязательна до первого запроса.** Реестр — `OnceLock`;
//! `composition::install_all()` вызывается в начале `main`, до сборки роутера.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use contracts::general_ledger::AggregateRepresentation;
use contracts::quality::SourceColumn;
use uuid::Uuid;

/// Статические свойства типа регистратора: как его назвать и что с ним можно.
pub struct RegistratorMeta {
    /// Полное название типа, напр. `"Реклама WB (день)"`.
    pub type_label: &'static str,
    /// Короткая подпись ссылки в дашбордах, напр. `"Реклама"`.
    /// `None` — ссылку на документ не показываем.
    pub link_label: Option<&'static str>,
    /// `true` — для типа доступно перепроведение.
    pub can_post: bool,
    /// Префикс tab-ключа карточки во фронтенде,
    /// напр. `Some("a026_wb_advert_daily_details")`.
    pub tab_key_prefix: Option<&'static str>,
}

impl RegistratorMeta {
    /// Метаданные неизвестного типа: показать «Документ», ничего не уметь.
    pub const UNKNOWN: RegistratorMeta = RegistratorMeta {
        type_label: "Документ",
        link_label: None,
        can_post: false,
        tab_key_prefix: None,
    };
}

/// Паспорт агрегата для страницы перепроведения `u508`.
pub struct RepostOption {
    /// Подпись пункта, напр. `"a015 — WB Orders"`.
    pub label: &'static str,
    /// Что именно произойдёт — текст показывается пользователю до запуска.
    pub description: &'static str,
}

/// Один тип регистратора: агрегат или проекция, на которую ссылаются проводки.
///
/// Обязателен только паспорт — [`kind`](Self::kind), [`meta`](Self::meta) и
/// [`table`](Self::table). Остальное включается по мере того, как срез это
/// действительно умеет: тип, который не проводится, не реализует
/// [`post_document`](Self::post_document), и `can_post` у него `false`.
#[async_trait]
pub trait Registrator: Send + Sync {
    /// Канонический ключ. Совпадает с именем каталога среза (`a012_wb_sales`).
    fn kind(&self) -> &'static str;

    /// Исторические ключи того же типа — те, что лежат в `p904_sales_data`
    /// (`WB_Sales`, `YM_Order`, `OZON_FBS`…). Реестр индексирует их наравне
    /// с [`kind`](Self::kind), поэтому старые данные резолвятся без миграции.
    fn aliases(&self) -> &'static [&'static str] {
        &[]
    }

    fn meta(&self) -> RegistratorMeta;

    /// Физическая таблица документа — нужна проверке существования.
    /// У всех нынешних типов совпадает с [`kind`](Self::kind); переопределяется
    /// тем, у кого разойдётся.
    fn table(&self) -> &'static str {
        self.kind()
    }

    /// Батч-резолв представлений (название + дата + номер) по id.
    /// По умолчанию пусто: тип участвует в проводках, но карточки не имеет.
    async fn represent_many(&self, _ids: &[String]) -> HashMap<String, AggregateRepresentation> {
        HashMap::new()
    }

    /// Провести документ заново.
    async fn post_document(&self, _id: Uuid) -> Result<()> {
        Err(anyhow!(
            "Тип регистратора '{}' не поддерживает перепроведение",
            self.kind()
        ))
    }

    /// Колонки исходного документа для drill-down по нарушению quality-check.
    /// Пусто — UI покажет базовые колонки.
    async fn source_columns(&self, _registrator_ref: &str) -> Vec<SourceColumn> {
        Vec::new()
    }

    /// Паспорт для страницы перепроведения `u508`.
    /// `None` — агрегат не перепроводится оптом за период.
    fn repost_option(&self) -> Option<RepostOption> {
        None
    }

    /// Id документов за период — вход для перепроведения оптом.
    async fn ids_in_period(
        &self,
        _date_from: &str,
        _date_to: &str,
        _only_posted: bool,
    ) -> Result<Vec<String>> {
        Err(anyhow!(
            "Тип регистратора '{}' не умеет отбирать документы за период",
            self.kind()
        ))
    }
}

/// Собранный реестр: порядок установки сохраняется (по нему строятся списки в
/// UI), поиск идёт и по каноническому ключу, и по каждому алиасу.
struct Registry {
    ordered: Vec<Arc<dyn Registrator>>,
    by_key: HashMap<&'static str, Arc<dyn Registrator>>,
}

static REGISTRY: OnceLock<Registry> = OnceLock::new();

/// Установить реестр. Зовётся один раз из `composition::install_all()`.
///
/// # Panics
/// При повторной установке и при конфликте ключей. Второй список означал бы,
/// что часть системы работает с другим набором типов — это не то, что стоит
/// обнаруживать по расхождению отчётов.
pub fn install(registrators: Vec<Arc<dyn Registrator>>) {
    let mut by_key: HashMap<&'static str, Arc<dyn Registrator>> = HashMap::new();
    for registrator in &registrators {
        let mut keys = vec![registrator.kind()];
        keys.extend_from_slice(registrator.aliases());
        for key in keys {
            if let Some(previous) = by_key.insert(key, Arc::clone(registrator)) {
                panic!(
                    "ключ регистратора '{key}' заявлен дважды: '{}' и '{}'",
                    previous.kind(),
                    registrator.kind()
                );
            }
        }
    }

    if REGISTRY
        .set(Registry {
            ordered: registrators,
            by_key,
        })
        .is_err()
    {
        panic!("реестр регистраторов уже установлен");
    }
}

fn registry() -> &'static Registry {
    REGISTRY
        .get()
        .expect("реестр регистраторов не установлен: composition::install_all() не был вызван")
}

/// Регистратор по каноническому ключу или алиасу. `None` — тип неизвестен.
pub fn find(kind: &str) -> Option<&'static Arc<dyn Registrator>> {
    registry().by_key.get(kind)
}

/// Все регистраторы в порядке установки.
pub fn all() -> &'static [Arc<dyn Registrator>] {
    &registry().ordered
}

/// Метаданные типа. Для неизвестного — [`RegistratorMeta::UNKNOWN`],
/// а не ошибка: в проводках встречаются типы, снятые с поддержки.
pub fn meta(kind: &str) -> RegistratorMeta {
    find(kind).map_or(RegistratorMeta::UNKNOWN, |registrator| registrator.meta())
}

/// Id документа из `registrator_ref` вида `"a026:<uuid>"` или голого uuid.
pub fn document_id(registrator_ref: &str) -> &str {
    registrator_ref
        .split_once(':')
        .map(|(_, id)| id)
        .unwrap_or(registrator_ref)
}

/// Существует ли исходный документ. Неизвестный тип — `false`, не ошибка.
pub async fn source_document_exists(kind: &str, registrator_ref: &str) -> Result<bool> {
    use sea_orm::{ConnectionTrait, Statement};

    let Some(registrator) = find(kind) else {
        return Ok(false);
    };

    let sql = format!(
        "SELECT COUNT(*) AS cnt FROM {} WHERE id = ?",
        registrator.table()
    );
    let rows = crate::shared::data::db::get_connection()
        .query_all(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            &sql,
            [document_id(registrator_ref).into()],
        ))
        .await?;
    let count: i64 = rows
        .first()
        .and_then(|row| row.try_get("", "cnt").ok())
        .unwrap_or(0);

    Ok(count > 0)
}

/// Колонки исходного документа для drill-down по нарушению quality-check.
/// Для типа без шаблона — пусто: UI покажет базовые колонки.
pub async fn source_columns(kind: &str, registrator_ref: &str) -> Vec<SourceColumn> {
    match find(kind) {
        Some(registrator) => registrator.source_columns(registrator_ref).await,
        None => Vec::new(),
    }
}

/// Перепровести документ по `registrator_ref` (с префиксом типа или без).
pub async fn repost_document(kind: &str, registrator_ref: &str) -> Result<()> {
    let registrator = find(kind).ok_or_else(|| anyhow!("неизвестный тип регистратора: {kind}"))?;
    let raw = document_id(registrator_ref);
    let id = Uuid::parse_str(raw)
        .map_err(|error| anyhow!("некорректный registrator_ref '{registrator_ref}': {error}"))?;
    registrator.post_document(id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Регистратор, объявивший только паспорт: всё остальное — умолчания трейта.
    struct Bare;

    #[async_trait]
    impl Registrator for Bare {
        fn kind(&self) -> &'static str {
            "a999_bare"
        }

        fn meta(&self) -> RegistratorMeta {
            RegistratorMeta::UNKNOWN
        }
    }

    #[test]
    fn document_id_strips_the_type_prefix() {
        // Проекции пишут ссылку двумя способами: p909 — с префиксом типа,
        // p911 — голым uuid. Обе формы обязаны давать один и тот же id.
        assert_eq!(
            document_id("a012:0000e9e7-1111-2222-3333-444455556666"),
            "0000e9e7-1111-2222-3333-444455556666"
        );
        assert_eq!(
            document_id("0000478f-1111-2222-3333-444455556666"),
            "0000478f-1111-2222-3333-444455556666"
        );
    }

    #[test]
    fn table_defaults_to_kind() {
        assert_eq!(Bare.table(), "a999_bare");
    }

    /// Тип без реализации не «проводится молча»: отказ называет тип, иначе
    /// в журнале перепроведения остаётся ошибка без адресата.
    #[tokio::test]
    async fn default_post_document_refuses_and_names_the_kind() {
        let error = Bare
            .post_document(Uuid::nil())
            .await
            .expect_err("умолчание обязано отказывать");
        assert!(error.to_string().contains("a999_bare"), "{error}");
    }

    #[tokio::test]
    async fn defaults_are_empty_rather_than_failing() {
        assert!(Bare.represent_many(&["x".to_string()]).await.is_empty());
        assert!(Bare.source_columns("x").await.is_empty());
        assert!(Bare.repost_option().is_none());
        assert!(Bare.aliases().is_empty());
    }

    /// Умолчание паспорта — «Документ» без ссылки и без права проведения.
    /// На нём держится ответ для типов, снятых с поддержки.
    #[test]
    fn unknown_meta_promises_nothing() {
        let meta = RegistratorMeta::UNKNOWN;
        assert_eq!(meta.type_label, "Документ");
        assert!(meta.link_label.is_none());
        assert!(!meta.can_post);
        assert!(meta.tab_key_prefix.is_none());
    }

    #[tokio::test]
    async fn ids_in_period_refuses_when_the_slice_does_not_select() {
        let error = Bare
            .ids_in_period("2026-01-01", "2026-01-31", true)
            .await
            .expect_err("умолчание обязано отказывать");
        assert!(error.to_string().contains("a999_bare"), "{error}");
    }
}
