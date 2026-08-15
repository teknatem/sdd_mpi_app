//! Прогресс одной строки загрузки.
//!
//! У u502/u503/u504 в контрактах три структурно одинаковых `ImportProgress`
//! (различие — одно поле `barcodes_imported`). Страница импорта работает не с
//! ними, а с одной плоской моделью: строка = один агрегат, поэтому из сессии
//! сразу вынимается прогресс своего агрегата.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunStatus {
    Pending,
    Running,
    Completed,
    CompletedWithErrors,
    Failed,
    Cancelled,
}

impl RunStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pending => "Ожидает",
            Self::Running => "В работе",
            Self::Completed => "Завершено",
            Self::CompletedWithErrors => "Есть ошибки",
            Self::Failed => "Ошибка",
            Self::Cancelled => "Отменено",
        }
    }

    pub fn badge_class(self) -> &'static str {
        match self {
            Self::Pending => "badge badge--neutral",
            Self::Running => "badge badge--primary",
            Self::Completed => "badge badge--success",
            Self::CompletedWithErrors => "badge badge--warning",
            Self::Failed | Self::Cancelled => "badge badge--error",
        }
    }

    pub fn is_finished(self) -> bool {
        !matches!(self, Self::Pending | Self::Running)
    }
}

/// Прогресс загрузки по одному агрегату.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunProgress {
    pub status: RunStatus,
    pub processed: i32,
    pub total: Option<i32>,
    pub inserted: i32,
    pub updated: i32,
    pub errors: i32,
    pub current_item: Option<String>,
    /// Сообщения об ошибках сессии (уже отсортированы и без дублей).
    pub messages: Vec<String>,
}

impl RunProgress {
    /// Доля выполнения в процентах, если её вообще можно посчитать.
    pub fn percent(&self) -> Option<i32> {
        match self.total {
            Some(total) if total > 0 => {
                Some(((self.processed as f64 / total as f64) * 100.0).clamp(0.0, 100.0) as i32)
            }
            _ if self.status.is_finished() => Some(100),
            _ => None,
        }
    }

    /// Короткая сводка счётчиков для колонки статуса.
    pub fn counters(&self) -> String {
        let volume = match self.total {
            Some(total) if total > 0 => format!("{} / {}", self.processed, total),
            _ => self.processed.to_string(),
        };
        format!(
            "{} · нов. {} · обн. {} · ошиб. {}",
            volume, self.inserted, self.updated, self.errors
        )
    }
}

/// Приведение контрактного `ImportProgress` любого use-case к строке страницы.
pub trait IntoRunProgress {
    fn into_run_progress(self, aggregate: &str) -> RunProgress;
}

/// Три реализации отличаются только путём к модулю контрактов.
macro_rules! impl_into_run_progress {
    ($($module:ident)::+) => {
        impl IntoRunProgress for $($module)::+::ImportProgress {
            fn into_run_progress(self, aggregate: &str) -> RunProgress {
                use $($module)::+::{AggregateImportStatus, ImportStatus};

                let mut messages: Vec<String> =
                    self.errors.into_iter().map(|error| error.message).collect();
                messages.sort();
                messages.dedup();

                let agg = self
                    .aggregates
                    .into_iter()
                    .find(|agg| agg.aggregate_index == aggregate);

                // Статус агрегата точнее статуса сессии: в сессии может быть
                // несколько агрегатов, а строку интересует только свой.
                let status = match &agg {
                    Some(agg) => match agg.status {
                        AggregateImportStatus::Pending => RunStatus::Pending,
                        AggregateImportStatus::Running => RunStatus::Running,
                        AggregateImportStatus::Failed => RunStatus::Failed,
                        AggregateImportStatus::Completed if agg.errors > 0 => {
                            RunStatus::CompletedWithErrors
                        }
                        AggregateImportStatus::Completed => RunStatus::Completed,
                    },
                    // Агрегата ещё нет в сессии — берём статус самой сессии.
                    None => match self.status {
                        ImportStatus::Running => RunStatus::Running,
                        ImportStatus::Completed => RunStatus::Completed,
                        ImportStatus::CompletedWithErrors => RunStatus::CompletedWithErrors,
                        ImportStatus::Failed => RunStatus::Failed,
                        ImportStatus::Cancelled => RunStatus::Cancelled,
                    },
                };

                match agg {
                    Some(agg) => RunProgress {
                        status,
                        processed: agg.processed,
                        total: agg.total,
                        inserted: agg.inserted,
                        updated: agg.updated,
                        errors: agg.errors,
                        current_item: agg.current_item,
                        messages,
                    },
                    None => RunProgress {
                        status,
                        processed: self.total_processed,
                        total: None,
                        inserted: self.total_inserted,
                        updated: self.total_updated,
                        errors: self.total_errors,
                        current_item: None,
                        messages,
                    },
                }
            }
        }
    };
}

impl_into_run_progress!(contracts::usecases::u502_import_from_ozon::progress);
impl_into_run_progress!(contracts::usecases::u503_import_from_yandex::progress);
impl_into_run_progress!(contracts::usecases::u504_import_from_wildberries::progress);
