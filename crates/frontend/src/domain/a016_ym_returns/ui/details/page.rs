//! Main page component for YM Returns details (MVVM Standard, mirrors a015_wb_orders)

use super::tabs::{GeneralTab, JsonTab, LinesTab, ProjectionsTab};
use super::view_model::YmReturnDetailsVm;
use crate::layout::global_context::AppGlobalContext;
use crate::shared::icons::icon;
use crate::shared::page_frame::PageFrame;
use crate::system::favorites::ui::FavoriteButton;
use leptos::prelude::*;
use thaw::*;
use wasm_bindgen::JsCast;

#[component]
pub fn YmReturnDetail(id: String, #[prop(into)] on_close: Callback<()>) -> impl IntoView {
    let vm = YmReturnDetailsVm::new();
    let tabs_store =
        leptos::context::use_context::<AppGlobalContext>().expect("AppGlobalContext not found");
    let stored_id = StoredValue::new(id.clone());

    vm.load(id);

    // Синхронизация заголовка вкладки
    Effect::new({
        let vm = vm.clone();
        move || {
            if vm.return_data.get().is_some() {
                let tab_key = format!("a016_ym_returns_details_{}", stored_id.get_value());
                let tab_title = vm.title().get();
                tabs_store.update_tab_title(&tab_key, &tab_title);
            }
        }
    });

    // Ленивая загрузка raw JSON при переходе на вкладку JSON
    Effect::new({
        let vm = vm.clone();
        move || {
            if vm.active_tab.get() == "json" && !vm.raw_json_loaded.get() {
                vm.load_raw_json();
            }
        }
    });

    let vm_header = vm.clone();
    let vm_tabs = vm.clone();
    let vm_content = vm.clone();

    view! {
        <PageFrame page_id="a016_ym_returns--detail" category="detail">
            <Header
                vm=vm_header
                favorite_target_id=stored_id.get_value()
                on_close=on_close
            />

            <TabBar vm=vm_tabs />

            <div class="page__content">
                {move || {
                    if vm.loading.get() {
                        view! {
                            <Flex gap=FlexGap::Small style="align-items: center; padding: var(--spacing-4xl); justify-content: center;">
                                <Spinner />
                                <span>"Загрузка..."</span>
                            </Flex>
                        }
                        .into_any()
                    } else if let Some(err) = vm.error.get() {
                        view! {
                            <div style="padding: var(--spacing-lg); background: var(--color-error-50); border: 1px solid var(--color-error-100); border-radius: var(--radius-sm); color: var(--color-error); margin: var(--spacing-lg);">
                                <strong>"Ошибка: "</strong>{err}
                            </div>
                        }
                        .into_any()
                    } else if vm.return_data.get().is_some() {
                        view! {
                            <TabContent vm=vm_content.clone() />
                        }
                        .into_any()
                    } else {
                        view! { <div>"Нет данных"</div> }.into_any()
                    }
                }}
            </div>
        </PageFrame>
    }
}

#[component]
fn Header(
    vm: YmReturnDetailsVm,
    favorite_target_id: String,
    on_close: Callback<()>,
) -> impl IntoView {
    let is_posted = vm.is_posted();
    let title = vm.title();
    let return_data = vm.return_data;
    let tab_key = format!("a016_ym_returns_details_{}", favorite_target_id);

    view! {
        <div class="page__header">
            <div class="page__header-left">
                <FavoriteButton
                    target_kind="a016_ym_returns_details".to_string()
                    target_id=favorite_target_id
                    target_title=title
                    tab_key=tab_key
                />
                <h1 class="page__title">{move || title.get()}</h1>
                <Show when=move || return_data.get().is_some()>
                    {move || {
                        let posted = is_posted.get();
                        view! {
                            <Badge
                                appearance=BadgeAppearance::Filled
                                color=if posted { BadgeColor::Success } else { BadgeColor::Warning }
                            >
                                {if posted { "Проведен" } else { "Не проведен" }}
                            </Badge>
                        }
                    }}
                </Show>
            </div>
            <div class="page__header-right">
                <PostButtons vm=vm.clone() />

                <Button
                    appearance=ButtonAppearance::Subtle
                    size=ButtonSize::Medium
                    on_click=move |_| on_close.run(())
                >
                    <span class="page-action-button__content">
                        <span class="page-action-button__icon page-action-button__icon--close">{icon("x")}</span>
                        <span class="page-action-button__text">"Закрыть"</span>
                    </span>
                </Button>
            </div>
        </div>
    }
}

#[component]
fn PostButtons(vm: YmReturnDetailsVm) -> impl IntoView {
    let posting = vm.posting;
    let return_data = vm.return_data;
    let is_posted = vm.is_posted();

    let on_post = {
        let vm = vm.clone();
        Callback::new(move |_: ()| vm.post())
    };
    let on_unpost = {
        let vm = vm.clone();
        Callback::new(move |_: ()| vm.unpost())
    };

    view! {
        <Show when=move || return_data.get().is_some()>
            <Button
                appearance=ButtonAppearance::Subtle
                size=ButtonSize::Medium
                on_click=move |_| on_post.run(())
                disabled=Signal::derive(move || posting.get())
            >
                <span class="page-action-button__content">
                    <span class="page-action-button__icon">
                        {move || {
                            if posting.get() {
                                view! { <span class="page-action-button__spinner"></span> }.into_any()
                            } else {
                                view! { <>{icon("refresh-cw")}</> }.into_any()
                            }
                        }}
                    </span>
                    <span class="page-action-button__text page-action-button__text--post">"Post"</span>
                </span>
            </Button>
            <Show when=move || is_posted.get()>
                <Button
                    appearance=ButtonAppearance::Subtle
                    size=ButtonSize::Medium
                    on_click=move |_| on_unpost.run(())
                    disabled=Signal::derive(move || posting.get())
                >
                    <span class="page-action-button__content">
                        <span class="page-action-button__icon">{icon("x")}</span>
                        <span class="page-action-button__text">"Unpost"</span>
                    </span>
                </Button>
            </Show>
        </Show>
    }
}

// ════════════════════════════════════════════════════════════════════════
// Наклонные ярлыки вкладок (эксперимент; цвета и раскладка —
// `static/pages/a016_ym_returns.css`).
//
// Всю графику бара рисует ОДНА svg поверх него: две ломаные (контур и его
// псевдотень) и по заливке на ярлык. Кнопки остаются обычным HTML — они
// задают раскладку, держат подписи и клики, но ничего не рисуют.
//
// Почему так, а не фигура из кусков на каждый ярлык: ширина ярлыка берётся
// из текста, а SVG-путь не умеет на неё сослаться. Раньше форма собиралась
// из уголков фиксированного размера и тянущейся середины, и каждый стык
// между ними давал дефект — то шов, то разная толщина, то двойная
// плотность тени в углу. Здесь стыков нет вообще: координаты снимаются с
// разметки (`measure`), и обе ломаные строятся целиком, от края до края.
//
// Побочная выгода: геометрия стала параметрической — угол, радиус и
// толщины задаются константами ниже, магических чисел в путях больше нет.
// ════════════════════════════════════════════════════════════════════════

/// Высота ярлыка. Продублирована в CSS (`--tab-h`) ради раскладки —
/// синхронизировать при правке. То же про `SHADOW_H` и `--tab-shadow-h`.
const TAB_H: f64 = 30.0;
/// Толщина псевдотени. Она же зазор над ярлыками: бар выше на неё, иначе
/// тени над верхней кромкой активного некуда лечь.
const SHADOW_H: f64 = 3.0;
const LINE_W: f64 = 1.0;
const RADIUS: f64 = 5.0;
/// Наклон боковых граней от вертикали.
const ANGLE_DEG: f64 = 15.0;
const BAR_H: f64 = TAB_H + SHADOW_H;

const TAB_KEYS: [&str; 4] = ["general", "lines", "projections", "json"];

/// Ломаная «верхнего среза страницы»: идёт по бару и поднимается на
/// активный ярлык. Хранит смещения относительно левого края ярлыка, чтобы
/// одну и ту же форму можно было приложить к любому из них.
///
/// `out` — сдвиг НАРУЖУ по нормали: 0 даёт сам контур, положительное
/// значение — параллельную ломаную для тени. Сдвигать надо именно по
/// нормали: скос отклонён от вертикали всего на 15°, и подъём копии по `y`
/// увёл бы её почти вдоль самой линии (просвет вышел бы `h · sin15°`).
struct Edge {
    /// Центр линии на прямых участках.
    y_line: f64,
    /// Центр линии на верху ярлыка.
    y_top: f64,
    /// Где ломаная сходит с прямого участка (у контура — ровно край ярлыка).
    vx: f64,
    /// Точка касания скругления со скосом.
    ax: f64,
    ay: f64,
    /// Точка касания скругления с верхом.
    bx: f64,
    r: f64,
}

fn edge(out: f64) -> Edge {
    let a = ANGLE_DEG.to_radians();
    let (sin_a, cos_a, tan_a) = (a.sin(), a.cos(), a.tan());

    let y_line = BAR_H - LINE_W / 2.0 - out;
    let y_top = SHADOW_H + LINE_W / 2.0 - out;

    // Смещённый скос: точка на нём и его уравнение x(y).
    let px = -out * cos_a;
    let py = BAR_H - LINE_W / 2.0 - out * sin_a;
    let at = |y: f64| px + (py - y) * tan_a;

    // Внутренний угол между скосом и верхом = 90° + a, отсюда отход до
    // точек касания скругления: t = r / tan(угол / 2).
    let r = RADIUS + out;
    let t = r / (std::f64::consts::FRAC_PI_4 + a / 2.0).tan();

    Edge {
        y_line,
        y_top,
        vx: at(y_line),
        ax: at(y_top) - t * sin_a,
        ay: y_top + t * cos_a,
        bx: at(y_top) + t,
        r,
    }
}

/// Подъём на ярлык: от его левого нижнего угла до правого нижнего.
/// Перо уже должно стоять в `(l + vx, y_line)`.
fn ridge(e: &Edge, l: f64, r: f64) -> String {
    // Страховка от вырождения на совсем узком ярлыке: верх не должен
    // вывернуться наизнанку.
    let mid = (l + r) / 2.0;
    let tl = (l + e.bx).min(mid);
    let tr = (r - e.bx).max(mid);
    format!(
        "L {:.2} {:.2} A {rr:.2} {rr:.2} 0 0 1 {:.2} {:.2} \
         L {:.2} {:.2} A {rr:.2} {rr:.2} 0 0 1 {:.2} {:.2} L {:.2} {:.2}",
        l + e.ax,
        e.ay,
        tl,
        e.y_top,
        tr,
        e.y_top,
        r - e.ax,
        e.ay,
        r - e.vx,
        e.y_line,
        rr = e.r,
    )
}

/// Ломаная через весь бар с подъёмом на активный ярлык `[l, r]`.
fn contour(e: &Edge, bar_w: f64, l: f64, r: f64) -> String {
    format!(
        "M 0 {:.2} L {:.2} {:.2} {} L {:.2} {:.2}",
        e.y_line,
        l + e.vx,
        e.y_line,
        ridge(e, l, r),
        bar_w,
        e.y_line,
    )
}

/// Заливка одного ярлыка. Активный доводим до низа бара — он и есть
/// страница, линии под ним нет; остальные обрываем на линии.
fn tab_fill(e: &Edge, l: f64, r: f64, active: bool) -> String {
    if active {
        format!(
            "M {:.2} {:.2} L {:.2} {:.2} {} L {:.2} {:.2} Z",
            l,
            BAR_H,
            l,
            e.y_line,
            ridge(e, l, r),
            r,
            BAR_H,
        )
    } else {
        format!("M {:.2} {:.2} {} Z", l, e.y_line, ridge(e, l, r))
    }
}

/// Снимок раскладки: ширина бара и (левый край, ширина) каждого ярлыка.
/// `offset_*` целочисленные — для декоративной подложки это даже кстати,
/// линии ложатся на пиксельную сетку.
fn measure(bar: &web_sys::HtmlElement) -> Option<(f64, Vec<(f64, f64)>)> {
    let list = bar.query_selector_all(".page__tab").ok()?;
    let mut tabs = Vec::with_capacity(list.length() as usize);
    for i in 0..list.length() {
        let node = list.item(i)?;
        let el = node.dyn_ref::<web_sys::HtmlElement>()?;
        tabs.push((el.offset_left() as f64, el.offset_width() as f64));
    }
    if tabs.is_empty() {
        return None;
    }
    Some((bar.offset_width() as f64, tabs))
}

#[derive(Clone, PartialEq)]
struct TabArt {
    line: String,
    shadow: String,
    fills: Vec<String>,
    active_fill: String,
}

#[component]
fn TabBar(vm: YmReturnDetailsVm) -> impl IntoView {
    let active_tab = vm.active_tab;
    let projections_count = vm.projections_count();

    let bar = NodeRef::<leptos::html::Div>::new();
    let layout = RwSignal::new(None::<(f64, Vec<(f64, f64)>)>);
    let hovered = RwSignal::new(None::<usize>);

    let remeasure = move || {
        if let Some(el) = bar.get_untracked() {
            layout.set(measure(&el));
        }
    };

    // Ширина ярлыка зависит от подписи и от счётчика в бейдже, поэтому
    // пересчитываем при их изменении. Смена вкладки на ширину не влияет,
    // но стоит дёшево и страхует от поздней раскладки (шрифты, первый кадр).
    Effect::new(move |_| {
        let _ = active_tab.get();
        let _ = projections_count.get();
        remeasure();
    });

    // Зум и изменение размера окна меняют пиксельные ширины.
    let resize = window_event_listener(leptos::ev::resize, move |_| remeasure());
    on_cleanup(move || resize.remove());

    let active_idx =
        Memo::new(move |_| TAB_KEYS.iter().position(|k| *k == active_tab.get()).unwrap_or(0));

    let art = Memo::new(move |_| {
        let (bar_w, tabs) = layout.get()?;
        let i = active_idx.get();
        let (al, aw) = *tabs.get(i)?;
        let e = edge(0.0);
        // Тень идёт впритык снаружи: смещение = половина линии + половина тени.
        let s = edge(LINE_W / 2.0 + SHADOW_H / 2.0);
        Some(TabArt {
            line: contour(&e, bar_w, al, al + aw),
            shadow: contour(&s, bar_w, al, al + aw),
            // У активного заливку рисует отдельный путь поверх всех — иначе
            // при нахлёсте её перекрыл бы следующий по разметке сосед.
            fills: tabs
                .iter()
                .enumerate()
                .map(|(j, &(l, w))| {
                    if j == i {
                        String::new()
                    } else {
                        tab_fill(&e, l, l + w, false)
                    }
                })
                .collect(),
            active_fill: tab_fill(&e, al, al + aw, true),
        })
    });
    let part = move |pick: fn(&TabArt) -> String| move || art.get().map(|a| pick(&a)).unwrap_or_default();

    view! {
        <div class="page__tabs" node_ref=bar>
            // Вся графика бара. `d` анимируется переходом, поэтому при смене
            // вкладки контур с тенью переезжают, а не перескакивают.
            <svg class="a016-tabs__art" aria-hidden="true">
                {(0..TAB_KEYS.len())
                    .map(|i| {
                        view! {
                            <path
                                class="a016-tabs__fill"
                                class:a016-tabs__fill--hover=move || hovered.get() == Some(i)
                                d=move || {
                                    art.get().and_then(|a| a.fills.get(i).cloned()).unwrap_or_default()
                                }
                            />
                        }
                    })
                    .collect_view()}
                <path
                    class="a016-tabs__fill a016-tabs__fill--active"
                    d=part(|a| a.active_fill.clone())
                />
                <path
                    class="a016-tabs__shadow"
                    stroke-width=SHADOW_H
                    d=part(|a| a.shadow.clone())
                />
                <path class="a016-tabs__line" stroke-width=LINE_W d=part(|a| a.line.clone()) />
            </svg>
            <button
                class="page__tab"
                class:page__tab--active=move || active_tab.get() == "general"
                on:click={
                    let vm = vm.clone();
                    move |_| vm.set_tab("general")
                }
                on:mouseenter=move |_| hovered.set(Some(0))
                on:mouseleave=move |_| hovered.set(None)
            >
                {icon("file-text")} "Общие"
            </button>

            <button
                class="page__tab"
                class:page__tab--active=move || active_tab.get() == "lines"
                on:click={
                    let vm = vm.clone();
                    move |_| vm.set_tab("lines")
                }
                on:mouseenter=move |_| hovered.set(Some(1))
                on:mouseleave=move |_| hovered.set(None)
            >
                {icon("list")} "Товары"
            </button>

            <button
                class="page__tab"
                class:page__tab--active=move || active_tab.get() == "projections"
                on:click={
                    let vm = vm.clone();
                    move |_| vm.set_tab("projections")
                }
                on:mouseenter=move |_| hovered.set(Some(2))
                on:mouseleave=move |_| hovered.set(None)
            >
                {icon("layers")} "Проекции"
                <Badge
                    appearance=BadgeAppearance::Tint
                    color=Signal::derive({
                        let active_tab = active_tab;
                        move || if active_tab.get() == "projections" {
                            BadgeColor::Brand
                        } else {
                            BadgeColor::Informative
                        }
                    })
                    attr:style="margin-left: 6px;"
                >
                    {move || projections_count.get().to_string()}
                </Badge>
            </button>

            <button
                class="page__tab"
                class:page__tab--active=move || active_tab.get() == "json"
                on:click=move |_| vm.set_tab("json")
                on:mouseenter=move |_| hovered.set(Some(3))
                on:mouseleave=move |_| hovered.set(None)
            >
                {icon("code")} "JSON"
            </button>
        </div>
    }
}

#[component]
fn TabContent(vm: YmReturnDetailsVm) -> impl IntoView {
    let active_tab = vm.active_tab;

    // UI-состояние сортировки таблиц (только для этого компонента)
    let (lines_sort_column, set_lines_sort_column) = signal::<Option<&'static str>>(None);
    let (lines_sort_asc, set_lines_sort_asc) = signal(true);
    let (proj_sort_column, set_proj_sort_column) = signal::<Option<&'static str>>(None);
    let (proj_sort_asc, set_proj_sort_asc) = signal(true);

    let vm_general = vm.clone();
    let return_data = vm.return_data;
    let projections = vm.projections;
    let projections_loading = vm.projections_loading;
    let raw_json = vm.raw_json;

    view! {
        {move || match active_tab.get() {
            "general" => view! { <GeneralTab vm=vm_general.clone() /> }.into_any(),
            "lines" => {
                let lines = return_data.get().map(|d| d.lines).unwrap_or_default();
                view! {
                    <LinesTab
                        lines=lines
                        sort_column=lines_sort_column.into()
                        set_sort_column=set_lines_sort_column
                        sort_asc=lines_sort_asc.into()
                        set_sort_asc=set_lines_sort_asc
                    />
                }
                .into_any()
            }
            "projections" => view! {
                <ProjectionsTab
                    projections=projections.into()
                    projections_loading=projections_loading.into()
                    sort_column=proj_sort_column.into()
                    set_sort_column=set_proj_sort_column
                    sort_asc=proj_sort_asc.into()
                    set_sort_asc=set_proj_sort_asc
                />
            }
            .into_any(),
            "json" => view! { <JsonTab raw_json=raw_json.into() /> }.into_any(),
            _ => view! { <GeneralTab vm=vm_general.clone() /> }.into_any(),
        }}
    }
}
