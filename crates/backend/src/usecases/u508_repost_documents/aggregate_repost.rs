//! Собственная стратегия перепроведения оптом — необязательный крюк для среза.
//!
//! **Зачем.** По умолчанию перепроведение за период устроено одинаково для
//! всех: `Registrator::ids_in_period` отдаёт id, [`repost_ids_concurrent`]
//! прогоняет их. Но у a012 набор за месяц — сотни тысяч документов, и
//! `post_document` на каждом заново собирает `PostingPreparationCache`,
//! повторяя дорогие lookups. Выигрыш даёт группировка по дню продажи с
//! прогревом кэша на день и отчёт о прогрессе по кабинетам.
//!
//! Это знание о том, как устроено проведение конкретного агрегата, — то есть
//! свойство среза, а не движка. Поэтому не ветка в `execute_aggregate_repost`,
//! а объявленная срезом стратегия.
//!
//! **Крюк необязательный.** Реализаций сейчас одна. Второй пусть не появляется
//! ради симметрии: обычному агрегату хватает `ids_in_period`, и общий путь
//! честнее частного.
//!
//! [`repost_ids_concurrent`]: super::executor::repost_ids_concurrent

use std::sync::{Arc, OnceLock};

use anyhow::Result;
use async_trait::async_trait;
use contracts::usecases::u508_repost_documents::aggregate_request::AggregateRepostRequest;

use super::progress_tracker::ProgressTracker;

/// Агрегат, который перепроводится оптом по-своему.
#[async_trait]
pub trait AggregateBulkRepost: Send + Sync {
    /// Ключ агрегата — имя его каталога (`a012_wb_sales`).
    fn key(&self) -> &'static str;

    /// Перепровести за период. В отличие от пересбора проекции, сессию
    /// закрывает сама стратегия: она одна знает, чем считать прогресс —
    /// документами, днями или кабинетами.
    async fn repost(
        &self,
        tracker: &Arc<ProgressTracker>,
        session_id: &str,
        request: &AggregateRepostRequest,
    ) -> Result<()>;
}

static REGISTRY: OnceLock<Vec<Arc<dyn AggregateBulkRepost>>> = OnceLock::new();

/// Установить реестр. Зовётся один раз из `composition::install_all()`.
///
/// # Panics
/// При повторной установке и при конфликте ключей.
pub fn install(strategies: Vec<Arc<dyn AggregateBulkRepost>>) {
    let mut keys = std::collections::HashSet::new();
    for strategy in &strategies {
        if !keys.insert(strategy.key()) {
            panic!(
                "стратегия перепроведения '{}' заявлена дважды",
                strategy.key()
            );
        }
    }
    if REGISTRY.set(strategies).is_err() {
        panic!("реестр стратегий перепроведения уже установлен");
    }
}

/// Стратегия для агрегата. `None` — перепроводить общим путём.
pub fn find(key: &str) -> Option<&'static Arc<dyn AggregateBulkRepost>> {
    REGISTRY
        .get()
        .and_then(|strategies| strategies.iter().find(|strategy| strategy.key() == key))
}
