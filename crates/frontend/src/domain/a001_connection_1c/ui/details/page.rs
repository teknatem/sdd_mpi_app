use crate::shared::api_utils::api_base;
use crate::shared::icons::icon;
use contracts::domain::a001_connection_1c::aggregate::{
    Connection1CDatabaseDto, ConnectionTestResult,
};
use contracts::domain::common::AggregateId;
use leptos::prelude::*;
use thaw::*;

#[component]
pub fn Connection1CDetails(
    #[prop(into)] id: Signal<Option<String>>,
    on_saved: Callback<()>,
    on_cancel: Callback<()>,
) -> impl IntoView {
    // Form fields
    let description = RwSignal::new(String::new());
    let url = RwSignal::new(String::new());
    let login = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let comment = RwSignal::new(String::new());
    let is_primary = RwSignal::new(false);

    let (conn_id, set_conn_id) = signal::<Option<String>>(None);
    let (conn_code, set_conn_code) = signal::<Option<String>>(None);

    let (error, set_error) = signal::<Option<String>>(None);
    let (test_result, set_test_result) = signal::<Option<ConnectionTestResult>>(None);
    let (is_testing, set_is_testing) = signal(false);

    // Reactive load/reset on id changes
    Effect::new(move |_| match id.get() {
        Some(conn_id_val) => {
            wasm_bindgen_futures::spawn_local(async move {
                match fetch_connection(&conn_id_val).await {
                    Ok(conn) => {
                        description.set(conn.description);
                        url.set(conn.url);
                        login.set(conn.login);
                        password.set(conn.password);
                        comment.set(conn.comment.unwrap_or_default());
                        is_primary.set(conn.is_primary);
                        set_conn_id.set(conn.id);
                        set_conn_code.set(conn.code);
                        set_error.set(None);
                        set_test_result.set(None);
                    }
                    Err(e) => set_error.set(Some(e)),
                }
            });
        }
        None => {
            description.set(String::new());
            url.set(String::new());
            login.set(String::new());
            password.set(String::new());
            comment.set(String::new());
            is_primary.set(false);
            set_conn_id.set(None);
            set_conn_code.set(None);
            set_error.set(None);
            set_test_result.set(None);
            set_is_testing.set(false);
        }
    });

    let handle_save = move |_: leptos::ev::MouseEvent| {
        let dto = Connection1CDatabaseDto {
            id: conn_id.get(),
            code: conn_code.get(),
            description: description.get(),
            url: url.get(),
            comment: if comment.get().is_empty() {
                None
            } else {
                Some(comment.get())
            },
            login: login.get(),
            password: password.get(),
            is_primary: is_primary.get(),
        };

        wasm_bindgen_futures::spawn_local(async move {
            match save_connection(dto).await {
                Ok(_) => on_saved.run(()),
                Err(e) => set_error.set(Some(e)),
            }
        });
    };

    let handle_test = move |_: leptos::ev::MouseEvent| {
        set_is_testing.set(true);
        set_test_result.set(None);
        set_error.set(None);

        let dto = Connection1CDatabaseDto {
            id: conn_id.get(),
            code: conn_code.get(),
            description: description.get(),
            url: url.get(),
            comment: if comment.get().is_empty() {
                None
            } else {
                Some(comment.get())
            },
            login: login.get(),
            password: password.get(),
            is_primary: is_primary.get(),
        };

        wasm_bindgen_futures::spawn_local(async move {
            match test_connection(dto).await {
                Ok(result) => {
                    set_test_result.set(Some(result));
                    set_is_testing.set(false);
                }
                Err(e) => {
                    set_error.set(Some(e));
                    set_is_testing.set(false);
                }
            }
        });
    };

    view! {
        <div class="details-container connection-1c-details">
            <div class="modal-header" style="display: flex; justify-content: space-between; align-items: center;">
                <h3 class="modal-title">
                    {move || {
                        if id.get().is_some() {
                            "Редактирование подключения 1C"
                        } else {
                            "Новое подключение 1C"
                        }
                    }}
                </h3>
                <div style="display: flex; gap: 8px;">
                    <Button
                        appearance=ButtonAppearance::Secondary
                        on_click=handle_test
                        disabled=Signal::derive(move || is_testing.get())
                    >
                        {icon("test")}
                        {move || if is_testing.get() { " Тест..." } else { " Тест" }}
                    </Button>
                    <Button appearance=ButtonAppearance::Primary on_click=handle_save>
                        {icon("save")}
                        " Сохранить"
                    </Button>
                    <Button appearance=ButtonAppearance::Secondary on_click=move |_| on_cancel.run(())>
                        {icon("x")}
                        " Закрыть"
                    </Button>
                </div>
            </div>

            <div class="modal-body" style="border: none;">
                {move || error.get().map(|e| view! {
                    <div style="padding: 8px 12px; margin-bottom: 10px; background: var(--color-error-50); border: 1px solid var(--color-error-100); border-radius: 6px; color: var(--color-error); font-size: 13px;">
                        {e}
                    </div>
                })}

                <div style="margin-bottom: 12px;">
                    <h4 style="margin: 0 0 8px 0; padding-bottom: 4px; border-bottom: 2px solid var(--color-border); font-size: 14px; font-weight: 600;">
                        "Основная информация"
                    </h4>
                    <div style="display: grid; grid-template-columns: 1fr 2fr; gap: 10px;">
                        <div class="form__group">
                            <label style="font-size: 13px; display: block; margin-bottom: 4px;">{"Наименование"}</label>
                            <Input value=description placeholder="Например: УТ11 (основная)" />
                        </div>
                        <div class="form__group">
                            <label style="font-size: 13px; display: block; margin-bottom: 4px;">{"URL (OData)"}</label>
                            <Input value=url placeholder="http://host/base/odata/standard.odata" />
                        </div>

                        <div class="form__group">
                            <label style="font-size: 13px; display: block; margin-bottom: 4px;">{"Логин"}</label>
                            <Input value=login placeholder="user" />
                        </div>
                        <div class="form__group">
                            <label style="font-size: 13px; display: block; margin-bottom: 4px;">{"Пароль"}</label>
                            <Input value=password input_type=InputType::Password placeholder="password" />
                        </div>

                        <div class="form__group" style="grid-column: 1 / -1;">
                            <label style="font-size: 13px; display: block; margin-bottom: 4px;">{"Комментарий"}</label>
                            <Textarea value=comment placeholder="Опционально" resize=TextareaResize::Vertical />
                        </div>
                    </div>
                </div>

                <div style="padding: 8px 12px; background: var(--color-background-secondary); border-radius: 6px; margin-bottom: 12px;">
                    <Checkbox checked=is_primary label="Основное подключение"/>
                </div>

                {move || test_result.get().map(|result| {
                    let class = if result.success { "success" } else { "error" };
                    view! {
                        <div class={class} style="margin-top: 12px; padding: 12px; border-radius: 6px; font-size: 13px;">
                            <h4 style="margin-top: 0; margin-bottom: 8px; font-size: 14px;">
                                {if result.success { "✅ Тест успешен" } else { "❌ Тест не пройден" }}
                            </h4>
                            <div>
                                <strong>{"Статус: "}</strong>
                                {result.message.clone()}
                                {" "}
                                <span style="color: #666; font-size: 11px;">{"("}{result.duration_ms}{"ms)"}</span>
                            </div>
                        </div>
                    }
                })}
            </div>
        </div>
    }
}

#[derive(Clone, Debug)]
struct Connection1CAggregateDto {
    id: Option<String>,
    code: Option<String>,
    description: String,
    url: String,
    comment: Option<String>,
    login: String,
    password: String,
    is_primary: bool,
}

async fn fetch_connection(id: &str) -> Result<Connection1CAggregateDto, String> {
    use contracts::domain::a001_connection_1c::aggregate::Connection1CDatabase;
    use wasm_bindgen::JsCast;
    use web_sys::{Request, RequestInit, RequestMode, Response};

    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);

    let url = format!("{}/api/connection_1c/{}", api_base(), id);
    let request = Request::new_with_str_and_init(&url, &opts).map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("{e:?}"))?;

    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: Response = resp_value.dyn_into().map_err(|e| format!("{e:?}"))?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let text = wasm_bindgen_futures::JsFuture::from(resp.text().map_err(|e| format!("{e:?}"))?)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let text: String = text.as_string().ok_or_else(|| "bad text".to_string())?;
    let data: Connection1CDatabase = serde_json::from_str(&text).map_err(|e| format!("{e}"))?;

    Ok(Connection1CAggregateDto {
        id: Some(data.base.id.as_string()),
        code: Some(data.base.code),
        description: data.base.description,
        url: data.url,
        comment: data.base.comment,
        login: data.login,
        password: data.password,
        is_primary: data.is_primary,
    })
}

async fn save_connection(dto: Connection1CDatabaseDto) -> Result<(), String> {
    use wasm_bindgen::JsCast;
    use web_sys::{Request, RequestInit, RequestMode, Response};

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(RequestMode::Cors);

    let body = serde_json::to_string(&dto).map_err(|e| format!("{e}"))?;
    opts.set_body(&wasm_bindgen::JsValue::from_str(&body));

    let url = format!("{}/api/connection_1c", api_base());
    let request = Request::new_with_str_and_init(&url, &opts).map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("{e:?}"))?;

    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: Response = resp_value.dyn_into().map_err(|e| format!("{e:?}"))?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    Ok(())
}

async fn test_connection(dto: Connection1CDatabaseDto) -> Result<ConnectionTestResult, String> {
    use wasm_bindgen::JsCast;
    use web_sys::{Request, RequestInit, RequestMode, Response};

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(RequestMode::Cors);

    let body = serde_json::to_string(&dto).map_err(|e| format!("{e}"))?;
    opts.set_body(&wasm_bindgen::JsValue::from_str(&body));

    let url = format!("{}/api/connection_1c/test", api_base());
    let request = Request::new_with_str_and_init(&url, &opts).map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("{e:?}"))?;

    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: Response = resp_value.dyn_into().map_err(|e| format!("{e:?}"))?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let text = wasm_bindgen_futures::JsFuture::from(resp.text().map_err(|e| format!("{e:?}"))?)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let text: String = text.as_string().ok_or_else(|| "bad text".to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("{e}"))
}
