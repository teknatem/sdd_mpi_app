//! Сервис представлений агрегатов.
//!
//! Возвращает человекочитаемое представление (наименование + дата + id/номер)
//! по паре (тип регистратора, id). Логика владения — в модуле каждого агрегата
//! (`<module>::representation`), выбор владельца — в реестре регистраторов
//! (`shared::registrators`). Здесь остались фасад и общие конструкторы
//! представления, которыми пользуются сами провайдеры.
//!
//! Используется для детализации GL по регистратору и резолва ссылок (`refs.rs`).

use std::collections::HashMap;

use contracts::general_ledger::AggregateRepresentation;

/// Размер чанка id под лимит переменных SQLite.
pub const ID_CHUNK: usize = 500;

/// Нормализует дату до `YYYY-MM-DD` (обрезает время), пустую → None.
pub fn norm_date(raw: Option<String>) -> Option<String> {
    raw.map(|d| d.chars().take(10).collect::<String>())
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty())
}

/// Собирает представление: title = название типа (из метаданных агрегата),
/// date нормализуется до YYYY-MM-DD, doc_id обрезается (пустой → None).
/// Итоговая подпись формируется в [`to_label`] как «title · date · #doc_id».
pub fn build(
    type_name: &str,
    date: Option<String>,
    doc_id: Option<String>,
) -> AggregateRepresentation {
    AggregateRepresentation {
        title: type_name.trim().to_string(),
        date: norm_date(date),
        doc_id: doc_id
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
    }
}

/// Прогоняет батч-запрос `f` по чанкам id (под лимит переменных SQLite) и
/// объединяет результаты. Используется провайдерами модулей.
pub async fn chunked<F, Fut>(ids: &[String], f: F) -> HashMap<String, AggregateRepresentation>
where
    F: Fn(Vec<String>) -> Fut,
    Fut: std::future::Future<Output = HashMap<String, AggregateRepresentation>>,
{
    if ids.len() <= ID_CHUNK {
        return f(ids.to_vec()).await;
    }
    let mut out = HashMap::new();
    for chunk in ids.chunks(ID_CHUNK) {
        out.extend(f(chunk.to_vec()).await);
    }
    out
}

/// Строковая форма представления для `refs.rs`: «title · date · #doc_id».
pub fn to_label(rep: &AggregateRepresentation) -> String {
    let mut parts = vec![rep.title.clone()];
    if let Some(date) = rep.date.as_ref().filter(|d| !d.is_empty()) {
        parts.push(date.clone());
    }
    if let Some(doc) = rep.doc_id.as_ref().filter(|d| !d.is_empty()) {
        parts.push(format!("#{doc}"));
    }
    parts.join(" · ")
}

/// Батч-резолв представлений для набора id одного типа регистратора.
///
/// Кто умеет представлять какой тип — знает реестр регистраторов
/// (`shared::registrators`), а не этот модуль. Для неизвестного типа или
/// ненайденных id возвращает пустую/частичную карту: вызывающая сторона
/// делает фолбэк (UI — на синтетику).
pub async fn resolve_many(kind: &str, ids: &[String]) -> HashMap<String, AggregateRepresentation> {
    if ids.is_empty() {
        return HashMap::new();
    }
    match crate::shared::registrators::find(kind) {
        Some(registrator) => registrator.represent_many(ids).await,
        None => HashMap::new(),
    }
}

/// Резолв представления одного объекта. None — тип неизвестен или объект не найден.
pub async fn resolve(kind: &str, id: &str) -> Option<AggregateRepresentation> {
    let ids = [id.to_string()];
    resolve_many(kind, &ids).await.remove(id)
}

/// Как срез представляет ссылку на себя по имени реквизита.
///
/// **Зачем трейт.** Резолвер `/api/refs/resolve` разбирал `kind` собственным
/// `match` на пять реквизитов и знал, какому агрегату какой из них принадлежит.
/// Знание это чужое: что `connection_mp_ref` ведёт в a006, знает a006, а не
/// обработчик ссылок. Реестр даёт ещё и предсказуемость — забытый реквизит
/// теперь не «тихо не резолвится», а просто отсутствует в составе.
#[async_trait::async_trait]
pub trait ReferenceResolver: Send + Sync {
    /// Имя реквизита: `connection_mp_ref`, `organization_ref`, …
    fn ref_kind(&self) -> &'static str;

    /// Человекочитаемое представление объекта. `None` — не найден.
    async fn represent(&self, id: uuid::Uuid) -> Option<String>;
}

static REFERENCE_RESOLVERS: std::sync::OnceLock<Vec<std::sync::Arc<dyn ReferenceResolver>>> =
    std::sync::OnceLock::new();

/// Установить резолверы ссылок. Зовётся один раз из `composition::install_all()`.
///
/// # Panics
/// При повторной установке и при конфликте имён реквизитов.
pub fn install_reference_resolvers(resolvers: Vec<std::sync::Arc<dyn ReferenceResolver>>) {
    let mut kinds = std::collections::HashSet::new();
    for resolver in &resolvers {
        if !kinds.insert(resolver.ref_kind()) {
            panic!("реквизит '{}' заявлен дважды", resolver.ref_kind());
        }
    }
    if REFERENCE_RESOLVERS.set(resolvers).is_err() {
        panic!("резолверы ссылок уже установлены");
    }
}

/// Представление ссылки по имени реквизита и id.
///
/// Сначала спрашивает реестр реквизитов, затем — реестр регистраторов: типы
/// документов (`a012_wb_sales`, `p903_…`) приходят сюда тем же роутом, но
/// адресуются не именем поля, а именем типа.
pub async fn resolve_reference(kind: &str, id: &str) -> Option<String> {
    if let Some(resolver) = REFERENCE_RESOLVERS
        .get()
        .and_then(|resolvers| resolvers.iter().find(|r| r.ref_kind() == kind))
    {
        let uuid = uuid::Uuid::parse_str(id).ok()?;
        return resolver.represent(uuid).await;
    }

    resolve(kind, id).await.map(|rep| to_label(&rep))
}

/// Первое непустое значение из двух, с обрезкой пробелов.
///
/// Порядок «описание, иначе код» — правило представления справочников: у части
/// записей описание пустое, и код там единственное, чем их можно назвать.
pub fn pick(primary: &str, fallback: &str) -> Option<String> {
    let primary = primary.trim();
    if !primary.is_empty() {
        return Some(primary.to_string());
    }
    let fallback = fallback.trim();
    if !fallback.is_empty() {
        return Some(fallback.to_string());
    }
    None
}
