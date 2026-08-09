//! Template-based renderer for BI indicator cards.
//!
//! One universal HTML template is combined with one of the style packs:
//!   - assets/dashboards/classic.css
//!   - assets/dashboards/modern.css
//!   - assets/dashboards/retro.css
//!   - assets/dashboards/future.css
//! Optional per-indicator custom CSS is supported via design `custom`.

use super::spark::{demo_spark_points, points_to_svg_path};
use super::IndicatorCardParams;

const CLASSIC_CSS: &str = include_str!("../../../assets/dashboards/classic.css");
const MODERN_CSS: &str = include_str!("../../../assets/dashboards/modern.css");
const RETRO_CSS: &str = include_str!("../../../assets/dashboards/retro.css");
const FUTURE_CSS: &str = include_str!("../../../assets/dashboards/future.css");
const INDICATOR_HTML: &str = include_str!("../../../assets/dashboards/indicator.html");

const ARROW_UP: &str = r#"<svg viewBox="0 0 24 24" fill="none" aria-hidden="true" style="width:14px;height:14px"><path d="M7 14l5-5 5 5" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;
const ARROW_DOWN: &str = r#"<svg viewBox="0 0 24 24" fill="none" aria-hidden="true" style="width:14px;height:14px"><path d="M7 10l5 5 5-5" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;
const ARROW_FLAT: &str = r#"<svg viewBox="0 0 24 24" fill="none" aria-hidden="true" style="width:14px;height:14px"><path d="M6 12h12" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"/></svg>"#;

pub fn render_srcdoc(params: &IndicatorCardParams) -> String {
    let style_name = normalize_style_name(&params.style_name, params.custom_css.as_deref());
    let card = render_base_card_html(params, style_name);
    let extra_css = if style_name == "custom" {
        params.custom_css.as_deref()
    } else {
        None
    };
    wrap_in_page(get_style_css(style_name), &card, &params.theme, extra_css)
}

pub fn render_card_html(params: &IndicatorCardParams) -> String {
    let style_name = normalize_style_name(&params.style_name, params.custom_css.as_deref());
    let card = render_base_card_html(params, style_name);
    if style_name == "custom" {
        if let Some(user_css) = params.custom_css.as_deref() {
            if !user_css.trim().is_empty() {
                let safe_css = user_css.replace("</style", "<\\/style");
                return format!(r#"<style>{safe_css}</style>{card}"#);
            }
        }
    }
    card
}

pub fn get_style_css(style_name: &str) -> &'static str {
    match style_name {
        "modern" => MODERN_CSS,
        "retro" => RETRO_CSS,
        "future" => FUTURE_CSS,
        "custom" => MODERN_CSS,
        _ => CLASSIC_CSS,
    }
}

fn normalize_style_name<'a>(style_name: &'a str, custom_css: Option<&str>) -> &'a str {
    match style_name {
        "classic" | "modern" | "retro" | "future" => style_name,
        "custom" => {
            if custom_css.unwrap_or_default().trim().is_empty() {
                "classic"
            } else {
                "custom"
            }
        }
        _ => "classic",
    }
}

fn render_base_card_html(p: &IndicatorCardParams, style_name: &str) -> String {
    let graph_type = p.graph_type.min(2);
    let spark_pts = if graph_type == 2 {
        if p.spark_points.is_empty() {
            demo_spark_points()
        } else {
            p.spark_points.clone()
        }
    } else {
        vec![]
    };
    let (spark_line, spark_fill) = points_to_svg_path(&spark_pts);

    let delta_arrow = match p.delta_dir.as_str() {
        "down" => ARROW_DOWN,
        "flat" => ARROW_FLAT,
        _ => ARROW_UP,
    };
    let delta_neutral_class = if p.delta_dir == "flat" { "neutral" } else { "" };

    INDICATOR_HTML
        .replace("{{col_class}}", &p.col_class)
        .replace("{{status}}", &p.status)
        .replace("{{graph_type}}", &graph_type.to_string())
        .replace("{{style_key}}", style_name)
        .replace("{{name}}", &p.name)
        .replace("{{meta_1}}", &p.meta_1)
        .replace("{{meta_2}}", &p.meta_2)
        .replace("{{chip}}", &p.chip)
        .replace("{{value}}", &p.value)
        .replace("{{unit}}", &p.unit)
        .replace("{{delta}}", &p.delta)
        .replace("{{delta_arrow_svg}}", delta_arrow)
        .replace("{{delta_neutral_class}}", delta_neutral_class)
        .replace("{{spark_line}}", &spark_line)
        .replace("{{spark_fill}}", &spark_fill)
        .replace("{{progress}}", &p.progress.to_string())
        .replace("{{hint}}", &p.hint)
        .replace("{{footer_1}}", &p.footer_1)
        .replace("{{footer_2}}", &p.footer_2)
}

fn wrap_in_page(css: &str, card: &str, theme: &str, extra_css: Option<&str>) -> String {
    // theme приходит уже схлопнутым до базы ("dark"|"light"), см. get_app_theme
    // у вызывающих; резолв через реестр страхует от сырых имён тем.
    let theme = crate::shared::theme::registry::theme_by_id(theme)
        .base
        .as_str();
    let extra_css = extra_css
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| v.replace("</style", "<\\/style"))
        .unwrap_or_default();
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="UTF-8">
<style>{css}</style>
<style>{extra_css}</style>
</head>
<body data-theme="{theme}" data-theme-base="{theme}">
<div style="width:min(280px,100%)">
{card}
</div>
</body>
</html>"#,
        css = css,
        extra_css = extra_css,
        theme = theme,
        card = card,
    )
}
