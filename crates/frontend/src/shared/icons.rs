use leptos::prelude::*;

/// Внешние атрибуты `<svg>` + разметка тела иконки.
///
/// Тело хранится строкой и вставляется через `inner_html`, а не разворачивается
/// в `view!`. Это сознательно: каждый инлайновый `view!` порождает отдельный тип,
/// и 114 иконок давали 114 мономорфизаций `into_any` + `Render` в wasm. Здесь
/// `view!` один на всех, поэтому тип один. Строки константные, пользовательский
/// ввод сюда не попадает.
struct IconSpec {
    size: &'static str,
    view_box: &'static str,
    fill: &'static str,
    stroke: &'static str,
    stroke_width: &'static str,
    body: &'static str,
}

/// Контурная иконка (Lucide-стиль): рисуется обводкой, заливки нет.
const fn stroke_icon(size: &'static str, body: &'static str) -> IconSpec {
    IconSpec {
        size,
        view_box: "0 0 24 24",
        fill: "none",
        stroke: "currentColor",
        stroke_width: "2",
        body,
    }
}

/// Сплошная иконка: рисуется заливкой по currentColor, обводки нет.
const fn fill_icon(size: &'static str, body: &'static str) -> IconSpec {
    IconSpec {
        size,
        view_box: "0 0 24 24",
        fill: "currentColor",
        stroke: "none",
        stroke_width: "0",
        body,
    }
}

/// Аватар чата 28x28: самодостаточен — цвета заданы на дочерних элементах,
/// поэтому fill/stroke на корне намеренно нейтральны.
const fn avatar_icon(body: &'static str) -> IconSpec {
    IconSpec {
        size: "28",
        view_box: "0 0 28 28",
        fill: "none",
        stroke: "none",
        stroke_width: "0",
        body,
    }
}

pub fn icon(name: &str) -> AnyView {
    let spec = icon_spec(name);
    view! {
        <svg
            width=spec.size
            height=spec.size
            viewBox=spec.view_box
            fill=spec.fill
            stroke=spec.stroke
            stroke-width=spec.stroke_width
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
            inner_html=spec.body
        ></svg>
    }
    .into_any()
}

fn icon_spec(name: &str) -> IconSpec {
    match name {
        "customers" => stroke_icon(
            "20",
            r#"<path d="M17 21v-2a4 4 0 0 0-4-4H7a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M23 21v-2a4 4 0 0 0-3-3.87"/><path d="M16 3.13a4 4 0 0 1 0 7.75"/>"#,
        ),
        "orders" => stroke_icon(
            "20",
            r#"<path d="M21 15V5a2 2 0 0 0-2-2H7l-4 4v8a2 2 0 0 0 2 2h6"/><path d="M3 7h4V3"/><path d="M16 21l2-2 4 4"/><path d="M22 19a3 3 0 1 0-6 0 3 3 0 0 0 6 0z"/>"#,
        ),
        "products" => stroke_icon(
            "20",
            r#"<path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/><path d="M3.27 6.96 12 12l8.73-5.04"/><path d="M12 22V12"/>"#,
        ),
        "inventory" => stroke_icon(
            "20",
            r#"<rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/>"#,
        ),
        "suppliers" => stroke_icon(
            "20",
            r#"<path d="M3 22h18"/><path d="M6 22V8l6-5 6 5v14"/><rect x="9" y="13" width="6" height="9"/>"#,
        ),
        "purchases" => stroke_icon(
            "20",
            r#"<circle cx="9" cy="21" r="1"/><circle cx="20" cy="21" r="1"/><path d="M1 1h4l2.68 12.39a2 2 0 0 0 2 1.61h7.72a2 2 0 0 0 2-1.61L23 6H6"/>"#,
        ),
        "invoices" => stroke_icon(
            "20",
            r#"<path d="M14 2H6a2 2 0 0 0-2 2v16l4-2 4 2 4-2 4 2V8z"/><path d="M14 2v6h6"/><path d="M8 13h8"/><path d="M8 17h5"/>"#,
        ),
        "payments" => stroke_icon(
            "20",
            r#"<rect x="2" y="4" width="20" height="16" rx="2"/><path d="M2 9h20"/><rect x="6" y="13" width="6" height="3" rx="1"/>"#,
        ),
        "shipments" => stroke_icon(
            "20",
            r#"<path d="M3 7h13v10H3z"/><path d="M16 7h3l2 3v7h-5z"/><circle cx="7.5" cy="18" r="1.5"/><circle cx="18.5" cy="18" r="1.5"/>"#,
        ),
        "users" => stroke_icon(
            "20",
            r#"<path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/>"#,
        ),
        // Свой личный чат — один человек.
        "chat-personal" => stroke_icon(
            "18",
            r#"<path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/>"#,
        ),
        // Общий доступ — глобус (виден всем).
        "chat-shared" => stroke_icon(
            "18",
            r#"<circle cx="12" cy="12" r="10"/><path d="M2 12h20"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/>"#,
        ),
        // Чужой личный чат — человек с замком (недоступен для правки другими).
        "chat-foreign" => stroke_icon(
            "18",
            r#"<path d="M14 20v-1a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v1"/><circle cx="8" cy="7" r="4"/><rect x="15" y="13" width="7" height="6" rx="1"/><path d="M16.5 13v-1.5a1.5 1.5 0 0 1 3 0V13"/>"#,
        ),
        "folder-open" => fill_icon(
            "18",
            r#"<path d="M19.9 9.1c-.1-.1-.2-.1-.4-.1H4.5c-.2 0-.3 0-.4.1-.1.1-.1.2-.1.4v9c0 1.1.9 2 2 2h12c1.1 0 2-.9 2-2v-9c0-.2 0-.3-.1-.4z" opacity="0.9"/><path d="M20 8V6c0-1.1-.9-2-2-2h-6L9.6 2.3c-.2-.2-.4-.3-.7-.3H6C4.9 2 4 2.9 4 4v4.5c0 .2 0 .3.1.4.1.1.2.1.4.1h15c.2 0 .3 0 .4-.1.1-.1.1-.2.1-.4z"/>"#,
        ),
        "folder-closed" => fill_icon(
            "18",
            r#"<path d="M20 6h-8l-2-2H4c-1.1 0-1.99.9-1.99 2L2 18c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V8c0-1.1-.9-2-2-2zm0 12H4V8h16v10z" opacity="0.9"/>"#,
        ),
        "item" => stroke_icon("14", r#"<circle cx="12" cy="12" r="3"/>"#),
        "chevron-right" => stroke_icon("16", r#"<polyline points="9 18 15 12 9 6"/>"#),
        "chevron-down" => stroke_icon("16", r#"<polyline points="6 9 12 15 18 9"/>"#),
        "plus" => stroke_icon(
            "16",
            r#"<line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>"#,
        ),
        "refresh" => stroke_icon(
            "16",
            r#"<path d="M21.5 2v6h-6M2.5 22v-6h6M2 11.5a10 10 0 0 1 18.8-4.3M22 12.5a10 10 0 0 1-18.8 4.2"/>"#,
        ),
        // Как Lucide refresh-cw — те же стрелки, чуть крупнее для кнопок тулбара.
        "refresh-cw" => stroke_icon(
            "18",
            r#"<path d="M21.5 2v6h-6M2.5 22v-6h6M2 11.5a10 10 0 0 1 18.8-4.3M22 12.5a10 10 0 0 1-18.8 4.2"/>"#,
        ),
        // Треугольник play (запуск).
        "play" => fill_icon("18", r#"<path d="M8 5v14l11-7-11-7z"/>"#),
        "save" => stroke_icon(
            "16",
            r#"<path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z"/><polyline points="17 21 17 13 7 13 7 21"/><polyline points="7 3 7 8 15 8"/>"#,
        ),
        "cancel" | "close" | "x" => stroke_icon(
            "16",
            r#"<line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>"#,
        ),
        "edit" => stroke_icon(
            "16",
            r#"<path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/>"#,
        ),
        "delete" | "trash" => stroke_icon(
            "16",
            r#"<polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/><line x1="10" y1="11" x2="10" y2="17"/><line x1="14" y1="11" x2="14" y2="17"/>"#,
        ),
        "search" => stroke_icon(
            "16",
            r#"<circle cx="11" cy="11" r="8"/><path d="m21 21-4.35-4.35"/>"#,
        ),
        "check" => stroke_icon("16", r#"<polyline points="20 6 9 17 4 12"/>"#),
        "download" => stroke_icon(
            "16",
            r#"<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/>"#,
        ),
        "upload" => stroke_icon(
            "16",
            r#"<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/>"#,
        ),
        "excel" => stroke_icon(
            "16",
            r#"<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="9" y1="13" x2="15" y2="17"/><line x1="15" y1="13" x2="9" y2="17"/>"#,
        ),
        "file-text" | "document" => stroke_icon(
            "16",
            r#"<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/><polyline points="10 9 9 9 8 9"/>"#,
        ),
        "database" => stroke_icon(
            "16",
            r#"<ellipse cx="12" cy="5" rx="9" ry="3"/><path d="M21 12c0 1.66-4 3-9 3s-9-1.34-9-3"/><path d="M3 5v14c0 1.66 4 3 9 3s9-1.34 9-3V5"/>"#,
        ),
        "link" | "plug" => stroke_icon(
            "16",
            r#"<path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/>"#,
        ),
        "building" => stroke_icon(
            "16",
            r#"<rect x="4" y="2" width="16" height="20" rx="2" ry="2"/><path d="M9 22v-4h6v4"/><path d="M8 6h.01"/><path d="M16 6h.01"/><path d="M12 6h.01"/><path d="M12 10h.01"/><path d="M12 14h.01"/><path d="M16 10h.01"/><path d="M16 14h.01"/><path d="M8 10h.01"/><path d="M8 14h.01"/>"#,
        ),
        "contact" | "user-circle" => stroke_icon(
            "16",
            r#"<path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/>"#,
        ),
        "list" | "menu" => stroke_icon(
            "16",
            r#"<line x1="8" y1="6" x2="21" y2="6"/><line x1="8" y1="12" x2="21" y2="12"/><line x1="8" y1="18" x2="21" y2="18"/><line x1="3" y1="6" x2="3.01" y2="6"/><line x1="3" y1="12" x2="3.01" y2="12"/><line x1="3" y1="18" x2="3.01" y2="18"/>"#,
        ),
        "shopping-bag" | "store" => stroke_icon(
            "16",
            r#"<path d="M6 2L3 6v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V6l-3-4z"/><line x1="3" y1="6" x2="21" y2="6"/><path d="M16 10a4 4 0 0 1-8 0"/>"#,
        ),
        "package" | "box" => stroke_icon(
            "16",
            r#"<line x1="16.5" y1="9.4" x2="7.5" y2="4.21"/><path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/><polyline points="3.27 6.96 12 12.01 20.73 6.96"/><line x1="12" y1="22.08" x2="12" y2="12"/>"#,
        ),
        "import" | "download-cloud" => stroke_icon(
            "16",
            r#"<polyline points="8 17 12 21 16 17"/><line x1="12" y1="12" x2="12" y2="21"/><path d="M20.88 18.09A5 5 0 0 0 18 9h-1.26A8 8 0 1 0 3 16.29"/>"#,
        ),
        "zap" | "lightning" => stroke_icon(
            "16",
            r#"<polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"/>"#,
        ),
        "layers" | "stack" => stroke_icon(
            "16",
            r#"<polygon points="12 2 2 7 12 12 22 7 12 2"/><polyline points="2 17 12 22 22 17"/><polyline points="2 12 12 17 22 12"/>"#,
        ),
        "file" => stroke_icon(
            "16",
            r#"<path d="M13 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z"/><polyline points="13 2 13 9 20 9"/>"#,
        ),
        "columns" | "table" => stroke_icon(
            "16",
            r#"<rect x="3" y="3" width="18" height="18" rx="2" ry="2"/><line x1="12" y1="3" x2="12" y2="21"/><line x1="3" y1="9" x2="21" y2="9"/><line x1="3" y1="15" x2="21" y2="15"/>"#,
        ),
        "eye" | "preview" => stroke_icon(
            "16",
            r#"<path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/>"#,
        ),
        "code" | "json" => stroke_icon(
            "16",
            r#"<polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/>"#,
        ),
        "copy" => stroke_icon(
            "16",
            r#"<rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/>"#,
        ),
        "dollar-sign" | "currency" | "cash" => stroke_icon(
            "16",
            r#"<line x1="12" y1="1" x2="12" y2="23"/><path d="M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6"/>"#,
        ),
        "credit-card" | "card" => stroke_icon(
            "16",
            r#"<rect x="1" y="4" width="22" height="16" rx="2" ry="2"/><line x1="1" y1="10" x2="23" y2="10"/>"#,
        ),
        "package-x" | "return" => stroke_icon(
            "16",
            r#"<path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/><polyline points="3.27 6.96 12 12.01 20.73 6.96"/><line x1="12" y1="22.08" x2="12" y2="12"/><line x1="15" y1="15" x2="9" y2="9"/><line x1="9" y1="15" x2="15" y2="9"/>"#,
        ),
        "percent" | "percentage" => stroke_icon(
            "16",
            r#"<line x1="19" y1="5" x2="5" y2="19"/><circle cx="6.5" cy="6.5" r="2.5"/><circle cx="17.5" cy="17.5" r="2.5"/>"#,
        ),
        "barcode" => stroke_icon(
            "16",
            r#"<path d="M3 5v14"/><path d="M8 5v14"/><path d="M12 5v14"/><path d="M17 5v14"/><path d="M21 5v14"/>"#,
        ),
        "bar-chart" | "chart" => stroke_icon(
            "16",
            r#"<line x1="12" y1="20" x2="12" y2="10"/><line x1="18" y1="20" x2="18" y2="4"/><line x1="6" y1="20" x2="6" y2="16"/>"#,
        ),
        // === Lucide Icons for UI Redesign ===
        "panel-left-close" => stroke_icon(
            "18",
            r#"<rect width="18" height="18" x="3" y="3" rx="2"/><path d="M9 3v18"/><path d="m16 15-3-3 3-3"/>"#,
        ),
        "panel-left-open" => stroke_icon(
            "18",
            r#"<rect width="18" height="18" x="3" y="3" rx="2"/><path d="M9 3v18"/><path d="m14 9 3 3-3 3"/>"#,
        ),
        "panel-right-close" => stroke_icon(
            "18",
            r#"<rect width="18" height="18" x="3" y="3" rx="2"/><path d="M15 3v18"/><path d="m8 9 3 3-3 3"/>"#,
        ),
        "panel-right-open" => stroke_icon(
            "18",
            r#"<rect width="18" height="18" x="3" y="3" rx="2"/><path d="M15 3v18"/><path d="m10 15-3-3 3-3"/>"#,
        ),
        "layout-dashboard" => stroke_icon(
            "18",
            r#"<rect width="7" height="9" x="3" y="3" rx="1"/><rect width="7" height="5" x="14" y="3" rx="1"/><rect width="7" height="9" x="14" y="12" rx="1"/><rect width="7" height="5" x="3" y="16" rx="1"/>"#,
        ),
        "settings" => stroke_icon(
            "18",
            r#"<path d="M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.319 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.319 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915"/><circle cx="12" cy="12" r="3"/>"#,
        ),
        "bell" => stroke_icon(
            "18",
            r#"<path d="M10.268 21a2 2 0 0 0 3.464 0"/><path d="M3.262 15.326A1 1 0 0 0 4 17h16a1 1 0 0 0 .74-1.673C19.41 13.956 18 12.499 18 8A6 6 0 0 0 6 8c0 4.499-1.411 5.956-2.738 7.326"/>"#,
        ),
        "log-out" => stroke_icon(
            "18",
            r#"<path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/><polyline points="16 17 21 12 16 7"/><line x1="21" x2="9" y1="12" y2="12"/>"#,
        ),
        "log-in" => stroke_icon(
            "18",
            r#"<path d="M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4"/><polyline points="10 17 15 12 10 7"/><line x1="15" x2="3" y1="12" y2="12"/>"#,
        ),
        "sun" => stroke_icon(
            "18",
            r#"<circle cx="12" cy="12" r="4"/><path d="M12 2v2"/><path d="M12 20v2"/><path d="m4.93 4.93 1.41 1.41"/><path d="m17.66 17.66 1.41 1.41"/><path d="M2 12h2"/><path d="M20 12h2"/><path d="m6.34 17.66-1.41 1.41"/><path d="m19.07 4.93-1.41 1.41"/>"#,
        ),
        "moon" => stroke_icon("18", r#"<path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/>"#),
        "palette" => stroke_icon(
            "18",
            r#"<circle cx="13.5" cy="6.5" r=".5" fill="currentColor"/><circle cx="17.5" cy="10.5" r=".5" fill="currentColor"/><circle cx="8.5" cy="7.5" r=".5" fill="currentColor"/><circle cx="6.5" cy="12.5" r=".5" fill="currentColor"/><path d="M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10c.926 0 1.648-.746 1.648-1.688 0-.437-.18-.835-.437-1.125-.29-.289-.438-.652-.438-1.125a1.64 1.64 0 0 1 1.668-1.668h1.996c3.051 0 5.555-2.503 5.555-5.555C21.965 6.012 17.461 2 12 2z"/>"#,
        ),
        "filter" => stroke_icon(
            "16",
            r#"<polygon points="22 3 2 3 10 12.46 10 19 14 21 14 12.46 22 3"/>"#,
        ),
        "activity" => stroke_icon(
            "16",
            r#"<path d="M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2"/>"#,
        ),
        "info" => stroke_icon(
            "16",
            r#"<circle cx="12" cy="12" r="10"/><path d="M12 16v-4"/><path d="M12 8h.01"/>"#,
        ),
        "list-ordered" => stroke_icon(
            "16",
            r#"<line x1="10" y1="6" x2="21" y2="6"/><line x1="10" y1="12" x2="21" y2="12"/><line x1="10" y1="18" x2="21" y2="18"/><path d="M4 6h1v4"/><path d="M4 10h2"/><path d="M6 18H4c0-1 2-2 2-3s-1-1.5-2-1"/>"#,
        ),
        "clock" => stroke_icon(
            "16",
            r#"<circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/>"#,
        ),
        "home" => stroke_icon(
            "18",
            r#"<path d="M15 21v-8a1 1 0 0 0-1-1h-4a1 1 0 0 0-1 1v8"/><path d="M3 10a2 2 0 0 1 .709-1.528l7-5.999a2 2 0 0 1 2.582 0l7 5.999A2 2 0 0 1 21 10v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>"#,
        ),
        "chevrons-left" => stroke_icon(
            "16",
            r#"<path d="m11 17-5-5 5-5"/><path d="m18 17-5-5 5-5"/>"#,
        ),
        "chevrons-right" => stroke_icon(
            "16",
            r#"<path d="m6 17 5-5-5-5"/><path d="m13 17 5-5-5-5"/>"#,
        ),
        "chevron-left" => stroke_icon("16", r#"<path d="m15 18-6-6 6-6"/>"#),
        "alert-circle" => stroke_icon(
            "16",
            r#"<circle cx="12" cy="12" r="10"/><line x1="12" x2="12" y1="8" y2="12"/><line x1="12" x2="12.01" y1="16" y2="16"/>"#,
        ),
        "alert-triangle" => stroke_icon(
            "16",
            r#"<path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3"/><path d="M12 9v4"/><path d="M12 17h.01"/>"#,
        ),
        "trending-up" => stroke_icon(
            "16",
            r#"<polyline points="22 7 13.5 15.5 8.5 10.5 2 17"/><polyline points="16 7 22 7 22 13"/>"#,
        ),
        "trending-down" => stroke_icon(
            "16",
            r#"<polyline points="22 17 13.5 8.5 8.5 13.5 2 7"/><polyline points="16 17 22 17 22 11"/>"#,
        ),
        "user" => stroke_icon(
            "18",
            r#"<path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/>"#,
        ),
        "test-tube" => stroke_icon(
            "20",
            r#"<path d="M14.5 2v17.5c0 1.4-1.1 2.5-2.5 2.5s-2.5-1.1-2.5-2.5V2"/><path d="M8.5 2h7"/><path d="M14.5 16h-5"/>"#,
        ),
        // ── Drag / grip ──────────────────────────────────────────────────
        // Open-palm hand — universal "you can drag this" affordance
        "grab" | "hand" => stroke_icon(
            "16",
            r#"<path d="M18 11V6a2 2 0 0 0-2-2 2 2 0 0 0-2 2"/><path d="M14 10V4a2 2 0 0 0-2-2 2 2 0 0 0-2 2v2"/><path d="M10 10.5V6a2 2 0 0 0-2-2 2 2 0 0 0-2 2v8"/><path d="M18 8a2 2 0 1 1 4 0v6a8 8 0 0 1-8 8h-2c-2.8 0-4.5-.86-5.99-2.34l-3.6-3.6a2 2 0 0 1 2.83-2.82L7 15"/>"#,
        ),
        // Classic 6-dot grip handle
        "grip-vertical" => fill_icon(
            "14",
            r#"<circle cx="9"  cy="5"  r="1.5"/><circle cx="9"  cy="12" r="1.5"/><circle cx="9"  cy="19" r="1.5"/><circle cx="15" cy="5"  r="1.5"/><circle cx="15" cy="12" r="1.5"/><circle cx="15" cy="19" r="1.5"/>"#,
        ),
        // ── Folders ──────────────────────────────────────────────────────
        "folder" => stroke_icon(
            "16",
            r#"<path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>"#,
        ),
        "folder-plus" => stroke_icon(
            "16",
            r#"<path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/><line x1="12" y1="11" x2="12" y2="17"/><line x1="9"  y1="14" x2="15" y2="14"/>"#,
        ),
        // ── Charts / metrics ─────────────────────────────────────────────
        "bar-chart-2" => stroke_icon(
            "16",
            r#"<line x1="18" y1="20" x2="18" y2="10"/><line x1="12" y1="20" x2="12" y2="4"/><line x1="6"  y1="20" x2="6"  y2="14"/><line x1="2"  y1="20" x2="22" y2="20"/>"#,
        ),
        // ── Misc ─────────────────────────────────────────────────────────
        "check-square" => stroke_icon(
            "16",
            r#"<polyline points="9 11 12 14 22 4"/><path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11"/>"#,
        ),
        "square" => stroke_icon(
            "16",
            r#"<rect x="3" y="3" width="18" height="18" rx="2" ry="2"/>"#,
        ),
        "loader" => stroke_icon("16", r#"<path d="M21 12a9 9 0 1 1-6.219-8.56"/>"#),
        "trash-2" => stroke_icon(
            "16",
            r#"<polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/><path d="M10 11v6"/><path d="M14 11v6"/><path d="M9 6V4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2"/>"#,
        ),
        "compass" => stroke_icon(
            "18",
            r#"<circle cx="12" cy="12" r="10"/><polygon points="16.24 7.76 14.12 14.12 7.76 16.24 9.88 9.88 16.24 7.76"/>"#,
        ),
        "megaphone" => stroke_icon(
            "16",
            r#"<path d="m3 11 18-5v12L3 14v-3z"/><path d="M11.6 16.8a3 3 0 1 1-5.8-1.6"/>"#,
        ),
        "shopping-cart" => stroke_icon(
            "16",
            r#"<circle cx="9" cy="21" r="1"/><circle cx="20" cy="21" r="1"/><path d="M1 1h4l2.68 12.39a2 2 0 0 0 2 1.61h7.72a2 2 0 0 0 2-1.61L23 6H6"/>"#,
        ),
        "tag" => stroke_icon(
            "16",
            r#"<path d="M20.59 13.41 13.42 20.58a2 2 0 0 1-2.83 0L2 12V2h10l8.59 8.59a2 2 0 0 1 0 2.82z"/><line x1="7" y1="7" x2="7.01" y2="7"/>"#,
        ),
        "receipt" => stroke_icon(
            "16",
            r#"<path d="M4 2v20l2-1 2 1 2-1 2 1 2-1 2 1 2-1 2 1V2l-2 1-2-1-2 1-2-1-2 1-2-1-2 1-2-1z"/><path d="M16 8h-6a2 2 0 1 0 0 4h4a2 2 0 1 1 0 4H8"/><path d="M12 17.5v-11"/>"#,
        ),
        "message-circle" => stroke_icon("16", r#"<path d="M7.9 20A9 9 0 1 0 4 16.1L2 22Z"/>"#),
        "paperclip" | "attach" => stroke_icon(
            "16",
            r#"<path d="m21.44 11.05-9.19 9.19a6 6 0 0 1-8.49-8.49l8.57-8.57A4 4 0 1 1 18 8.84l-8.59 8.57a2 2 0 0 1-2.83-2.83l8.49-8.48"/>"#,
        ),
        // Lucide mail — конверт.
        "mail" => stroke_icon(
            "20",
            r#"<rect x="2" y="4" width="20" height="16" rx="2"/><path d="m22 7-8.97 5.7a1.94 1.94 0 0 1-2.06 0L2 7"/>"#,
        ),
        // Paper-plane «Send» (Lucide send).
        "send" => stroke_icon(
            "16",
            r#"<path d="M22 2 11 13"/><path d="M22 2 15 22 11 13 2 9 22 2z"/>"#,
        ),
        // ── Chat avatars (self-contained: colored circle + white glyph) ──
        // Пользователь.
        "avatar-user" => avatar_icon(
            r##"<circle cx="14" cy="14" r="14" fill="#6b7280"/><g fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 20.5v-1a3 3 0 0 0-3-3h-6a3 3 0 0 0-3 3v1"/><circle cx="14" cy="10" r="3.2"/></g>"##,
        ),
        // Ассистент (робот).
        "avatar-assistant" => avatar_icon(
            r##"<circle cx="14" cy="14" r="14" fill="#3f6ad8"/><g fill="none" stroke="#fff" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="7.5" y="10" width="13" height="9.5" rx="2.5"/><path d="M14 7v3"/><circle cx="14" cy="6.4" r="1.1" fill="#fff" stroke="none"/><path d="M7.5 14.5h-1"/><path d="M21.5 14.5h-1"/></g><circle cx="11.3" cy="14.6" r="1.2" fill="#fff"/><circle cx="16.7" cy="14.6" r="1.2" fill="#fff"/>"##,
        ),
        // Прикреплённый контекст (документ).
        "avatar-context" => avatar_icon(
            r##"<circle cx="14" cy="14" r="14" fill="#9ca3af"/><g fill="none" stroke="#fff" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M16 7.5h-4.5A1.5 1.5 0 0 0 10 9v10a1.5 1.5 0 0 0 1.5 1.5h5A1.5 1.5 0 0 0 18 19V9.5z"/><path d="M15.8 7.5V10H18.3"/></g>"##,
        ),
        // Гаечный ключ (Lucide wrench) — инструменты.
        "wrench" | "tool" => stroke_icon(
            "16",
            r#"<path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"/>"#,
        ),
        "microphone" | "mic" => stroke_icon(
            "16",
            r#"<path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><line x1="12" y1="19" x2="12" y2="22"/>"#,
        ),
        "mic-off" => stroke_icon(
            "16",
            r#"<line x1="2" y1="2" x2="22" y2="22"/><path d="M18.89 13.23A7 7 0 0 0 19 12v-2"/><path d="M5 10v2a7 7 0 0 0 12 5"/><path d="M15 9.34V5a3 3 0 0 0-5.68-1.33"/><path d="M9 9v3a3 3 0 0 0 5.12 2.12"/><line x1="12" y1="19" x2="12" y2="22"/>"#,
        ),
        // ── База знаний ──────────────────────────────────────────────────
        // Lucide book-open — раскрытая книга.
        "book-open" => stroke_icon(
            "16",
            r#"<path d="M12 7v14"/><path d="M3 18a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h5a4 4 0 0 1 4 4 4 4 0 0 1 4-4h5a1 1 0 0 1 1 1v13a1 1 0 0 1-1 1h-6a3 3 0 0 0-3 3 3 3 0 0 0-3-3z"/>"#,
        ),
        // Lucide book-open-text — книга со строками текста.
        "book-open-text" => stroke_icon(
            "16",
            r#"<path d="M12 7v14"/><path d="M16 12h2"/><path d="M16 8h2"/><path d="M3 18a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h5a4 4 0 0 1 4 4 4 4 0 0 1 4-4h5a1 1 0 0 1 1 1v13a1 1 0 0 1-1 1h-6a3 3 0 0 0-3 3 3 3 0 0 0-3-3z"/><path d="M6 8h2"/><path d="M6 12h2"/>"#,
        ),
        // ── LLM ──────────────────────────────────────────────────────────
        // Lucide message-square — прямоугольный чат-пузырь.
        "message-square" => stroke_icon(
            "16",
            r#"<path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>"#,
        ),
        // Lucide bot — робот/агент.
        "robot" | "bot" => stroke_icon(
            "16",
            r#"<path d="M12 8V4H8"/><rect width="16" height="12" x="4" y="8" rx="2"/><path d="M2 14h2"/><path d="M20 14h2"/><path d="M15 13v2"/><path d="M9 13v2"/>"#,
        ),
        // ── Система ──────────────────────────────────────────────────────
        // Lucide shield — щит.
        "shield" => stroke_icon(
            "16",
            r#"<path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"/>"#,
        ),
        // Lucide shield-check — щит с галочкой.
        "shield-check" => stroke_icon(
            "16",
            r#"<path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"/><path d="m9 12 2 2 4-4"/>"#,
        ),
        // Lucide circle-check — круг с галочкой (quality checks).
        "check-circle" | "circle-check" => stroke_icon(
            "16",
            r#"<circle cx="12" cy="12" r="10"/><path d="m9 12 2 2 4-4"/>"#,
        ),
        // Lucide calendar — календарь (задачи).
        "calendar" => stroke_icon(
            "16",
            r#"<path d="M8 2v4"/><path d="M16 2v4"/><rect width="18" height="18" x="3" y="4" rx="2"/><path d="M3 10h18"/>"#,
        ),
        // Lucide calendar-check — календарь с галочкой (a033 закрытие дня).
        "calendar-check" => stroke_icon(
            "16",
            r#"<path d="M8 2v4"/><path d="M16 2v4"/><rect width="18" height="18" x="3" y="4" rx="2"/><path d="M3 10h18"/><path d="m9 16 2 2 4-4"/>"#,
        ),
        // Lucide ruler — линейка (выбор периода / «измерить»).
        "ruler" => stroke_icon(
            "16",
            r#"<path d="M21.3 15.3a2.4 2.4 0 0 1 0 3.4l-2.6 2.6a2.4 2.4 0 0 1-3.4 0L2.7 8.7a2.41 2.41 0 0 1 0-3.4l2.6-2.6a2.41 2.41 0 0 1 3.4 0Z"/><path d="m14.5 12.5 2-2"/><path d="m11.5 9.5 2-2"/><path d="m8.5 6.5 2-2"/><path d="m17.5 15.5 2-2"/>"#,
        ),
        _ => stroke_icon(
            "16",
            r#"<circle cx="12" cy="12" r="10"/><path d="M12 8v4l3 3"/>"#,
        ),
    }
}
