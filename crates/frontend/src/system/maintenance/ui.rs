//! Страница-заглушка обслуживания и админский тумблер.

use contracts::system::maintenance::{custom_reason as reason_rule, MaintenanceStatusDto};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::shared::date_utils::format_datetime_utc_local;
use crate::system::auth::context::{do_logout, use_auth};
use crate::system::maintenance::{api, use_maintenance};

const DATETIME_FMT: &str = "%d.%m.%Y %H:%M";

/// Причина, которую администратор написал сам (правило — в `contracts`, там же
/// оно и проверяется тестом). `None` — писать нечего: текст по умолчанию
/// дословно повторил бы начало сообщения.
fn custom_reason(status: &MaintenanceStatusDto) -> Option<String> {
    reason_rule(status.reason.as_deref()).map(str::to_string)
}

/// Одна строка «что происходит» — общая для всех экранов: плашки админа,
/// формы входа и заглушки. Текст один и тот же, потому что и вопрос один и тот
/// же: почему приложение не работает.
///
/// `audience_admin` — админ уже внутри, ему важно, что закрыто для остальных;
/// всем прочим важнее, что вход временно только для администраторов.
#[component]
pub fn MaintenanceLine(
    status: Signal<MaintenanceStatusDto>,
    #[prop(optional)] audience_admin: bool,
) -> impl IntoView {
    view! {
        <div class="maintenance-line">
            <span class="maintenance-line__icon">"🛠"</span>
            <span class="maintenance-line__text">
                {move || {
                    let status = status.get();
                    let reason = custom_reason(&status);
                    let head = if audience_admin {
                        match reason {
                            Some(reason) => format!("Режим обслуживания: {reason}."),
                            None => "Режим обслуживания включён.".to_string(),
                        }
                    } else {
                        match reason {
                            Some(reason) => format!("Идут технические работы: {reason}."),
                            None => "Идут технические работы.".to_string(),
                        }
                    };
                    let tail = if audience_admin && status.requires_restart {
                        "Требуется перезапуск бэкенда — до него режим снять нельзя."
                    } else if audience_admin {
                        "Пользователи не могут войти."
                    } else {
                        "Вход доступен только администраторам."
                    };
                    format!("{head} {tail}")
                }}
                {move || status.get().since.map(|since| view! {
                    <span class="maintenance-line__since">
                        {format!(" Начало: {}.", format_datetime_utc_local(&since, DATETIME_FMT))}
                    </span>
                })}
            </span>
        </div>
    }
}

/// Полноэкранная заглушка для пользователей, пока идут работы.
///
/// Формы входа здесь нет намеренно: `.login` — полноэкранный слой, вложенный в
/// карточку он перекрывал её целиком, и пользователь видел только «503» от
/// входа. Не вошедшим показывается обычный экран входа, а на нём — та же
/// строка о работах; вошедшему не-администратору здесь достаточно кнопки
/// выхода, чтобы уступить место администратору.
#[component]
pub fn MaintenancePage(status: Signal<MaintenanceStatusDto>) -> impl IntoView {
    let (_, set_auth_state) = use_auth();
    let logout = move |_| {
        spawn_local(async move {
            let _ = do_logout(set_auth_state).await;
        });
    };

    view! {
        <div class="maintenance">
            <div class="maintenance__card">
                <div class="maintenance__icon">"🛠"</div>
                <h1 class="maintenance__title">"Идут технические работы"</h1>
                // Только то, что администратор написал сам: текст по умолчанию
                // повторил бы заголовок слово в слово.
                {move || custom_reason(&status.get()).map(|reason| view! {
                    <p class="maintenance__reason">{reason}</p>
                })}
                <p class="maintenance__hint">
                    "Работа с данными приостановлена, чтобы записи не потерялись. \
                     Страница обновится сама, когда работы закончатся."
                </p>
                {move || status.get().since.map(|since| view! {
                    <div class="maintenance__since">
                        {format!("Начало: {}", format_datetime_utc_local(&since, DATETIME_FMT))}
                    </div>
                })}
                <div class="maintenance__actions">
                    <button class="button button--secondary" on:click=logout>
                        "Выйти и войти администратором"
                    </button>
                </div>
            </div>
        </div>
    }
}

/// Плашка для администратора: он внутрь пущен, но должен видеть, что для
/// остальных приложение закрыто.
#[component]
pub fn MaintenanceNotice() -> impl IntoView {
    let maintenance = use_maintenance();

    view! {
        <Show when=move || maintenance.status.get().active>
            <div class="maintenance-banner">
                <MaintenanceLine status=maintenance.status.into() audience_admin=true />
            </div>
        </Show>
    }
}

/// Управление режимом. Виден только администраторам (страница уже admin-only).
#[component]
pub fn MaintenanceToggle() -> impl IntoView {
    let maintenance = use_maintenance();
    let reason = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let error = RwSignal::<Option<String>>::new(None);

    let enable = move |_| {
        let value = reason.get().trim().to_string();
        busy.set(true);
        error.set(None);
        spawn_local(async move {
            match api::enable((!value.is_empty()).then_some(value)).await {
                Ok(status) => maintenance.status.set(status),
                Err(message) => error.set(Some(message)),
            }
            busy.set(false);
        });
    };

    let disable = move |_| {
        busy.set(true);
        error.set(None);
        spawn_local(async move {
            match api::disable().await {
                Ok(status) => maintenance.status.set(status),
                Err(message) => error.set(Some(message)),
            }
            busy.set(false);
        });
    };

    view! {
        <section class="datasets__section">
            <h2 class="datasets__section-title">"Режим обслуживания"</h2>
            <p class="datasets__section-hint">
                "Закрывает приложение для всех, кроме администраторов: вход и запросы \
                 обычных пользователей получают 503, внешний API — тоже. Операции с набором \
                 «База данных» включают режим сами. Режим живёт в памяти процесса и снимается \
                 перезапуском бэкенда; после восстановления он запускается автоматически."
            </p>

            {move || error.get().map(|message| view! {
                <div class="alert alert--error">{message}</div>
            })}

            <Show
                when=move || maintenance.status.get().active
                fallback=move || view! {
                    <div class="datasets__export-controls">
                        <label class="form__field datasets__note">
                            <span class="form__label">"Причина (увидят пользователи)"</span>
                            <input
                                type="text"
                                class="form__input"
                                placeholder="Например: перенос базы на новый сервер"
                                prop:value=move || reason.get()
                                on:input=move |ev| reason.set(event_target_value(&ev))
                            />
                        </label>
                        <button
                            class="button button--danger"
                            disabled=move || busy.get()
                            on:click=enable
                        >
                            "Включить обслуживание"
                        </button>
                    </div>
                }
            >
                <div class="alert alert--warning">
                    {move || {
                        let status = maintenance.status.get();
                        format!(
                            "Режим включён ({}). Причина: {}",
                            status.trigger.map(|t| t.label_ru()).unwrap_or("источник неизвестен"),
                            status.reason.unwrap_or_default(),
                        )
                    }}
                </div>
                {move || maintenance.status.get().requires_restart.then(|| view! {
                    <div class="alert alert--error">
                        "Подготовлена подмена базы данных. Автоматический перезапуск уже \
                         запланирован — снять режим сейчас нельзя, иначе пользователи начнут \
                         писать в базу, которая будет заменена."
                    </div>
                })}
                <div class="datasets__export-controls">
                    <button
                        class="button button--primary"
                        disabled=move || busy.get() || maintenance.status.get().requires_restart
                        on:click=disable
                    >
                        "Снять обслуживание"
                    </button>
                </div>
            </Show>
        </section>
    }
}
