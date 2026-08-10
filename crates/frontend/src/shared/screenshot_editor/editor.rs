use crate::shared::modal_frame::ModalFrame;
use leptos::html::Canvas;
use leptos::prelude::*;
use std::f64::consts::TAU;
use thaw::{Button, ButtonAppearance};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, HtmlImageElement, MouseEvent};

// Размеры зафиксированы в пикселях (скриншоты — это формы приложения ~1:1),
// а не масштабируются от размера картинки: так толщина рамок и кегль стабильны.
const LINE_W: f64 = 2.0;
const ARROW_LINE_W: f64 = 2.5;
const ARROW_HEAD: f64 = 13.0;
const BADGE_R: f64 = 11.0;
const BADGE_FS: f64 = 13.0;
const PANEL_FS: f64 = 13.0;
const PANEL_LINE_H: f64 = 18.0;
const PANEL_MARGIN: f64 = 12.0;
const PANEL_BADGE_COL: f64 = 32.0;
const PANEL_SEP_H: f64 = 20.0;

/// Палитра аннотаций. Зелёный/красный несут дефолтный комментарий
/// (хорошо/плохо), остальные — чисто визуальное выделение.
const PALETTE: [PenColor; 5] = [
    PenColor::Good,
    PenColor::Bad,
    PenColor::Accent,
    PenColor::Warn,
    PenColor::Muted,
];

/// Цвет аннотации.
#[derive(Clone, Copy, PartialEq)]
enum PenColor {
    /// Зелёный — «хорошо».
    Good,
    /// Красный — «плохо».
    Bad,
    /// Синий — акцент.
    Accent,
    /// Жёлтый — внимание.
    Warn,
    /// Светло-серый — нейтральная пометка.
    Muted,
}

impl PenColor {
    fn stroke(self) -> &'static str {
        match self {
            PenColor::Good => "#22c55e",
            PenColor::Bad => "#ef4444",
            PenColor::Accent => "#3b82f6",
            PenColor::Warn => "#eab308",
            PenColor::Muted => "#94a3b8",
        }
    }

    fn fill(self) -> &'static str {
        match self {
            PenColor::Good => "rgba(34, 197, 94, 0.15)",
            PenColor::Bad => "rgba(239, 68, 68, 0.15)",
            PenColor::Accent => "rgba(59, 130, 246, 0.15)",
            PenColor::Warn => "rgba(234, 179, 8, 0.15)",
            PenColor::Muted => "rgba(148, 163, 184, 0.15)",
        }
    }

    fn label(self) -> &'static str {
        match self {
            PenColor::Good => "Хорошо",
            PenColor::Bad => "Плохо",
            PenColor::Accent => "Акцент",
            PenColor::Warn => "Внимание",
            PenColor::Muted => "Нейтрально",
        }
    }

    /// Комментарий по умолчанию для новой аннотации этого цвета. Только
    /// зелёный/красный самоочевидны; остальные — без текста по умолчанию.
    fn default_comment(self) -> String {
        match self {
            PenColor::Good => "хорошо".to_string(),
            PenColor::Bad => "плохо".to_string(),
            _ => String::new(),
        }
    }
}

/// Тип фигуры-аннотации.
#[derive(Clone, Copy, PartialEq)]
enum Shape {
    Rect,
    Arrow,
}

/// Аннотация. Хранит две нормализованные (0..1) точки: `a` — якорь (верхний
/// левый угол прямоугольника / начало стрелки), `b` — противоположный
/// угол / остриё стрелки. `id` — стабильный ключ списка; номер = позиция+1.
#[derive(Clone)]
struct Annotation {
    id: u64,
    kind: Shape,
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
    color: PenColor,
    comment: String,
}

/// Редактор скриншота: превью с рисованием мышью пронумерованных
/// прямоугольников и стрелок в палитре из 5 цветов + правый статичный drawer со
/// списком аннотаций и редактируемыми комментариями. Номер прямоугольника — в
/// цветном квадрате в углу, номер стрелки — в цветном круге в начале вектора.
/// При подтверждении номера «впекаются» в фигуры, а под изображением
/// дорисовывается панель с нумерованными комментариями (только непустыми),
/// отделённая волнистой линией — машиночитаемая копия правой колонки. Без
/// аннотаций отдаётся исходный файл.
#[component]
pub fn ScreenshotEditor(
    source_file: web_sys::File,
    preview_url: String,
    on_cancel: UnsyncCallback<()>,
    on_confirm: UnsyncCallback<web_sys::File>,
) -> impl IntoView {
    let canvas_ref = NodeRef::<Canvas>::new();
    let annotations = RwSignal::new(Vec::<Annotation>::new());
    let next_id = RwSignal::new(1u64);
    let color = RwSignal::new(PenColor::Good);
    let shape = RwSignal::new(Shape::Rect);
    // Незавершённая фигура: (a_x, a_y, b_x, b_y) нормализованно.
    let drag = RwSignal::new(None::<(f64, f64, f64, f64)>);
    let image = RwSignal::new_local(None::<HtmlImageElement>);
    let error = RwSignal::new(None::<String>);

    let escape_handle = window_event_listener(leptos::ev::keydown, move |ev| {
        if ev.key() == "Escape" {
            ev.prevent_default();
            on_cancel.run(());
        }
    });
    on_cleanup(move || escape_handle.remove());

    // Загрузка изображения: как только оно декодировано — кладём в сигнал,
    // что триггерит первую отрисовку канвы.
    {
        let src = preview_url.clone();
        Effect::new(move |_| {
            let Ok(img) = HtmlImageElement::new() else {
                return;
            };
            let img_for_cb = img.clone();
            let onload = Closure::<dyn FnMut()>::new(move || {
                image.set(Some(img_for_cb.clone()));
            });
            img.set_onload(Some(onload.as_ref().unchecked_ref()));
            onload.forget();
            img.set_src(&src);
            // Картинка могла быть уже в кэше — тогда onload не сработает.
            if img.complete() && img.natural_width() > 0 {
                image.set(Some(img));
            }
        });
    }

    // Отрисовка канвы: фон-изображение + все фигуры с номерами + набросок.
    Effect::new(move |_| {
        let list = annotations.get();
        let drag_v = drag.get();
        let Some(img) = image.get() else {
            return;
        };
        let Some(canvas) = canvas_ref.get() else {
            return;
        };
        let nw = img.natural_width();
        let nh = img.natural_height();
        if nw == 0 || nh == 0 {
            return;
        }
        // Ресайз (и связанное перевыделение буфера) только при реальной смене
        // размеров: правка комментария дёргает перерисовку, но размер тот же.
        if canvas.width() != nw || canvas.height() != nh {
            canvas.set_width(nw);
            canvas.set_height(nh);
        }
        let Ok(Some(obj)) = canvas.get_context("2d") else {
            return;
        };
        let Ok(ctx) = obj.dyn_into::<CanvasRenderingContext2d>() else {
            return;
        };
        let nwf = nw as f64;
        let nhf = nh as f64;
        ctx.clear_rect(0.0, 0.0, nwf, nhf);
        let _ = ctx.draw_image_with_html_image_element(&img, 0.0, 0.0);
        draw_annotations(&ctx, &list, nwf, nhf);
        if let Some((ax, ay, bx, by)) = drag_v {
            draw_annotation(
                &ctx,
                shape.get_untracked(),
                ax,
                ay,
                bx,
                by,
                color.get_untracked(),
                nwf,
                nhf,
                None,
            );
        }
    });

    // Нормализованные координаты события относительно отрисованной канвы.
    let to_norm = move |ev: &MouseEvent| -> Option<(f64, f64)> {
        let canvas = canvas_ref.get()?;
        let rect = canvas.get_bounding_client_rect();
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            return None;
        }
        let x = ((ev.client_x() as f64 - rect.left()) / rect.width()).clamp(0.0, 1.0);
        let y = ((ev.client_y() as f64 - rect.top()) / rect.height()).clamp(0.0, 1.0);
        Some((x, y))
    };

    let on_down = move |ev: MouseEvent| {
        if let Some((x, y)) = to_norm(&ev) {
            drag.set(Some((x, y, x, y)));
        }
    };
    let on_move = move |ev: MouseEvent| {
        if drag.get_untracked().is_none() {
            return;
        }
        if let Some((x, y)) = to_norm(&ev) {
            drag.update(|d| {
                if let Some((_, _, bx, by)) = d {
                    *bx = x;
                    *by = y;
                }
            });
        }
    };
    let commit_drag = move || {
        if let Some((ax, ay, bx, by)) = drag.get_untracked() {
            let kind = shape.get_untracked();
            // Отсекаем случайные клики без протяжки.
            let significant = match kind {
                Shape::Rect => (bx - ax).abs() > 0.005 && (by - ay).abs() > 0.005,
                Shape::Arrow => (bx - ax).hypot(by - ay) > 0.02,
            };
            if significant {
                let id = next_id.get_untracked();
                next_id.set(id + 1);
                let color = color.get_untracked();
                annotations.update(|list| {
                    list.push(Annotation {
                        id,
                        kind,
                        ax,
                        ay,
                        bx,
                        by,
                        color,
                        comment: color.default_comment(),
                    });
                });
            }
        }
        drag.set(None);
    };
    let on_up = move |_ev: MouseEvent| commit_drag();
    let on_leave = move |_ev: MouseEvent| commit_drag();

    // Держим исходный File в локальном хранилище: сам File (JsValue) не Send,
    // а thaw `Button::on_click` требует Send-замыкание — в колбэк попадает
    // только Send-хендл, а не сам файл.
    let source = StoredValue::new_local(source_file);
    let confirm = move |_| {
        let list = annotations.get_untracked();
        // Без аннотаций отдаём исходник — не перекодируем зря.
        if list.is_empty() {
            on_confirm.run(source.get_value());
            return;
        }
        let Some(img) = image.get_untracked() else {
            on_confirm.run(source.get_value());
            return;
        };
        match export_annotated_png(&img, &list) {
            Ok(file) => on_confirm.run(file),
            Err(message) => error.set(Some(message)),
        }
    };

    // Кнопка инструмента = фигура + цвет разом (переключатель фигуры не нужен).
    let tool_button = move |kind: Shape, variant: PenColor, glyph: &'static str| {
        view! {
            <button
                type="button"
                title=format!(
                    "{} · {}",
                    if kind == Shape::Rect { "Рамка" } else { "Стрелка" },
                    variant.label(),
                )
                on:click=move |_| {
                    shape.set(kind);
                    color.set(variant);
                }
                style=move || {
                    let active = shape.get() == kind && color.get() == variant;
                    format!(
                        "display:flex;align-items:center;justify-content:center;width:34px;height:30px;\
                         border-radius:6px;cursor:pointer;font-size:17px;line-height:1;color:{};\
                         border:2px solid {};background:{};",
                        variant.stroke(),
                        if active { variant.stroke() } else { "var(--colorNeutralStroke2)" },
                        if active { variant.fill() } else { "var(--colorNeutralBackground1)" },
                    )
                }
            >
                {glyph}
            </button>
        }
    };

    view! {
        <ModalFrame
            on_close=Callback::new(|_| {})
            close_on_overlay=false
            z_index=3000
            modal_style="width: min(96vw, 1600px); height: 94vh; max-width: none; padding: 0; overflow: hidden; display: flex; flex-direction: column;".to_string()
        >
            <header style="flex: 0 0 auto; display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 12px 18px; border-bottom: 1px solid var(--colorNeutralStroke2);">
                <h2 style="font-size: 18px; margin: 0;">
                    "Редактор скриншота"
                </h2>
                <div style="display: flex; flex: 0 0 auto; gap: 8px;">
                    <Button
                        appearance=ButtonAppearance::Secondary
                        on_click=move |_| on_cancel.run(())
                    >
                        "Отмена"
                    </Button>
                    <Button
                        appearance=ButtonAppearance::Primary
                        on_click=confirm
                    >
                        "ОК"
                    </Button>
                </div>
            </header>
            <div style="flex: 1 1 auto; min-height: 0; display: flex;">
                <main style="flex: 1 1 auto; min-width: 0; display: flex; align-items: center; justify-content: center; padding: 16px; background: var(--colorNeutralBackground3);">
                    <canvas
                        node_ref=canvas_ref
                        on:mousedown=on_down
                        on:mousemove=on_move
                        on:mouseup=on_up
                        on:mouseleave=on_leave
                        style="max-width: 100%; max-height: 100%; object-fit: contain; cursor: crosshair; touch-action: none; user-select: none;"
                    />
                </main>
                // Правый статичный drawer: инструменты + список комментариев.
                <aside style="flex: 0 0 340px; display: flex; flex-direction: column; min-height: 0; border-left: 1px solid var(--colorNeutralStroke2); background: var(--colorNeutralBackground1);">
                    <div style="flex: 0 0 auto; display: flex; flex-direction: column; gap: 8px; padding: 12px; border-bottom: 1px solid var(--colorNeutralStroke2);">
                        <div style="display: flex; align-items: center; gap: 8px;">
                            <span style="font-size:12px;width:60px;color:var(--colorNeutralForeground3);">"Рамки"</span>
                            <div style="display: flex; gap: 6px; flex-wrap: wrap;">
                                {PALETTE.iter().map(|&c| tool_button(Shape::Rect, c, "▭")).collect_view()}
                            </div>
                        </div>
                        <div style="display: flex; align-items: center; gap: 8px;">
                            <span style="font-size:12px;width:60px;color:var(--colorNeutralForeground3);">"Стрелки"</span>
                            <div style="display: flex; gap: 6px; flex-wrap: wrap;">
                                {PALETTE.iter().map(|&c| tool_button(Shape::Arrow, c, "↗")).collect_view()}
                            </div>
                        </div>
                        <span style="font-size: 12px; color: var(--colorNeutralForeground3);">
                            "Зажмите ЛКМ на изображении и растяните фигуру"
                        </span>
                        {move || {
                            let count = annotations.get().len();
                            (count > 0).then(|| view! {
                                <Button
                                    appearance=ButtonAppearance::Subtle
                                    on_click=move |_| annotations.set(Vec::new())
                                >
                                    {format!("Очистить все ({count})")}
                                </Button>
                            })
                        }}
                        {move || error.get().map(|message| view! {
                            <div role="alert" style="color: var(--colorPaletteRedForeground1); font-size: 13px;">
                                {message}
                            </div>
                        })}
                    </div>
                    <div style="flex: 1 1 auto; min-height: 0; overflow-y: auto; padding: 8px;">
                        // Плейсхолдер и список — сиблинги. <For> создаётся один раз
                        // и не пересоздаётся при правке комментария (иначе ввод терял бы фокус).
                        {move || annotations.get().is_empty().then(|| view! {
                            <div style="padding: 16px 8px; font-size: 13px; color: var(--colorNeutralForeground3);">
                                "Пока нет фигур. Нарисуйте рамку или стрелку на изображении — она появится здесь под своим номером."
                            </div>
                        })}
                        <For
                            each=move || annotations.get()
                            key=|a| a.id
                            let:item
                        >
                            <AnnotationRow
                                annotations=annotations
                                id=item.id
                                kind=item.kind
                                color=item.color
                                initial=item.comment.clone()
                            />
                        </For>
                    </div>
                </aside>
            </div>
        </ModalFrame>
    }
}

/// Строка списка аннотаций: динамический номер, редактируемый комментарий
/// (uncontrolled — чтобы ввод не терял фокус при перерисовке) и удаление.
/// Форма бейджа повторяет фигуру: квадрат для рамки, круг для стрелки.
#[component]
fn AnnotationRow(
    annotations: RwSignal<Vec<Annotation>>,
    id: u64,
    kind: Shape,
    color: PenColor,
    initial: String,
) -> impl IntoView {
    let number = move || {
        annotations
            .get()
            .iter()
            .position(|a| a.id == id)
            .map(|i| i + 1)
            .unwrap_or(0)
    };
    let radius = if kind == Shape::Rect { "4px" } else { "50%" };
    view! {
        <div style="display: flex; gap: 8px; padding: 8px; border-bottom: 1px solid var(--colorNeutralStroke2);">
            <div style=format!(
                "flex:0 0 auto;width:24px;height:24px;border-radius:{};background:{};color:#fff;\
                 display:flex;align-items:center;justify-content:center;font-size:12px;font-weight:700;",
                radius, color.stroke(),
            )>
                {number}
            </div>
            <textarea
                placeholder="Комментарий…"
                rows="2"
                on:input=move |ev| {
                    let value = event_target_value(&ev);
                    annotations.update(|list| {
                        if let Some(a) = list.iter_mut().find(|a| a.id == id) {
                            a.comment = value.clone();
                        }
                    });
                }
                style="flex:1 1 auto;min-width:0;resize:vertical;font-size:13px;padding:4px 6px;border-radius:4px;\
                       border:1px solid var(--colorNeutralStroke2);background:var(--colorNeutralBackground1);\
                       color:var(--colorNeutralForeground1);font-family:inherit;"
            >
                {initial}
            </textarea>
            <button
                type="button"
                title="Удалить фигуру"
                on:click=move |_| annotations.update(|list| list.retain(|a| a.id != id))
                style="flex:0 0 auto;border:0;background:none;cursor:pointer;padding:0 4px;line-height:1;\
                       font-size:18px;color:var(--colorNeutralForeground3);"
            >
                "×"
            </button>
        </div>
    }
}

/// Отрисовать все фигуры с номерами (общий код для превью и экспорта).
fn draw_annotations(ctx: &CanvasRenderingContext2d, annotations: &[Annotation], nw: f64, nh: f64) {
    for (index, a) in annotations.iter().enumerate() {
        draw_annotation(
            ctx,
            a.kind,
            a.ax,
            a.ay,
            a.bx,
            a.by,
            a.color,
            nw,
            nh,
            Some(index + 1),
        );
    }
}

/// Отрисовать одну фигуру. `number` = None для наброска (без номера).
#[allow(clippy::too_many_arguments)]
fn draw_annotation(
    ctx: &CanvasRenderingContext2d,
    kind: Shape,
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
    color: PenColor,
    nw: f64,
    nh: f64,
    number: Option<usize>,
) {
    ctx.set_stroke_style_str(color.stroke());
    match kind {
        Shape::Rect => {
            let x = ax.min(bx) * nw;
            let y = ay.min(by) * nh;
            let w = (bx - ax).abs() * nw;
            let h = (by - ay).abs() * nh;
            ctx.set_line_width(LINE_W);
            ctx.set_fill_style_str(color.fill());
            ctx.fill_rect(x, y, w, h);
            ctx.stroke_rect(x, y, w, h);
            if let Some(n) = number {
                // Квадрат с номером в левом верхнем углу.
                draw_number_square(ctx, n, color, x, y);
            }
        }
        Shape::Arrow => {
            let x1 = ax * nw;
            let y1 = ay * nh;
            let x2 = bx * nw;
            let y2 = by * nh;
            ctx.set_line_width(ARROW_LINE_W);
            ctx.begin_path();
            ctx.move_to(x1, y1);
            ctx.line_to(x2, y2);
            ctx.stroke();
            // Остриё-треугольник в конце вектора.
            let angle = (y2 - y1).atan2(x2 - x1);
            let spread = 0.5;
            let lx = x2 - ARROW_HEAD * (angle - spread).cos();
            let ly = y2 - ARROW_HEAD * (angle - spread).sin();
            let rx = x2 - ARROW_HEAD * (angle + spread).cos();
            let ry = y2 - ARROW_HEAD * (angle + spread).sin();
            ctx.set_fill_style_str(color.stroke());
            ctx.begin_path();
            ctx.move_to(x2, y2);
            ctx.line_to(lx, ly);
            ctx.line_to(rx, ry);
            ctx.close_path();
            ctx.fill();
            if let Some(n) = number {
                // Круг с номером в начале вектора стрелки.
                draw_number_circle(ctx, n, color, x1, y1);
            }
        }
    }
}

/// Цветной квадрат с белым номером; (x, y) — левый верхний угол.
fn draw_number_square(
    ctx: &CanvasRenderingContext2d,
    number: usize,
    color: PenColor,
    x: f64,
    y: f64,
) {
    let side = BADGE_R * 2.0;
    ctx.set_fill_style_str(color.stroke());
    ctx.fill_rect(x, y, side, side);
    draw_badge_number(ctx, number, x + side / 2.0, y + side / 2.0);
}

/// Цветной круг с белым номером; (cx, cy) — центр.
fn draw_number_circle(
    ctx: &CanvasRenderingContext2d,
    number: usize,
    color: PenColor,
    cx: f64,
    cy: f64,
) {
    ctx.begin_path();
    let _ = ctx.arc(cx, cy, BADGE_R, 0.0, TAU);
    ctx.set_fill_style_str(color.stroke());
    ctx.fill();
    draw_badge_number(ctx, number, cx, cy);
}

/// Белая цифра по центру (cx, cy) — общая для квадратного и круглого бейджа.
fn draw_badge_number(ctx: &CanvasRenderingContext2d, number: usize, cx: f64, cy: f64) {
    ctx.set_fill_style_str("#ffffff");
    ctx.set_font(&format!("bold {BADGE_FS}px sans-serif"));
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");
    let _ = ctx.fill_text(&number.to_string(), cx, cy);
}

/// Волнистая (синусоидальная) линия по всей ширине на уровне `y` — визуально
/// отделяет оригинальное изображение от панели комментариев.
fn draw_wavy_separator(ctx: &CanvasRenderingContext2d, width: f64, y: f64) {
    ctx.set_stroke_style_str("#94a3b8");
    ctx.set_line_width(2.0);
    ctx.begin_path();
    ctx.move_to(0.0, y);
    let amplitude = 4.0;
    let freq = TAU / 40.0; // длина волны ≈ 40px
    let mut x = 0.0;
    while x <= width {
        ctx.line_to(x, y + amplitude * (x * freq).sin());
        x += 2.0;
    }
    ctx.stroke();
}

/// Собрать полноразмерный PNG: изображение + пронумерованные фигуры + нижняя
/// панель с нумерованными комментариями (только непустыми), отделённая
/// волнистой линией — машиночитаемая копия правой колонки.
fn export_annotated_png(
    img: &HtmlImageElement,
    annotations: &[Annotation],
) -> Result<web_sys::File, String> {
    let nw = img.natural_width();
    let nh = img.natural_height();
    if nw == 0 || nh == 0 {
        return Err("Изображение ещё не готово".to_string());
    }
    let nwf = nw as f64;
    let nhf = nh as f64;

    let window = web_sys::window().ok_or_else(|| "Нет доступа к окну".to_string())?;
    let document = window
        .document()
        .ok_or_else(|| "Нет доступа к документу".to_string())?;
    let canvas: HtmlCanvasElement = document
        .create_element("canvas")
        .map_err(|_| "Не удалось создать канву".to_string())?
        .dyn_into()
        .map_err(|_| "Не удалось создать канву".to_string())?;
    canvas.set_width(nw);
    let ctx = canvas
        .get_context("2d")
        .map_err(|_| "Нет 2D-контекста".to_string())?
        .ok_or_else(|| "Нет 2D-контекста".to_string())?
        .dyn_into::<CanvasRenderingContext2d>()
        .map_err(|_| "Нет 2D-контекста".to_string())?;

    // --- Пасс 1: разметка панели. Только аннотации с непустым комментарием. ---
    let badge_col = PANEL_BADGE_COL;
    let text_max = (nwf - PANEL_MARGIN * 2.0 - badge_col).max(PANEL_FS * 4.0);
    ctx.set_font(&format!("{PANEL_FS}px sans-serif"));
    let mut rows: Vec<(usize, Shape, PenColor, Vec<String>)> = Vec::new();
    for (index, a) in annotations.iter().enumerate() {
        let comment = a.comment.trim();
        if comment.is_empty() {
            continue;
        }
        rows.push((
            index + 1,
            a.kind,
            a.color,
            wrap_text(&ctx, comment, text_max),
        ));
    }

    // Панель нужна только если есть что показать.
    let panel_h = if rows.is_empty() {
        0.0
    } else {
        let mut h = PANEL_SEP_H + PANEL_MARGIN + PANEL_LINE_H + 6.0; // разделитель + отступ + заголовок
        for (_, _, _, lines) in &rows {
            h += lines.len() as f64 * PANEL_LINE_H + 8.0;
        }
        h + PANEL_MARGIN
    };

    // --- Пасс 2: отрисовка. set_height сбрасывает состояние ctx. ---
    let total_h = nhf + panel_h;
    canvas.set_height(total_h as u32);

    ctx.set_fill_style_str("#ffffff");
    ctx.fill_rect(0.0, 0.0, nwf, total_h);
    let _ = ctx.draw_image_with_html_image_element(img, 0.0, 0.0);
    draw_annotations(&ctx, annotations, nwf, nhf);

    if !rows.is_empty() {
        draw_wavy_separator(&ctx, nwf, nhf + PANEL_SEP_H * 0.5);

        ctx.set_text_align("left");
        ctx.set_text_baseline("top");
        let mut y = nhf + PANEL_SEP_H + PANEL_MARGIN;
        ctx.set_fill_style_str("#0f172a");
        ctx.set_font(&format!("bold {PANEL_FS}px sans-serif"));
        let _ = ctx.fill_text("Комментарии", PANEL_MARGIN, y);
        y += PANEL_LINE_H + 6.0;

        for (number, kind, color, lines) in rows {
            let mid = y + PANEL_LINE_H * 0.5;
            match kind {
                Shape::Rect => draw_number_square(&ctx, number, color, PANEL_MARGIN, mid - BADGE_R),
                Shape::Arrow => {
                    draw_number_circle(&ctx, number, color, PANEL_MARGIN + BADGE_R, mid)
                }
            }
            ctx.set_fill_style_str("#111827");
            ctx.set_font(&format!("{PANEL_FS}px sans-serif"));
            ctx.set_text_align("left");
            ctx.set_text_baseline("top");
            for line in lines {
                let _ = ctx.fill_text(&line, PANEL_MARGIN + badge_col, y);
                y += PANEL_LINE_H;
            }
            y += 8.0;
        }
    }

    canvas_to_png_file(&canvas)
}

/// Перенос текста по словам под заданную максимальную ширину (по текущему
/// шрифту контекста). Пустые строки исходного текста сохраняются.
fn wrap_text(ctx: &CanvasRenderingContext2d, text: &str, max_w: f64) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let words: Vec<&str> = paragraph.split_whitespace().collect();
        if words.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in words {
            let trial = if current.is_empty() {
                word.to_string()
            } else {
                format!("{current} {word}")
            };
            let width = ctx.measure_text(&trial).map(|m| m.width()).unwrap_or(0.0);
            if width > max_w && !current.is_empty() {
                lines.push(std::mem::take(&mut current));
                current = word.to_string();
            } else {
                current = trial;
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }
    lines
}

/// Экспорт канвы в PNG-`File`.
fn canvas_to_png_file(canvas: &HtmlCanvasElement) -> Result<web_sys::File, String> {
    let data_url = canvas
        .to_data_url_with_type("image/png")
        .map_err(|_| "Не удалось отрендерить изображение".to_string())?;
    let comma = data_url
        .find(',')
        .ok_or_else(|| "Некорректные данные изображения".to_string())?;
    let window = web_sys::window().ok_or_else(|| "Нет доступа к окну".to_string())?;
    let binary = window
        .atob(&data_url[comma + 1..])
        .map_err(|_| "Не удалось декодировать изображение".to_string())?;
    // atob возвращает строку, где каждый символ — байт 0..255.
    let bytes: Vec<u8> = binary.chars().map(|c| c as u8).collect();
    let array = js_sys::Uint8Array::from(bytes.as_slice());
    let parts = js_sys::Array::of1(&array);
    let options = web_sys::FilePropertyBag::new();
    options.set_type("image/png");
    web_sys::File::new_with_u8_array_sequence_and_options(&parts, "screenshot.png", &options)
        .map_err(|_| "Не удалось собрать файл изображения".to_string())
}
