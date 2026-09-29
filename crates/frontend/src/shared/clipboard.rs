//! Clipboard utilities for copying text to clipboard.
//!
//! `navigator.clipboard.writeText` is blocked in unique-origin plugin iframes
//! (Permissions-Policy / crbug 414348233). Callers in the parent document still
//! have a working clipboard; sandboxed plugins should go through `host.copyText`.
//! JS exceptions are caught: a blocked Clipboard API is not an application panic.

use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

/// Copy text to the system clipboard.
///
/// Failures are swallowed: a blocked Clipboard API is not an application error.
pub fn copy_to_clipboard(text: &str) {
    let text = text.to_owned();
    spawn_local(async move {
        let _ = write_text(&text).await;
    });
}

/// Copy text to clipboard and run `on_success` only when a method actually succeeded.
pub fn copy_to_clipboard_with_callback<F>(text: &str, on_success: F)
where
    F: FnOnce() + 'static,
{
    let text = text.to_owned();
    spawn_local(async move {
        if write_text(&text).await {
            on_success();
        }
    });
}

/// Copy plain text. Returns whether anything landed in the clipboard.
pub async fn write_text(text: &str) -> bool {
    let Ok(payload) = serde_json::to_string(text) else {
        return false;
    };
    let script = format!(
        "(async function(text){{\
            try {{\
                if (navigator.clipboard && navigator.clipboard.writeText) {{\
                    await navigator.clipboard.writeText(text);\
                    return true;\
                }}\
            }} catch (e) {{}}\
            try {{\
                var t=document.createElement('textarea');\
                t.value=text;\
                t.setAttribute('readonly','');\
                t.style.cssText='position:fixed;left:-9999px;top:0;opacity:0';\
                document.body.appendChild(t);\
                t.select();\
                var ok=false;\
                try {{ ok=document.execCommand('copy'); }} catch (e2) {{}}\
                document.body.removeChild(t);\
                if (ok) return true;\
            }} catch (e) {{}}\
            return false;\
        }})({payload})"
    );
    let Ok(value) = js_sys::eval(&script) else {
        return false;
    };
    if let Some(flag) = value.as_bool() {
        return flag;
    }
    let Ok(promise) = value.dyn_into::<js_sys::Promise>() else {
        return false;
    };
    wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .ok()
        .and_then(|resolved| resolved.as_bool())
        .unwrap_or(false)
}
