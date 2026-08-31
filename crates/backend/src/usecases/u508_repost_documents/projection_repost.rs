//! Пересбор проекции за период — контракт и реестр.
//!
//! **Зачем трейт.** Три проекции пересобираются тремя разными способами: p903
//! — одним вызовом на весь период, p907 — построчно плюс отдельная пересборка
//! перечислений, p904 — через перепроведение документов-регистраторов. Общего
//! у них только рамка: сессия, прогресс, итоговый статус. Пока эти три способа
//! лежали ветками одного `match` в движке, движок знал имена трёх
//! маркетплейсных проекций и их внутренние сервисы.
//!
//! Теперь способ принадлежит проекции, рамка — движку. Состав реестра
//! объявляет `composition::projection_reposts`.

use std::sync::{Arc, OnceLock};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use contracts::usecases::u508_repost_documents::progress::RepostStatus;
use uuid::Uuid;

use super::progress_tracker::ProgressTracker;

/// Паспорт пункта на странице перепроведения.
pub struct ProjectionOptionInfo {
    /// Подпись пункта, напр. `"p903 — WB Finance Report"`.
    pub label: &'static str,
    /// Что именно произойдёт — текст показывается пользователю до запуска.
    pub description: &'static str,
}

/// Рамка прогона: куда писать прогресс и за какой период пересобирать.
pub struct RepostContext<'a> {
    pub tracker: &'a Arc<ProgressTracker>,
    pub session_id: &'a str,
    pub date_from: &'a str,
    pub date_to: &'a str,
}

impl RepostContext<'_> {
    /// Перепровести список регистраторов, ведя прогресс.
    ///
    /// Форма `(тип, ссылка)`, а не тип проекции: движок не должен знать строку
    /// p904, чтобы её перепровести. Тип резолвится реестром регистраторов,
    /// поэтому здесь работают и канонические ключи, и исторические.
    pub async fn repost_registrators(&self, registrators: &[(String, String)]) -> Result<()> {
        let total = registrators.len() as i32;
        self.tracker.set_total(self.session_id, total);

        let mut reposted = 0;
        for (index, (registrator_type, registrator_ref)) in registrators.iter().enumerate() {
            let current_item = format!("{registrator_type} {registrator_ref}");
            self.tracker.update_progress(
                self.session_id,
                index as i32,
                reposted,
                Some(current_item.clone()),
            );

            match Uuid::parse_str(registrator_ref) {
                Ok(id) => match super::executor::repost_one(registrator_type, id).await {
                    Ok(()) => reposted += 1,
                    Err(error) => self.tracker.add_error(
                        self.session_id,
                        format!("Failed to repost {registrator_type} {registrator_ref}: {error}"),
                    ),
                },
                Err(error) => self.tracker.add_error(
                    self.session_id,
                    format!("Invalid registrator_ref {registrator_ref}: {error}"),
                ),
            }

            self.tracker.update_progress(
                self.session_id,
                (index + 1) as i32,
                reposted,
                Some(current_item),
            );
        }

        self.tracker
            .update_progress(self.session_id, total, reposted, None);
        Ok(())
    }
}

/// Проекция, которую можно пересобрать за период.
#[async_trait]
pub trait ProjectionRepost: Send + Sync {
    /// Ключ проекции — имя её каталога (`p903_wb_finance_report`).
    fn key(&self) -> &'static str;

    fn option(&self) -> ProjectionOptionInfo;

    /// Пересобрать за период. Сессию закрывает движок, здесь — только работа
    /// и прогресс; ошибка отдельного элемента копится в сессии, а не
    /// возвращается наружу.
    async fn rebuild(&self, ctx: &RepostContext<'_>) -> Result<()>;
}

static REGISTRY: OnceLock<Vec<Arc<dyn ProjectionRepost>>> = OnceLock::new();

/// Установить реестр. Зовётся один раз из `composition::install_all()`.
///
/// # Panics
/// При повторной установке и при конфликте ключей.
pub fn install(projections: Vec<Arc<dyn ProjectionRepost>>) {
    let mut keys = std::collections::HashSet::new();
    for projection in &projections {
        if !keys.insert(projection.key()) {
            panic!("ключ проекции '{}' заявлен дважды", projection.key());
        }
    }
    if REGISTRY.set(projections).is_err() {
        panic!("реестр пересбора проекций уже установлен");
    }
}

/// Все проекции в порядке установки — он же порядок пунктов в UI.
pub fn all() -> &'static [Arc<dyn ProjectionRepost>] {
    REGISTRY
        .get()
        .map(Vec::as_slice)
        .expect("реестр пересбора проекций не установлен: composition::install_all() не был вызван")
}

pub fn find(key: &str) -> Option<&'static Arc<dyn ProjectionRepost>> {
    all().iter().find(|projection| projection.key() == key)
}

/// Итоговый статус сессии: с ошибками, если хоть одна накопилась.
pub(super) fn final_status(tracker: &Arc<ProgressTracker>, session_id: &str) -> RepostStatus {
    if tracker
        .get_progress(session_id)
        .map(|progress| progress.errors > 0)
        .unwrap_or(false)
    {
        RepostStatus::CompletedWithErrors
    } else {
        RepostStatus::Completed
    }
}

/// Пересобрать проекцию и закрыть сессию.
pub(super) async fn run(ctx: RepostContext<'_>, projection_key: &str) -> Result<()> {
    let projection = find(projection_key)
        .ok_or_else(|| anyhow!("Unsupported projection_key: {projection_key}"))?;
    projection.rebuild(&ctx).await?;
    let status = final_status(ctx.tracker, ctx.session_id);
    ctx.tracker.complete_session(ctx.session_id, status);
    Ok(())
}
