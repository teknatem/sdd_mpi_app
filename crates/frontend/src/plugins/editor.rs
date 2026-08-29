//! Обёртка над CodeMirror 6 (`static/plugin-editor.js`, сборка —
//! `pnpm build:plugin-editor`).
//!
//! Бандл грузится лениво при первом монтировании редактора. Тема не
//! передаётся: и хром редактора, и палитра подсветки заданы в JS через
//! `var(--...)`, поэтому цвета берутся от контейнера `.plugin-code-editor`
//! и меняются вместе с темой приложения без пересоздания редактора.

use js_sys::Function;
use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

#[wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(js_name = loadPluginCodeEditor)]
    fn load_plugin_code_editor() -> js_sys::Promise;

    #[wasm_bindgen::prelude::wasm_bindgen(js_namespace = PluginCodeEditor, js_name = create)]
    fn create_editor(
        parent: &web_sys::Element,
        language: &str,
        value: &str,
        on_change: &Function,
        on_save: &JsValue,
    ) -> JsValue;

    #[wasm_bindgen::prelude::wasm_bindgen(js_namespace = PluginCodeEditor, js_name = setValue)]
    fn set_editor_value(editor: &JsValue, value: &str);

    #[wasm_bindgen::prelude::wasm_bindgen(js_namespace = PluginCodeEditor, js_name = destroy)]
    fn destroy_editor(editor: &JsValue);
}

struct EditorHandle {
    editor: JsValue,
    _on_change: Closure<dyn FnMut(String)>,
    _on_save: Option<Closure<dyn FnMut()>>,
}

#[component]
pub fn CodeEditor(
    language: &'static str,
    value: RwSignal<String>,
    /// Ctrl/Cmd+S внутри редактора. Без него сочетание уходит браузеру.
    #[prop(optional)]
    on_save: Option<Callback<()>>,
    #[prop(optional, into)] class: String,
) -> impl IntoView {
    let node_ref = NodeRef::<html::Div>::new();
    let handle = StoredValue::new_local(None::<EditorHandle>);

    node_ref.on_load(move |element| {
        spawn_local(async move {
            if let Err(error) = JsFuture::from(load_plugin_code_editor()).await {
                web_sys::console::error_2(&JsValue::from_str("Failed to load CodeMirror"), &error);
                return;
            }
            if handle.is_disposed() {
                return;
            }

            let on_change = Closure::wrap(Box::new(move |next: String| {
                value.set(next);
            }) as Box<dyn FnMut(String)>);
            let save_closure = on_save.map(|callback| {
                Closure::wrap(Box::new(move || callback.run(())) as Box<dyn FnMut()>)
            });
            let save_arg = save_closure
                .as_ref()
                .map(|closure| closure.as_ref().clone())
                .unwrap_or(JsValue::UNDEFINED);

            let editor = create_editor(
                element.unchecked_ref::<web_sys::Element>(),
                language,
                &value.get_untracked(),
                on_change.as_ref().unchecked_ref(),
                &save_arg,
            );
            handle.set_value(Some(EditorHandle {
                editor,
                _on_change: on_change,
                _on_save: save_closure,
            }));
        });
    });

    Effect::new(move |_| {
        let next = value.get();
        handle.with_value(|handle| {
            if let Some(handle) = handle {
                set_editor_value(&handle.editor, &next);
            }
        });
    });

    on_cleanup(move || {
        let mut removed = None;
        handle.update_value(|current| {
            removed = current.take();
        });
        if let Some(handle) = removed {
            destroy_editor(&handle.editor);
        }
    });

    view! {
        <div node_ref=node_ref class=format!("plugin-code-editor {class}")></div>
    }
}
