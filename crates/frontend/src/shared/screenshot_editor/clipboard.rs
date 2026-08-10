//! Чтение изображения из системного буфера обмена (Clipboard API).

use js_sys::Array;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, File, FilePropertyBag};

const IMAGE_MIMES: &[&str] = &["image/png", "image/jpeg", "image/webp", "image/gif"];

/// Прочитать первое изображение из буфера обмена и вернуть как `File`.
pub async fn read_image_file_from_clipboard() -> Result<File, String> {
    let window = web_sys::window().ok_or_else(|| "Нет доступа к окну".to_string())?;
    let clipboard = window.navigator().clipboard();
    let items_val = JsFuture::from(clipboard.read()).await.map_err(|_| {
        "Не удалось прочитать буфер обмена. Разрешите доступ или скопируйте скриншот.".to_string()
    })?;

    let items: Array = items_val
        .dyn_into()
        .map_err(|_| "Некорректный формат буфера обмена".to_string())?;

    for index in 0..items.length() {
        let Some(item_val) = items.get(index).dyn_into::<web_sys::ClipboardItem>().ok() else {
            continue;
        };
        let types = item_val.types();
        for type_index in 0..types.length() {
            let mime = types.get(type_index).as_string().unwrap_or_default();
            if !mime.starts_with("image/") {
                continue;
            }
            let blob_val = JsFuture::from(item_val.get_type(&mime))
                .await
                .map_err(|_| format!("Не удалось прочитать {mime}"))?;
            let blob: Blob = blob_val
                .dyn_into()
                .map_err(|_| "Некорректные данные изображения".to_string())?;
            return blob_to_file(&blob, &mime);
        }
    }

    // Fallback: перебор известных MIME, если types() не вернул image/*
    for index in 0..items.length() {
        let Some(item_val) = items.get(index).dyn_into::<web_sys::ClipboardItem>().ok() else {
            continue;
        };
        for mime in IMAGE_MIMES {
            if let Ok(blob_val) = JsFuture::from(item_val.get_type(mime)).await {
                if let Ok(blob) = blob_val.dyn_into::<Blob>() {
                    if blob.size() > 0.0 {
                        return blob_to_file(&blob, mime);
                    }
                }
            }
        }
    }

    Err("В буфере обмена нет изображения".to_string())
}

fn blob_to_file(blob: &Blob, mime: &str) -> Result<File, String> {
    let ext = match mime {
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => "png",
    };
    let filename = format!("screenshot.{ext}");
    let array = Array::of1(blob);
    let options = FilePropertyBag::new();
    options.set_type(mime);
    File::new_with_blob_sequence_and_options(&array, &filename, &options)
        .map_err(|_| "Не удалось собрать файл изображения".to_string())
}

/// Из `DataTransfer` (paste/drop) извлечь первый файл-изображение.
pub fn first_image_from_data_transfer(dt: &web_sys::DataTransfer) -> Option<File> {
    let files = dt.files()?;
    for index in 0..files.length() {
        let file = files.get(index)?;
        if crate::shared::screenshot_editor::is_editable_image(&file) {
            return Some(file);
        }
    }
    None
}
