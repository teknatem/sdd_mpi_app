//! Голосовой ввод (вариант A — браузерный Web Speech API) + переиспользуемая
//! кнопка-микрофон [`DictationButton`].
//!
//! Низкоуровневый интероп сделан через «сырой» JS (`js_sys::Reflect`), а не через
//! типизированные биндинги `web_sys::SpeechRecognition`: последние спрятаны за
//! `web_sys_unstable_apis` и потребовали бы правки RUSTFLAGS/`.cargo/config`.
//! Reflect-подход живёт на стабильном `web-sys` без новых фич в `Cargo.toml`.
//!
//! Ограничение: фактически работает в Chrome (использует серверы Google). В WebView2
//! (будущая Tauri-сборка) конструктор отсутствует — это ловит [`is_supported`], и
//! кнопка блокируется.
//!
//! Использование:
//! ```ignore
//! <DictationButton
//!     target=my_text_signal              // RwSignal<String>, куда дописывать речь
//!     disabled=is_busy                   // опционально: внешняя блокировка
//!     on_error=Callback::new(|m| ...)    // опционально: куда сообщить об ошибке
//! />
//! ```

use crate::shared::clipboard::copy_to_clipboard;
use crate::shared::components::hint_link::HintLink;
use crate::shared::icons::icon;
use leptos::prelude::*;
use thaw::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// Достаёт конструктор распознавания речи из `window`
/// (`SpeechRecognition` или вендорный `webkitSpeechRecognition`).
fn recognition_ctor() -> Option<js_sys::Function> {
    let window = web_sys::window()?;
    for key in ["SpeechRecognition", "webkitSpeechRecognition"] {
        if let Ok(v) = js_sys::Reflect::get(&window, &JsValue::from_str(key)) {
            if v.is_function() {
                return Some(v.unchecked_into::<js_sys::Function>());
            }
        }
    }
    None
}

/// Поддерживает ли текущий рантайм Web Speech API.
pub fn is_supported() -> bool {
    recognition_ctor().is_some()
}

/// Browser speech and microphone APIs require a secure context. `localhost` and
/// `127.0.0.1` are treated as secure exceptions, but LAN HTTP addresses are not.
pub fn is_secure_context() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    js_sys::Reflect::get(&window, &JsValue::from_str("isSecureContext"))
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn insecure_context_message() -> String {
    "Распознавание речи: браузер блокирует микрофон на небезопасном адресе. Откройте приложение по HTTPS или через localhost/127.0.0.1.".to_string()
}

/// Chrome-флаг, который позволяет пометить конкретный HTTP-origin как «безопасный»
/// и тем самым разблокировать микрофон в LAN-развёртывании без HTTPS.
const CHROME_INSECURE_ORIGIN_FLAG: &str =
    "chrome://flags/#unsafely-treat-insecure-origin-as-secure";

/// Текущий origin страницы (напр. `http://192.168.1.10:3000`) — его нужно вписать
/// в список разрешённых в chrome-флаге.
fn current_origin() -> String {
    web_sys::window()
        .and_then(|w| w.location().origin().ok())
        .unwrap_or_default()
}

fn format_recognition_error(msg: &str) -> String {
    match msg {
        "not-allowed" | "service-not-allowed" => {
            if !is_secure_context() {
                insecure_context_message()
            } else {
                "Распознавание речи: доступ к микрофону запрещён. Разрешите микрофон для этого сайта в настройках браузера.".to_string()
            }
        }
        "audio-capture" => {
            "Распознавание речи: микрофон не найден или занят другим приложением.".to_string()
        }
        "network" => "Распознавание речи: ошибка сети сервиса распознавания.".to_string(),
        other => format!("Распознавание речи: {}", other),
    }
}

fn set(obj: &JsValue, key: &str, value: &JsValue) {
    let _ = js_sys::Reflect::set(obj, &JsValue::from_str(key), value);
}

/// Запускает распознавание речи (один проход: останавливается сам после паузы).
///
/// - `on_result` — финальный распознанный текст (вызывается при `onresult`);
/// - `on_end`    — распознавание завершилось (пользователь умолк / вызван stop);
/// - `on_error`  — ошибка распознавания (строкой).
///
/// Возвращает JS-объект распознавателя; держите его живым, чтобы можно было
/// вызвать [`stop`]. `None` — если API недоступно.
///
/// Замечание: обработчики регистрируются через `Closure::forget`, поэтому каждый
/// запуск утекает три замыкания. Для прототипа приемлемо (утечка ограничена числом
/// нажатий на микрофон за сессию); при переходе на вариант B это уйдёт вместе с API.
pub fn start(
    on_result: impl Fn(String) + 'static,
    on_end: impl Fn() + 'static,
    on_error: impl Fn(String) + 'static,
) -> Option<JsValue> {
    let ctor = recognition_ctor()?;
    let rec = js_sys::Reflect::construct(&ctor, &js_sys::Array::new()).ok()?;

    set(&rec, "lang", &JsValue::from_str("ru-RU"));
    set(&rec, "interimResults", &JsValue::FALSE);
    set(&rec, "continuous", &JsValue::FALSE);
    set(&rec, "maxAlternatives", &JsValue::from_f64(1.0));

    // onresult: собрать transcript из results[i][0].transcript.
    let on_result_cb = Closure::<dyn FnMut(JsValue)>::new(move |ev: JsValue| {
        let Ok(results) = js_sys::Reflect::get(&ev, &JsValue::from_str("results")) else {
            return;
        };
        let len = js_sys::Reflect::get(&results, &JsValue::from_str("length"))
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as u32;
        let mut text = String::new();
        for i in 0..len {
            let Ok(result) = js_sys::Reflect::get(&results, &JsValue::from_f64(i as f64)) else {
                continue;
            };
            let Ok(alt) = js_sys::Reflect::get(&result, &JsValue::from_f64(0.0)) else {
                continue;
            };
            if let Ok(t) = js_sys::Reflect::get(&alt, &JsValue::from_str("transcript")) {
                if let Some(s) = t.as_string() {
                    text.push_str(&s);
                }
            }
        }
        on_result(text);
    });
    set(&rec, "onresult", on_result_cb.as_ref().unchecked_ref());
    on_result_cb.forget();

    let on_error_cb = Closure::<dyn FnMut(JsValue)>::new(move |ev: JsValue| {
        let msg = js_sys::Reflect::get(&ev, &JsValue::from_str("error"))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_else(|| "unknown".to_string());
        on_error(msg);
    });
    set(&rec, "onerror", on_error_cb.as_ref().unchecked_ref());
    on_error_cb.forget();

    let on_end_cb = Closure::<dyn FnMut(JsValue)>::new(move |_ev: JsValue| on_end());
    set(&rec, "onend", on_end_cb.as_ref().unchecked_ref());
    on_end_cb.forget();

    call_method(&rec, "start");
    Some(rec)
}

/// Останавливает активное распознавание (после этого придёт `onend`).
pub fn stop(rec: &JsValue) {
    call_method(rec, "stop");
}

fn call_method(obj: &JsValue, name: &str) {
    if let Ok(f) = js_sys::Reflect::get(obj, &JsValue::from_str(name)) {
        if f.is_function() {
            let _ = f.unchecked_into::<js_sys::Function>().call0(obj);
        }
    }
}

/// Кнопка голосового ввода. Один клик — начать запись, повторный — остановить.
/// Распознанный текст дописывается в `target` (с пробелом-разделителем).
///
/// Состояние записи и хэндл распознавателя — внутренние: компонент самодостаточен,
/// его можно поставить рядом с любым текстовым полем.
#[component]
#[allow(non_snake_case)]
pub fn DictationButton(
    /// Поле, в которое дописывается распознанный текст.
    target: RwSignal<String>,
    /// Внешняя блокировка (например, пока идёт отправка). По умолчанию — нет.
    #[prop(optional, into)]
    disabled: Signal<bool>,
    /// Куда сообщить об ошибке распознавания (опционально).
    #[prop(optional, into)]
    on_error: Option<Callback<String>>,
    /// Доп. inline-стиль кнопки (например, компактная ширина в ряду ввода).
    #[prop(optional, into)]
    button_style: Option<String>,
) -> impl IntoView {
    let is_listening = RwSignal::new(false);
    // Хэндл активного распознавателя держим в StoredValue с LocalStorage-хранилищем
    // (JsValue не Send/Sync), чтобы остановить запись по повторному клику.
    let recognition_handle = StoredValue::new_local(None::<JsValue>);
    let supported = is_supported();
    let secure_context = is_secure_context();
    let button_style = StoredValue::new(button_style.unwrap_or_default());

    move || {
        let listening = is_listening.get();
        // Небезопасный контекст оставляем кликабельным: по клику отдадим внятную
        // ошибку и подсказку про chrome-флаг (см. DictationDiagnostics).
        let is_disabled = disabled.get() || !supported;
        let appearance = if listening {
            ButtonAppearance::Primary
        } else {
            ButtonAppearance::Secondary
        };
        let title = if !supported {
            "Голосовой ввод недоступен в этом браузере"
        } else if !secure_context {
            "Голосовой ввод требует HTTPS или localhost/127.0.0.1"
        } else if listening {
            "Идёт запись — нажмите, чтобы остановить"
        } else {
            "Голосовой ввод"
        };
        // Стандартная иконочная кнопка ряда ввода — те же размеры, что у соседних
        // «Прикрепить»/«Отправить». Никакой своей стилизации: рамку, фон, центровку
        // иконки и контраст (в т.ч. в тёмной теме) целиком отдаём thaw.
        // Состояние записи различаем стандартно — сменой appearance на Primary.
        let mut style = String::from("min-width: 40px; padding-left: 8px; padding-right: 8px;");
        let extra = button_style.get_value();
        if !extra.is_empty() {
            style.push(' ');
            style.push_str(&extra);
        }
        view! {
            <Button
                appearance=appearance
                disabled=is_disabled
                attr:title=title
                attr:style=style
                on_click=move |_| {
                    // Уже пишем — остановить и выйти.
                    if is_listening.get_untracked() {
                        recognition_handle
                            .update_value(|h| {
                                if let Some(rec) = h.take() {
                                    stop(&rec);
                                }
                            });
                        is_listening.set(false);
                        return;
                    }
                    if !is_secure_context() {
                        if let Some(cb) = on_error {
                            cb.run(insecure_context_message());
                        }
                        return;
                    }
                    let started = start(
                        move |text| {
                            let t = text.trim();
                            if !t.is_empty() {
                                let mut cur = target.get_untracked();
                                if !cur.is_empty() && !cur.ends_with(' ') {
                                    cur.push(' ');
                                }
                                cur.push_str(t);
                                target.set(cur);
                            }
                        },
                        move || is_listening.set(false),
                        move |msg| {
                            is_listening.set(false);
                            if let Some(cb) = on_error {
                                cb.run(format_recognition_error(&msg));
                            }
                        },
                    );
                    match started {
                        Some(rec) => {
                            recognition_handle.set_value(Some(rec));
                            is_listening.set(true);
                        }
                        None => {
                            if let Some(cb) = on_error {
                                cb.run(
                                    "Голосовой ввод не поддерживается этим браузером".to_string(),
                                );
                            }
                        }
                    }
                }
            >
                {icon(if listening { "mic-off" } else { "microphone" })}
            </Button>
        }
    }
}

/// Ссылка-подсказка «Микрофон?»: по клику раскрывает поповер со статусом
/// (поддержка Web Speech API, безопасный контекст, origin) и инструкцией, как
/// разблокировать микрофон на HTTP-адресе через chrome-флаг
/// `unsafely-treat-insecure-origin-as-secure`.
#[component]
#[allow(non_snake_case)]
pub fn DictationDiagnostics() -> impl IntoView {
    let supported = is_supported();
    let secure_context = is_secure_context();
    let origin = current_origin();
    // Проблема ровно тогда, когда что-то мешает записи. Именно этот случай
    // лечит chrome-флаг (небезопасный origin).
    let has_problem = !supported || !secure_context;

    let origin_for_copy = origin.clone();
    let flag_url = CHROME_INSECURE_ORIGIN_FLAG.to_string();
    let flag_url_for_copy = flag_url.clone();

    let status_style = if has_problem {
        "color: var(--colorPaletteRedForeground1, #c50f1f);"
    } else {
        "color: var(--colorPaletteGreenForeground1, #0e700e);"
    };

    view! {
        <HintLink label="Микрофон?" width_px=400>
            <div style="display: flex; flex-direction: column; gap: 10px; font-size: 13px;">
                <div style="font-weight: 600;">"Диагностика микрофона"</div>
                <div style="display: flex; flex-direction: column; gap: 4px;">
                    <div style=status_style>
                        {if supported {
                            "✓ Web Speech API поддерживается"
                        } else {
                            "✗ Web Speech API недоступен (нужен Chrome/Chromium)"
                        }}
                    </div>
                    <div style=status_style>
                        {if secure_context {
                            "✓ Безопасный контекст (HTTPS или localhost)"
                        } else {
                            "✗ Небезопасный контекст — браузер запрещает микрофон"
                        }}
                    </div>
                    <div style="display: flex; align-items: center; gap: 6px;">
                        <span style="opacity: 0.7;">"Текущий origin:"</span>
                        <code style="user-select: all;">{origin.clone()}</code>
                        <Button
                            appearance=ButtonAppearance::Subtle
                            attr:title="Скопировать origin"
                            attr:style="min-width: 32px; padding: 2px 6px;"
                            on_click=move |_| copy_to_clipboard(&origin_for_copy)
                        >
                            {icon("copy")}
                        </Button>
                    </div>
                </div>

                <div style="opacity: 0.85; line-height: 1.5;">
                    "Если приложение открыто по HTTP (например, по IP в локальной сети), \
                     Chrome блокирует микрофон. Чтобы разблокировать без HTTPS, отметьте \
                     этот origin как «безопасный» в флаге браузера:"
                </div>

                <div style="display: flex; align-items: center; gap: 6px; flex-wrap: wrap;">
                    <a
                        href=flag_url.clone()
                        style="word-break: break-all;"
                    >
                        {flag_url.clone()}
                    </a>
                    <Button
                        appearance=ButtonAppearance::Subtle
                        attr:title="Скопировать ссылку"
                        attr:style="min-width: 32px; padding: 2px 6px;"
                        on_click=move |_| copy_to_clipboard(&flag_url_for_copy)
                    >
                        {icon("copy")}
                    </Button>
                </div>

                <ol style="margin: 0; padding-left: 18px; opacity: 0.85; line-height: 1.6;">
                    <li>"Скопируйте ссылку выше и вставьте её в адресную строку Chrome (браузер не даёт перейти по chrome:// по клику)."</li>
                    <li>"Включите флаг «Enabled» и впишите в поле текущий origin."</li>
                    <li>"Нажмите «Relaunch», чтобы перезапустить браузер."</li>
                </ol>
            </div>
        </HintLink>
    }
}
