//! Реестр клиентских китов: какие статические ассеты соответствуют киту,
//! объявленному в `manifest.client_kits`.
//!
//! Киты — **инфраструктура приложения, а не содержимое бандла**. Файлы лежат в
//! `crates/frontend/static/` (публикуются в `dist/` через `copy-dir` из
//! `index.html`), раздаются `ServeDir` с того же origin и кешируются браузером
//! один раз на все плагины и все перезапуски. Поэтому цена лишнего кита — не
//! трафик и не размер плагина, а **разбор и компиляция JS в каждом открываемом
//! iframe**: байты приходят из кеша, но парсятся заново на каждый документ.
//! Отсюда правило — грузим только объявленное.
//!
//! Реестр задан здесь, на Rust, а исполняется в bootstrap'е iframe:
//! [`kit_assets_json`] инжектится в srcdoc константой `KIT_ASSETS`, и
//! `loadKits()` разбирает её уже в браузере.

use contracts::plugins::{PluginClientKit, PluginManifest};

/// Ассеты одного кита. Порядок внутри `js` значим.
struct KitAssets {
    js: &'static [&'static str],
    css: &'static [&'static str],
}

fn assets(kit: PluginClientKit) -> KitAssets {
    match kit {
        PluginClientKit::Tables => KitAssets {
            js: &["/static/plugin-tables.js"],
            css: &[],
        },
        // chart.umd ставит `window.Chart`, обёртка её читает при загрузке —
        // порядок здесь обязателен, а не косметичен.
        PluginClientKit::Charts => KitAssets {
            js: &[
                "/static/vendor/chartjs/chart.umd.min.js",
                "/static/plugin-charts.js",
            ],
            css: &[],
        },
        PluginClientKit::Flow => KitAssets {
            js: &["/static/plugin-flow.js"],
            css: &["/static/plugin-flow.css"],
        },
    }
}

/// Реестр в виде JSON для инжекта в srcdoc: `{"charts":{"js":[…],"css":[…]},…}`.
pub(super) fn kit_assets_json() -> String {
    let map: serde_json::Map<String, serde_json::Value> = PluginClientKit::ALL
        .iter()
        .map(|kit| {
            let entry = assets(*kit);
            (
                kit.as_str().to_string(),
                serde_json::json!({ "js": entry.js, "css": entry.css }),
            )
        })
        .collect();
    serde_json::to_string(&map).unwrap_or_else(|_| "{}".to_string())
}

/// Имена китов для payload `plugin_init`.
///
/// Разрешение легаси-набора (поле отсутствует → `["tables","charts"]`) живёт в
/// контракте, а не здесь: то же правило нужно и серверной стороне.
pub fn kit_names(manifest: &PluginManifest) -> Vec<String> {
    manifest
        .resolved_client_kits()
        .iter()
        .map(|kit| kit.as_str().to_string())
        .collect()
}
