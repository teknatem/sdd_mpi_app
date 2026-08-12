//! Режим технического обслуживания на стороне фронтенда.
//!
//! Статус опрашивается глобально, потому что режим может включиться в любой
//! момент — в том числе автоматически, операцией переноса базы, запущенной
//! другим администратором. Пользователь, сидящий на открытой вкладке, должен
//! узнать об этом сам, а не по внезапным 503 в каждом запросе.

pub mod api;
pub mod ui;

use contracts::system::maintenance::MaintenanceStatusDto;
use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use leptos::task::spawn_local;

pub use ui::{MaintenanceLine, MaintenanceNotice, MaintenancePage, MaintenanceToggle};

/// Период опроса статуса. Реже, чем change-tokens: включение режима — событие
/// редкое, а десять секунд задержки на входе в обслуживание роли не играют.
const POLL_MS: u32 = 10_000;

#[derive(Clone, Copy)]
pub struct MaintenanceContext {
    pub status: RwSignal<MaintenanceStatusDto>,
}

impl Default for MaintenanceContext {
    fn default() -> Self {
        Self::new()
    }
}

impl MaintenanceContext {
    pub fn new() -> Self {
        Self {
            status: RwSignal::new(MaintenanceStatusDto::default()),
        }
    }

    pub fn is_active(&self) -> bool {
        self.status.get().active
    }

    /// Разовое обновление статуса вне очереди опроса. Нужно там, где ответ
    /// сервера уже намекнул на режим — например, вход только что отказал: ждать
    /// до десяти секунд, пока экран объяснит причину, незачем.
    pub fn refresh(self) {
        spawn_local(async move {
            if let Ok(next) = api::fetch_status().await {
                self.status.set(next);
            }
        });
    }

    /// Запускает фоновый опрос. Ошибки сети намеренно не гасят режим: если
    /// бэкенд недоступен, показывать «всё в порядке» неверно — оставляем
    /// последнее известное состояние.
    pub fn start_polling(self) {
        spawn_local(async move {
            loop {
                if let Ok(next) = api::fetch_status().await {
                    // Сравниваем целиком: смена `requires_restart` меняет текст
                    // на экране ровно так же, как включение режима.
                    let previous = self.status.get_untracked();
                    if next != previous {
                        let backend_restarted = previous.active && !next.active;
                        self.status.set(next);
                        // После подмены БД недостаточно снять заглушку: сигналы и
                        // открытые вкладки ещё содержат данные прежней базы.
                        if backend_restarted {
                            if let Some(window) = web_sys::window() {
                                let _ = window.location().reload();
                            }
                            return;
                        }
                    }
                }
                TimeoutFuture::new(POLL_MS).await;
            }
        });
    }
}

pub fn use_maintenance() -> MaintenanceContext {
    use_context::<MaintenanceContext>().unwrap_or_default()
}
