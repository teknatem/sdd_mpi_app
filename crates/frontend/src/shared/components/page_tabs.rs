//! `PageTabs` — полоса закладок уровня страницы (UI-003) в двух вариантах.
//!
//! `Light` — компактные вкладки с нижней полоской (та же геометрия, что у
//! `.detail-tabs` / sys_tasks), высота 32px. `Heavy` — наклонные ярлыки-
//! «черепица»: левая грань вертикальная, скошена только правая, соседи стоят
//! внахлёст, активный вырастает от кромки листа. Heavy выше на поле под тень.
//!
//! `Heavy` — просьба, а не гарантия: он вырождается в `Light` в двух
//! случаях — когда не хватает ширины и когда активна весёлая тема
//! (`kind=fancy`), где тяжёлая графика спорит с фоновой картинкой. Вид
//! темы берётся реактивно из `ThemeContext`, поэтому переключение темы
//! перестраивает бар на месте.
//!
//! ── Разделение ответственности ──────────────────────────────────────────
//! Кнопки задают раскладку (ширину диктует подпись), держат текст, иконку,
//! бейдж и клики. У `Heavy` поверх бара лежит одна декоративная `svg.tabs-art`:
//! она снимает координаты кнопок (`measure`) и строит по ним графику целиком.
//! Стыков в графике нет. Геометрия (высота, скос, нахлёст, левый отступ) живёт
//! константами в этом файле и прокидывается на бар CSS-переменными; CSS только
//! потребляет `--tabs-*`.
//!
//! У Heavy коробка кнопки включает скос (`padding-right: var(--tabs-slant)`),
//! соседи стоят внахлёст, `clip-path` совпадает с ярлыком — клик по скосу
//! достаётся тому ярлыку, который нарисован сверху.
//!
//! ── Фон ─────────────────────────────────────────────────────────────────
//! Верх бара компонент НЕ красит: SVG прозрачна, фон даёт сам `.page__tabs`
//! (`var(--tabs-bar-bg)`, в светлой теме — градиент, который в `fill` не
//! положить). Снизу активный ярлык красится `--tabs-sheet` — токеном той
//! области, что лежит под баром. Совпадение фона сверху и снизу выходит по
//! построению, а не подбором цвета; разбор — у `.page__tabs` в `layout.css`.
//! Все токены — там же.
//!
//! Пример:
//! ```rust
//! <PageTabs
//!     tabs=vec![
//!         TabItem::new("general", "Общие").with_icon("file-text"),
//!         TabItem::new("projections", "Проекции").with_badge(count),
//!     ]
//!     active=vm.active_tab.into()
//!     on_select=Callback::new(move |k| vm.set_tab(k))
//!     variant=TabsVariant::Heavy
//! />
//! ```

use crate::shared::icons::icon;
use crate::shared::theme::{ThemeContext, ThemeKind};
use leptos::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use thaw::{Badge, BadgeAppearance, BadgeColor};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;

// ════════════════════════════════════════════════════════════════════════
// Публичный API
// ════════════════════════════════════════════════════════════════════════

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum TabsVariant {
    /// Компактные вкладки с нижней полоской — вид sys_tasks / `.detail-tabs`.
    #[default]
    Light,
    /// Наклонные ярлыки с всплытием активного.
    Heavy,
}

/// Одна закладка. Ключ — `&'static str`: набор вкладок у страницы фиксирован,
/// а строки без аллокаций сравниваются и копируются.
#[derive(Clone)]
pub struct TabItem {
    pub key: &'static str,
    pub label: &'static str,
    /// Имя иконки для `shared::icons::icon`.
    pub icon: Option<&'static str>,
    /// Счётчик справа от подписи. Меняет ширину ярлыка — графика
    /// пересчитывается сама.
    pub badge: Option<Signal<String>>,
    /// Недоступная вкладка: `disabled` на кнопке, пропуск в клавиатурной
    /// навигации. Если активная стала недоступной — выбирается первая живая.
    pub disabled: Option<Signal<bool>>,
}

/// Держит JS callback живым и отключает observer вместе с reactive owner.
struct ResizeObserverGuard {
    observer: web_sys::ResizeObserver,
    _callback: Closure<dyn FnMut()>,
}

impl Drop for ResizeObserverGuard {
    fn drop(&mut self) {
        self.observer.disconnect();
    }
}

impl TabItem {
    pub fn new(key: &'static str, label: &'static str) -> Self {
        Self {
            key,
            label,
            icon: None,
            badge: None,
            disabled: None,
        }
    }

    pub fn with_icon(mut self, name: &'static str) -> Self {
        self.icon = Some(name);
        self
    }

    pub fn with_badge(mut self, badge: Signal<String>) -> Self {
        self.badge = Some(badge);
        self
    }

    pub fn with_disabled(mut self, disabled: Signal<bool>) -> Self {
        self.disabled = Some(disabled);
        self
    }
}

#[component]
pub fn PageTabs(
    tabs: Vec<TabItem>,
    active: Signal<&'static str>,
    on_select: Callback<&'static str>,
    #[prop(optional)] variant: TabsVariant,
) -> impl IntoView {
    let count = tabs.len();
    let keys = StoredValue::new(tabs.iter().map(|t| t.key).collect::<Vec<_>>());
    let disabled_flags: Vec<Option<Signal<bool>>> = tabs.iter().map(|t| t.disabled).collect();

    debug_assert!(!tabs.is_empty(), "PageTabs requires at least one tab");
    debug_assert!(
        tabs.iter()
            .enumerate()
            .all(|(i, tab)| tabs[..i].iter().all(|other| other.key != tab.key)),
        "PageTabs tab keys must be unique",
    );
    debug_assert!(
        tabs.iter().any(|tab| tab.key == active.get_untracked()),
        "PageTabs active key must reference an existing tab",
    );

    let bar = NodeRef::<leptos::html::Div>::new();
    let hovered = RwSignal::new(None::<usize>);

    let active_idx = Memo::new(move |_| {
        let cur = active.get();
        keys.with_value(|k| k.iter().position(|x| *x == cur).unwrap_or(0))
    });

    // В release не оставляем tablist без выбранной и фокусируемой вкладки.
    // Неизвестный ключ или disabled-активная → первая доступная; один раз
    // на каждое такое значение `active`, чтобы не крутиться, если родитель
    // не подхватил.
    let last_normalized = RwSignal::new(None::<&'static str>);
    Effect::new({
        let disabled_flags = disabled_flags.clone();
        move |_| {
            let cur = active.get();
            let idx = keys.with_value(|k| k.iter().position(|x| *x == cur));
            debug_assert!(
                idx.is_some() || count == 0,
                "PageTabs active key must reference an existing tab"
            );

            let active_disabled = idx
                .and_then(|i| disabled_flags.get(i).copied().flatten())
                .map(|s| s.get())
                .unwrap_or(false);

            if idx.is_some() && !active_disabled {
                last_normalized.set(None);
                return;
            }

            let first_enabled = keys.with_value(|k| {
                k.iter()
                    .enumerate()
                    .find(|(i, _)| {
                        !disabled_flags
                            .get(*i)
                            .copied()
                            .flatten()
                            .map(|s| s.get_untracked())
                            .unwrap_or(false)
                    })
                    .map(|(_, key)| *key)
            });

            if let Some(first) = first_enabled {
                if last_normalized.get_untracked() != Some(cur) {
                    last_normalized.set(Some(cur));
                    on_select.run(first);
                }
            }
        }
    });

    // Перевод фокуса при навигации стрелками: кнопок в баре ровно столько же
    // и в том же порядке, поэтому индекс совпадает.
    let focus_tab = move |i: usize| {
        let Some(el) = bar.get_untracked() else {
            return;
        };
        let Ok(list) = el.query_selector_all(".page__tab") else {
            return;
        };
        if let Some(node) = list.item(i as u32) {
            if let Some(html) = node.dyn_ref::<web_sys::HtmlElement>() {
                let _ = html.focus();
            }
        }
    };

    let on_keydown = move |ev: leptos::ev::KeyboardEvent| {
        if count == 0 {
            return;
        }
        let is_disabled = |i: usize| {
            disabled_flags
                .get(i)
                .copied()
                .flatten()
                .map(|s| s.get_untracked())
                .unwrap_or(false)
        };
        let cur = active_idx.get_untracked();
        let next = match ev.key().as_str() {
            "ArrowLeft" => (1..=count).find_map(|step| {
                let i = (cur + count - step) % count;
                (!is_disabled(i)).then_some(i)
            }),
            "ArrowRight" => (1..=count).find_map(|step| {
                let i = (cur + step) % count;
                (!is_disabled(i)).then_some(i)
            }),
            "Home" => (0..count).find(|&i| !is_disabled(i)),
            "End" => (0..count).rev().find(|&i| !is_disabled(i)),
            _ => return,
        };
        let Some(next) = next else {
            return;
        };
        ev.prevent_default();
        keys.with_value(|k| on_select.run(k[next]));
        focus_tab(next);
    };

    // «Черепицу» просят только строгие темы. В весёлых её тяжёлая графика
    // спорит с фоновой картинкой, поэтому там всегда подчёркивания — тот же
    // Light, что и при нехватке ширины. Memo, а не bool: тему переключают на
    // живой странице, и бар обязан перестроиться без переоткрытия вкладки.
    let theme_ctx = leptos::context::use_context::<ThemeContext>();
    let wants_heavy = Memo::new(move |_| {
        let kind = theme_ctx
            .map(|c| c.0.get().kind)
            .unwrap_or(ThemeKind::Strict);
        variant == TabsVariant::Heavy && kind == ThemeKind::Strict
    });
    let display_heavy = RwSignal::new(wants_heavy.get_untracked());

    // Смена темы возвращает бар к запрошенному варианту; дальше решает замер.
    Effect::new(move |_| display_heavy.set(wants_heavy.get()));
    let overflowing = RwSignal::new(false);
    let layout = RwSignal::new(None::<Vec<(f64, f64)>>);
    let heavy_required_width = RwSignal::new(None::<f64>);

    let remeasure = move || {
        let Some(el) = bar.get_untracked() else {
            return;
        };
        let is_heavy = wants_heavy.get_untracked() && display_heavy.get_untracked();
        let measured = measure(&el, is_heavy);
        let client_width = el.client_width() as f64;
        let required_width = measured
            .as_ref()
            .and_then(|tabs| tabs.last())
            .map(|&(l, w)| l + w + slant().0);
        let overflows = if is_heavy {
            required_width.is_some_and(|required| required > client_width)
        } else {
            el.scroll_width() > el.client_width()
        };

        overflowing.set(overflows);
        if !wants_heavy.get_untracked() {
            layout.set(None);
        } else if is_heavy {
            if let Some(required) = required_width {
                heavy_required_width.set(Some(required.ceil()));
            }
            if overflows {
                display_heavy.set(false);
                layout.set(None);
            } else {
                layout.set(measured);
            }
        } else if !overflows
            && heavy_required_width
                .get_untracked()
                .is_some_and(|required| required <= client_width)
        {
            // Помимо Light должен помещаться последний скос Heavy. Запомненный
            // порог не даёт вариантам дребезжать на пограничной ширине.
            display_heavy.set(true);
            layout.set(None);
        }
    };

    let resize_callback = Closure::<dyn FnMut()>::new(move || remeasure());
    let resize_observer =
        web_sys::ResizeObserver::new(resize_callback.as_ref().unchecked_ref()).ok();

    // Без observer Heavy не сможет снять раскладку — остаёмся на Light.
    if resize_observer.is_none() {
        display_heavy.set(false);
    }

    if let Some(observer) = resize_observer.clone() {
        Effect::new(move |_| {
            let Some(el) = bar.get() else { return };
            observer.observe(&el);
            if let Ok(buttons) = el.query_selector_all(".page__tab") {
                for i in 0..buttons.length() {
                    if let Some(button) = buttons.item(i) {
                        if let Some(element) = button.dyn_ref::<web_sys::Element>() {
                            observer.observe(element);
                        }
                    }
                }
            }
            remeasure();
        });
    }

    let _resize_observer =
        StoredValue::new_local(resize_observer.map(|observer| ResizeObserverGuard {
            observer,
            _callback: resize_callback,
        }));

    let buttons = tabs
        .into_iter()
        .enumerate()
        .map(|(i, item)| {
            let key = item.key;
            let disabled = item.disabled;
            let is_active = Memo::new(move |_| active_idx.get() == i);
            let z = count.saturating_sub(i);
            view! {
                <button
                    type="button"
                    class="page__tab"
                    class:page__tab--active=move || active_idx.get() == i
                    role="tab"
                    aria-selected=move || (active_idx.get() == i).to_string()
                    tabindex=move || if active_idx.get() == i { "0" } else { "-1" }
                    disabled=move || disabled.map(|d| d.get()).unwrap_or(false)
                    style=format!("--tabs-z: {z}")
                    on:click=move |_| on_select.run(key)
                    on:mouseenter=move |_| hovered.set(Some(i))
                    on:mouseleave=move |_| hovered.set(None)
                >
                    {item.icon.map(icon)}
                    {item.label}
                    {item
                        .badge
                        .map(|badge| {
                            view! {
                                <Badge
                                    appearance=BadgeAppearance::Tint
                                    color=Signal::derive(move || {
                                        if is_active.get() {
                                            BadgeColor::Brand
                                        } else {
                                            BadgeColor::Informative
                                        }
                                    })
                                >
                                    {move || badge.get()}
                                </Badge>
                            }
                        })}
                </button>
            }
        })
        .collect_view();

    let bar_style = move || {
        if wants_heavy.get() {
            heavy_css_vars()
        } else {
            String::new()
        }
    };

    view! {
        <div
            class=move || {
                if display_heavy.get() {
                    "page__tabs page__tabs--heavy"
                } else if overflowing.get() {
                    "page__tabs page__tabs--light page__tabs--overflow"
                } else {
                    "page__tabs page__tabs--light"
                }
            }
            style=bar_style
            node_ref=bar
            role="tablist"
            aria-orientation="horizontal"
            on:keydown=on_keydown
        >
            {move || {
                (display_heavy.get() && layout.get().is_some()).then(|| {
                    view! {
                        <TabsArt
                            count=count
                            active_idx=active_idx
                            hovered=hovered
                            layout=layout
                        />
                    }
                })
            }}
            {buttons}
        </div>
    }
}

// ════════════════════════════════════════════════════════════════════════
// Геометрия «тяжёлого» варианта
//
// Фигура ярлыка: левая грань вертикальная, верх со скруглениями, правая
// грань — скос наружу. Верхняя грань — ширина подписи с паддингами; скос
// входит в `padding-right` кнопки, поэтому хит-тест совпадает с картинкой.
//
// Числа ниже — единственный источник. На бар они уходят CSS-переменными
// (`heavy_css_vars`); `measure` вычитает скос из `offset_width`, чтобы
// SVG строил верх по ширине подписи, а не по коробке со скосом.
// ════════════════════════════════════════════════════════════════════════

/// Высота ярлыка. На бар уходит как `--tabs-tab-h`.
const TAB_H: f64 = 33.0;
/// Поле над ярлыками: место, куда ложится тень над верхней кромкой активного.
const SHADOW_H: f64 = 3.0;
const BAR_H: f64 = TAB_H + SHADOW_H;
const LINE_W: f64 = 1.0;
const RADIUS: f64 = 5.0;
/// Наклон правой грани от вертикали.
const ANGLE_DEG: f64 = 30.0;
/// На сколько заливка активного заходит ПОД кромку: иначе нижняя половина
/// линии кромки просвечивает сквозь лист.
const LIP: f64 = 2.0;
/// Нахлёст соседних ярлыков по горизонтали (`--tabs-overlap`).
const OVERLAP: f64 = 12.0;
/// Отступ первого ярлыка от левого края бара (`--tabs-pad-left`).
const PAD_LEFT: f64 = 14.0;

/// Центр линии кромки — низ бара.
const Y_LINE: f64 = BAR_H - LINE_W / 2.0;
/// Центр линии на верху ярлыка.
const Y_TOP: f64 = SHADOW_H + LINE_W / 2.0;

/// Геометрия Heavy → CSS-переменные на `.page__tabs`.
fn heavy_css_vars() -> String {
    let (s, _, _) = slant();
    format!(
        "--tabs-tab-h: {TAB_H}px; --tabs-shadow-h: {SHADOW_H}px; --tabs-slant: {s:.2}px; --tabs-overlap: {OVERLAP}px; --tabs-pad-left: {PAD_LEFT}px;"
    )
}

/// Вынос скоса по горизонтали и вектор скругления вдоль него.
///
/// Скругление верхнего правого угла — квадратичная кривая с опорой в самом
/// углу: конец лежит на скосе на расстоянии `RADIUS` от угла. Так же, как
/// слева, только там направление вертикальное.
fn slant() -> (f64, f64, f64) {
    let h = Y_LINE - Y_TOP;
    let s = h * ANGLE_DEG.to_radians().tan();
    let hyp = (s * s + h * h).sqrt();
    (s, RADIUS * s / hyp, RADIUS * h / hyp)
}

/// Верх ярлыка: левый край, скруглённый верх, скос. Перо уже стоит на дне
/// слева, в `(l, ...)`.
fn crown(l: f64, w: f64) -> String {
    let (_, rx, ry) = slant();
    // Страховка от вырождения на совсем узком ярлыке: скругления не должны
    // перехлестнуться.
    let r = RADIUS.min(w / 2.0);
    format!(
        "L {:.2} {:.2} Q {:.2} {:.2} {:.2} {:.2} L {:.2} {:.2} Q {:.2} {:.2} {:.2} {:.2}",
        l,
        Y_TOP + r,
        l,
        Y_TOP,
        l + r,
        Y_TOP,
        l + w - r,
        Y_TOP,
        l + w,
        Y_TOP,
        l + w + rx,
        Y_TOP + ry,
    )
}

/// Замкнутая фигура ярлыка. `lip` уводит дно ниже кромки (для активного).
fn shape(l: f64, w: f64, lip: f64) -> String {
    let (s, _, _) = slant();
    let lip_x = lip * ANGLE_DEG.to_radians().tan();
    format!(
        "M {:.2} {:.2} {} L {:.2} {:.2} Z",
        l,
        Y_LINE + lip,
        crown(l, w),
        l + w + s + lip_x,
        Y_LINE + lip,
    )
}

/// Контур ярлыка без дна — он же продолжение линии кромки по активному.
fn outline(l: f64, w: f64) -> String {
    let (s, _, _) = slant();
    format!(
        "M {:.2} {:.2} {} L {:.2} {:.2}",
        l,
        Y_LINE,
        crown(l, w),
        l + w + s,
        Y_LINE
    )
}

/// Снимок раскладки: `(левый край, ширина верха)` каждого ярлыка.
/// У Heavy из `offset_width` вычитается скос — он в `padding-right`, а верх
/// фигуры равен ширине подписи с боковыми паддингами.
fn measure(bar: &web_sys::HtmlElement, subtract_slant: bool) -> Option<Vec<(f64, f64)>> {
    let list = bar.query_selector_all(".page__tab").ok()?;
    let extra = if subtract_slant { slant().0 } else { 0.0 };
    let mut tabs = Vec::with_capacity(list.length() as usize);
    for i in 0..list.length() {
        let node = list.item(i)?;
        let el = node.dyn_ref::<web_sys::HtmlElement>()?;
        let w = (el.offset_width() as f64 - extra).max(0.0);
        tabs.push((el.offset_left() as f64, w));
    }
    if tabs.is_empty() {
        return None;
    }
    Some(tabs)
}

/// Готовые пути на каждый ярлык: заливка, «ухо» (заливка активного), его
/// контур и центр по X для `transform-origin`.
#[derive(Clone, PartialEq)]
struct Art {
    tabs: Vec<(String, String, String, f64)>,
}

/// Счётчик для уникальных id фильтра и клипа: на странице может оказаться
/// больше одного бара.
static SEQ: AtomicUsize = AtomicUsize::new(0);

#[component]
fn TabsArt(
    count: usize,
    active_idx: Memo<usize>,
    hovered: RwSignal<Option<usize>>,
    layout: RwSignal<Option<Vec<(f64, f64)>>>,
) -> impl IntoView {
    let uid = SEQ.fetch_add(1, Ordering::Relaxed);
    let filter_id = format!("tabs-art-shadow-{uid}");
    let clip_id = format!("tabs-art-clip-{uid}");
    let filter_url = format!("url(#{filter_id})");
    let clip_url = format!("url(#{clip_id})");

    let art = Memo::new(move |_| {
        let tabs = layout.get()?;
        Some(Art {
            tabs: tabs
                .iter()
                .map(|&(l, w)| {
                    (
                        shape(l, w, 0.0),
                        shape(l, w, LIP),
                        outline(l, w),
                        l + w / 2.0,
                    )
                })
                .collect(),
        })
    });

    let path_of = move |i: usize, pick: fn(&(String, String, String, f64)) -> String| {
        move || {
            art.get()
                .and_then(|a| a.tabs.get(i).map(pick))
                .unwrap_or_default()
        }
    };
    let origin_of = move |i: usize| {
        move || {
            let cx = art
                .get()
                .and_then(|a| a.tabs.get(i).map(|t| t.3))
                .unwrap_or(0.0);
            format!("transform-origin: {cx:.2}px {BAR_H:.2}px;")
        }
    };

    view! {
        <svg class="tabs-art" aria-hidden="true">
            <defs>
                // Только тень, без исходной фигуры: `feDropShadow` подмешал бы
                // к тени ещё одну копию заливки, а она на части тем
                // полупрозрачная — цвет активного ярлыка поехал бы.
                <filter id=filter_id.clone() x="-20%" y="-100%" width="140%" height="300%">
                    <feGaussianBlur in="SourceAlpha" stdDeviation="2.5" result="blur" />
                    <feOffset in="blur" dy="-1" result="off" />
                    <feFlood class="tabs-art__flood" result="color" />
                    <feComposite in="color" in2="off" operator="in" />
                </filter>
                // Тень не должна ложиться на лист — обрезаем по кромке.
                <clipPath id=clip_id.clone()>
                    <rect x="0" y="0" width="100%" height=Y_LINE />
                </clipPath>
            </defs>

            // Ярлыки справа налево: левый перекрывает правого.
            {(0..count)
                .rev()
                .map(|i| {
                    view! {
                        <path
                            class="tabs-art__tab"
                            class:tabs-art__tab--hover=move || hovered.get() == Some(i)
                            d=path_of(i, |t| t.0.clone())
                        />
                    }
                })
                .collect_view()}

            <line class="tabs-art__edge" x1="0" y1=Y_LINE x2="100%" y2=Y_LINE />

            // Активный ярлык вырастает от кромки листа.
            {(0..count)
                .rev()
                .map(|i| {
                    let filter_url = filter_url.clone();
                    let clip_url = clip_url.clone();
                    view! {
                        <g
                            class="tabs-art__ear"
                            data-on=move || (active_idx.get() == i).to_string()
                            style=origin_of(i)
                        >
                            <g clip-path=clip_url>
                                <path filter=filter_url d=path_of(i, |t| t.1.clone()) />
                            </g>
                            <path class="tabs-art__ear-fill" d=path_of(i, |t| t.1.clone()) />
                            <path class="tabs-art__ear-line" d=path_of(i, |t| t.2.clone()) />
                        </g>
                    }
                })
                .collect_view()}
        </svg>
    }
}
