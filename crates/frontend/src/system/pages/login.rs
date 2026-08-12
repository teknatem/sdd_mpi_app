use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::shared::theme::ThemeSelect;
use crate::system::auth::{api, context::use_auth, storage};
use crate::system::maintenance::{use_maintenance, MaintenanceLine};

#[component]
pub fn LoginPage() -> impl IntoView {
    let username = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let error_message = RwSignal::new(Option::<String>::None);
    let is_loading = RwSignal::new(false);

    let (_, set_auth_state) = use_auth();
    // Статус обслуживания читается без авторизации: человек должен видеть, что
    // идут работы, ДО того как попробует войти и получит отказ.
    let maintenance = use_maintenance();

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();

        let username_val = username.get();
        let password_val = password.get();

        is_loading.set(true);
        error_message.set(None);

        spawn_local(async move {
            match api::login(username_val, password_val).await {
                Ok(response) => {
                    // Save tokens
                    storage::save_access_token(&response.access_token);
                    storage::save_refresh_token(&response.refresh_token);

                    // Update auth state - это автоматически переключит на MainLayout
                    set_auth_state.set(crate::system::auth::context::AuthState {
                        access_token: Some(response.access_token),
                        user_info: Some(response.user),
                    });

                    is_loading.set(false);
                }
                Err(e) => {
                    error_message.set(Some(e));
                    is_loading.set(false);
                    // Отказ мог случиться из-за режима обслуживания, включённого
                    // после загрузки страницы: обновляем статус сразу, чтобы
                    // полоса с объяснением появилась без ожидания опроса.
                    maintenance.refresh();
                }
            }
        });
    };

    view! {
        <div
            class="login"
            class:login--maintenance=move || maintenance.status.get().active
        >
            <div class="login__background"></div>

            // Строка о технических работах — вверху экрана и до всякого входа:
            // отказ «503» без объяснения человек читает как поломку.
            <Show when=move || maintenance.status.get().active>
                <div class="login__maintenance">
                    <MaintenanceLine status=maintenance.status.into() audience_admin=false />
                </div>
            </Show>

            <div class="login__theme-selector">
                <ThemeSelect />
            </div>

            <div class="login__card">
                <div class="login__header">
                    <div class="login__logo">
                        <img src="assets/images/gears.svg" alt="Integrator" width="64" height="64" />
                    </div>
                    <h1 class="login__title">"Integrator"</h1>
                    <p class="login__subtitle">"Войдите в систему"</p>
                </div>

                <Show when=move || error_message.get().is_some()>
                    <div class="warning-box warning-box--error login__error">
                        <span class="warning-box__icon">"⚠"</span>
                        <span class="warning-box__text">
                            {move || error_message.get().unwrap_or_default()}
                        </span>
                    </div>
                </Show>

                <form on:submit=on_submit class="login__form">
                    <div class="login__form-group">
                        <label class="login__label">
                            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                <path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2"></path>
                                <circle cx="12" cy="7" r="4"></circle>
                            </svg>
                            <span>"Логин"</span>
                        </label>
                        <input
                            type="text"
                            class="form__input"
                            value=move || username.get()
                            on:input=move |ev| username.set(event_target_value(&ev))
                            placeholder="Введите логин"
                            required
                            disabled=move || is_loading.get()
                            autocomplete="username"
                        />
                    </div>

                    <div class="login__form-group">
                        <label class="login__label">
                            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                <rect x="3" y="11" width="18" height="11" rx="2" ry="2"></rect>
                                <path d="M7 11V7a5 5 0 0 1 10 0v4"></path>
                            </svg>
                            <span>"Пароль"</span>
                        </label>
                        <input
                            type="password"
                            class="form__input"
                            value=move || password.get()
                            on:input=move |ev| password.set(event_target_value(&ev))
                            placeholder="Введите пароль"
                            required
                            disabled=move || is_loading.get()
                            autocomplete="current-password"
                        />
                    </div>

                <div class="login__footer">
                    <button
                        type="submit"
                        class="button button--primary login__button"
                        disabled=move || is_loading.get()
                    >
                        <Show
                            when=move || is_loading.get()
                            fallback=move || view! {
                                <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                    <path d="M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4"></path>
                                    <polyline points="10 17 15 12 10 7"></polyline>
                                    <line x1="15" y1="12" x2="3" y2="12"></line>
                                </svg>
                                "Войти"
                            }
                        >
                            <div class="loading-spinner"></div>
                        </Show>
                    </button>
                </div>
            </form>


            </div>
        </div>
    }
}
