//! Страница «Метрики проекта» — снимок состояния экземпляра и кодовой базы.
//!
//! Отвечает не на вопрос «сколько сейчас», а «куда движется»: рядом с каждым
//! значением стоит дельта к предыдущему запуску и спарклайн по последним
//! снимкам.
//!
//! Раскладка подчинена этому же. Полсотни плиток равного веса — стена, в
//! которой не видно ничего; поэтому сверху идёт блок «Требует внимания» с теми
//! метриками, что перешли порог, и только под ним — полный разбор по группам.
//! Это форма «выделение»: одна вещь громкая, остальные тихие.
//!
//! Полсотни плиток — это ещё и объём, по которому надо уметь ходить, поэтому
//! к раскладке добавлены три механизма навигации: поиск по подстроке, липкие
//! сворачиваемые заголовки групп и кнопка передачи всего снимка в AI-чат.
//!
//! Ни подписей, ни порогов, ни порядка плиток страница не знает: всё приходит с
//! бэкенда из `METRIC_CATALOG`. Добавить метрику — правка каталога, здесь не
//! нужно менять ничего.

use std::collections::{HashMap, HashSet};

use contracts::system::metrics::{
    DetailTable, MetricSnapshotDto, MetricStatus, MetricValueDto, ProjectMetricsDto,
};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::domain::a018_llm_chat::ui::launch::launch_chat_with_context;
use crate::layout::global_context::AppGlobalContext;
use crate::shared::components::meta_strip::{MetaItem, MetaSection, MetaStrip};
use crate::shared::components::page_header::PageHeader;
use crate::shared::components::search_box::SearchBox;
use crate::shared::components::table::number_format::format_number_with_decimals;
use crate::shared::components::viz::sparkline::MIN_POINTS;
use crate::shared::components::viz::{DeltaChip, Meter, Sparkline, StatTile};
use crate::shared::icons::icon;
use crate::shared::page_frame::PageFrame;
use crate::shared::page_standard::PAGE_CAT_DASHBOARD;

use super::api;

/// Сколько снимков тянем на спарклайн.
const SERIES_LIMIT: u64 = 60;

/// Сколько последних точек показывает обычная плитка.
///
/// Весь ряд на полоске шириной ~240 px даёт отсчёт каждые 4 px — точки
/// сливаются в пунктир и перестают быть отсчётами. Крупный график получает
/// ряд целиком: там места хватает.
const TILE_POINTS: usize = 24;

/// Ключ, под которым состояние свёрнутых групп переживает уход с вкладки.
const COLLAPSED_STATE_KEY: &str = "sys_metrics_collapsed_groups";

/// Вопрос, с которого начинается разбор метрик в чате.
const AI_PROMPT: &str = "Разбери метрики проекта: что сейчас требует внимания, что ухудшилось \
                         с прошлого снимка и за что взяться в первую очередь. Коротко, по \
                         пунктам, с приоритетом.";

/// Ряды спарклайнов: значения и отметки времени снимков, ключ — код метрики.
/// Время нужно ховеру: число без привязки к моменту ни о чём не говорит.
type Series = HashMap<String, (Vec<f64>, Vec<String>)>;

fn format_delta(delta: f64, precision: u8) -> String {
    format_number_with_decimals(delta.abs(), precision)
}

/// Короткий вид отметки времени: «15.08 08:23» из RFC 3339 без разбора в дату.
fn short_stamp(value: &str) -> String {
    let Some((date, rest)) = value.split_once('T') else {
        return value.to_string();
    };
    let time: String = rest.chars().take(5).collect();
    let parts: Vec<&str> = date.split('-').collect();
    match parts.as_slice() {
        [_, month, day] => format!("{day}.{month} {time}"),
        _ => format!("{date} {time}"),
    }
}

/// Насколько метрика «громкая»: сначала перешедшие порог, потом близкие к нему.
fn severity(status: MetricStatus) -> u8 {
    match status {
        MetricStatus::Bad => 0,
        MetricStatus::Warn => 1,
        _ => 2,
    }
}

/// Какому блоку принадлежит таблица-детализация. Префикс кода почти совпадает с
/// кодом группы — расходится только качество данных.
fn detail_group(code: &str) -> &str {
    match code.split('.').next().unwrap_or("") {
        "quality" => "data_quality",
        other => other,
    }
}

/// Подходит ли метрика под поисковый запрос.
///
/// Ищем ровно по тому, что видно на экране, плюс по ключу: `code.lines.total`
/// набирается быстрее русской подписи, а на этой странице ключи у пользователя
/// перед глазами — они же в режиме «Цифрами».
fn metric_matches(metric: &MetricValueDto, group_label: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let haystack = format!(
        "{} {} {} {} {}",
        metric.label,
        metric.key,
        metric.unit,
        metric.hint.clone().unwrap_or_default(),
        group_label,
    )
    .to_lowercase();
    haystack.contains(needle)
}

/// Плитка метрики со всеми слотами: дельта, шкала порогов, тренд.
fn tile(metric: &MetricValueDto, series: &Series) -> AnyView {
    let precision = metric.precision;
    let format_value =
        Callback::new(move |(value,): (f64,)| format_number_with_decimals(value, precision));

    let delta = metric.delta.map(|delta| {
        view! {
            <DeltaChip
                delta=delta
                direction=metric.direction
                formatted=format_delta(delta, precision)
                since="с прошлого старта"
            />
        }
        .into_any()
    });

    let (mut points, mut labels) = series.get(&metric.key).cloned().unwrap_or_default();
    if points.len() > TILE_POINTS {
        points = points.split_off(points.len() - TILE_POINTS);
        if labels.len() > TILE_POINTS {
            labels = labels.split_off(labels.len() - TILE_POINTS);
        }
    }
    let trend = (points.len() >= MIN_POINTS).then(|| {
        view! { <Sparkline points=points labels=labels format_value=format_value /> }.into_any()
    });

    // Шкала — только там, где каталог задал пороги: без них ей не от чего
    // отмерять, а рисовать полосу «просто так» значит врать про масштаб.
    let meter = match (metric.warn, metric.bad) {
        (Some(warn), Some(bad)) => Some(
            view! {
                <Meter
                    value=metric.value
                    warn=warn
                    bad=bad
                    direction=metric.direction
                    status=metric.status
                    format_value=format_value
                />
            }
            .into_any(),
        ),
        _ => None,
    };

    view! {
        <StatTile
            label=metric.label.clone()
            value=format_number_with_decimals(metric.value, precision)
            unit=metric.unit.clone()
            status=metric.status
            hint=metric.hint.clone().unwrap_or_default()
            delta=delta
            meter=meter
            trend=trend
        />
    }
    .into_any()
}

/// Таблица-детализация. Числовая колонка несёт бар: её задача — сравнить
/// величины, а колонка цифр для этого работает медленнее полосок.
#[component]
fn DetailTableView(table: DetailTable) -> impl IntoView {
    // Проценты показываем с дробной частью, счётчики — без: одна колонка на
    // таблицу, поэтому решение принимается по её заголовку.
    let decimals: u8 = if table.value_label.contains('%') {
        2
    } else {
        0
    };

    // Бар меряется от максимума таблицы, а не от суммы: сравниваются строки
    // между собой, доля целого здесь ничего не значит.
    let max = table
        .rows
        .iter()
        .map(|row| row.value)
        .fold(0.0_f64, f64::max);

    view! {
        <div class="sys-metrics__table-block">
            <div class="sys-metrics__table-title">{table.label.clone()}</div>
            <div class="table-wrapper">
                <table class="table__data sys-metrics__table">
                    <thead>
                        <tr>
                            <th>"Объект"</th>
                            <th>{table.value_label.clone()}</th>
                            <th>""</th>
                        </tr>
                    </thead>
                    <tbody>
                        {table
                            .rows
                            .into_iter()
                            .map(|row| {
                                let share = if max > 0.0 { row.value / max } else { 0.0 };
                                view! {
                                    <tr>
                                        <td>{row.name}</td>
                                        <td class="table__cell--bar">
                                            {format_number_with_decimals(row.value, decimals)}
                                            <span
                                                class="table__cell-bar"
                                                style:width=format!("{:.1}%", share * 100.0)
                                            ></span>
                                        </td>
                                        <td class="sys-metrics__cell-muted">
                                            {row.extra.unwrap_or_default()}
                                        </td>
                                    </tr>
                                }
                            })
                            .collect_view()}
                    </tbody>
                </table>
            </div>
        </div>
    }
}

/// Табличный двойник страницы: все метрики числами, без графики.
///
/// Не украшение и не «режим для бедных»: значения рядов иначе доступны только
/// через наведение, а это делает их непрочитываемыми с клавиатуры и
/// непригодными для копирования в отчёт.
#[component]
fn NumbersView(values: Vec<MetricValueDto>) -> impl IntoView {
    view! {
        <div class="sys-metrics__table-block">
            <div class="table-wrapper">
                <table class="table__data">
                    <thead>
                        <tr>
                            <th>"Метрика"</th>
                            <th>"Ключ"</th>
                            <th>"Значение"</th>
                            <th>"Было"</th>
                            <th>"Дельта"</th>
                            <th>"Оценка"</th>
                        </tr>
                    </thead>
                    <tbody>
                        {values
                            .into_iter()
                            .map(|metric| {
                                let precision = metric.precision;
                                let dash = || "—".to_string();
                                let previous = metric
                                    .previous
                                    .map(|value| format_number_with_decimals(value, precision))
                                    .unwrap_or_else(dash);
                                let delta = metric
                                    .delta
                                    .map(|value| {
                                        let sign = if value > 0.0 { "+" } else { "" };
                                        format!(
                                            "{sign}{}",
                                            format_number_with_decimals(value, precision),
                                        )
                                    })
                                    .unwrap_or_else(dash);
                                let verdict = match metric.status {
                                    MetricStatus::Ok => "норма",
                                    MetricStatus::Warn => "внимание",
                                    MetricStatus::Bad => "порог",
                                    MetricStatus::Neutral => "—",
                                };
                                view! {
                                    <tr>
                                        <td>{metric.label}</td>
                                        <td class="sys-metrics__cell-muted">{metric.key}</td>
                                        <td class="table__cell--right sys-metrics__cell-number">
                                            {format_number_with_decimals(metric.value, precision)}
                                            " " {metric.unit}
                                        </td>
                                        <td class="table__cell--right sys-metrics__cell-number">
                                            {previous}
                                        </td>
                                        <td class="table__cell--right sys-metrics__cell-number">
                                            {delta}
                                        </td>
                                        <td>{verdict}</td>
                                    </tr>
                                }
                            })
                            .collect_view()}
                    </tbody>
                </table>
            </div>
        </div>
    }
}

/// Паспорт снимка: чем собрано и когда.
///
/// Восемь пар подряд не читаются — глазу не за что зацепиться, чтобы отделить
/// «чем собрано» от «когда собрано». Секции возвращают эту границу.
#[component]
fn Passport(snapshot: MetricSnapshotDto, previous: Option<String>) -> impl IntoView {
    let commit = snapshot
        .git_commit
        .clone()
        .unwrap_or_else(|| "—".to_string());
    let code_stamp = snapshot
        .code_generated_at
        .clone()
        .map(|value| short_stamp(&value))
        .unwrap_or_else(|| "—".to_string());
    let previous_stamp = previous
        .map(|value| short_stamp(&value))
        .unwrap_or_else(|| "нет".to_string());

    view! {
        <MetaStrip>
            <MetaSection label="Сборка">
                <MetaItem label="Версия" value=snapshot.app_version.clone() />
                <MetaItem label="Коммит" value=commit mono=true />
                <MetaItem label="Профиль" value=snapshot.build_profile.clone() />
            </MetaSection>
            <MetaSection label="Данные">
                <MetaItem label="Схема БД" value=snapshot.schema_version.to_string() />
                <MetaItem label="Метрики кода от" value=code_stamp />
            </MetaSection>
            <MetaSection label="Снимок">
                <MetaItem label="Текущий" value=short_stamp(&snapshot.captured_at) />
                <MetaItem label="Предыдущий" value=previous_stamp />
                <MetaItem label="Сбор" value=format!("{} мс", snapshot.collect_ms) />
            </MetaSection>
        </MetaStrip>
    }
}

/// Ведущий блок: только то, что перешло порог или подошло к нему.
#[component]
fn AttentionBlock(metrics: Vec<MetricValueDto>, series: Series) -> impl IntoView {
    if metrics.is_empty() {
        return view! {
            <section class="sys-metrics__attention sys-metrics__attention--clear">
                <h2 class="sys-metrics__block-title">"Требует внимания"</h2>
                <div class="sys-metrics__state">
                    "Ни одна метрика не перешла порог. Дальше — полный разбор по группам."
                </div>
            </section>
        }
        .into_any();
    }

    let count = metrics.len();
    view! {
        <section class="sys-metrics__attention">
            <div class="sys-metrics__attention-head">
                <h2 class="sys-metrics__block-title">"Требует внимания"</h2>
                <span class="sys-metrics__attention-count">
                    {format!("{count} из всех показателей за порогом")}
                </span>
            </div>
            // Ровная сетка: порядок здесь уже несёт приоритет (тяжёлые сверху),
            // а укрупнять первую плитку оказалось нечем — тренд на 28 px не
            // становится содержательнее оттого, что вокруг него больше места.
            // Разбор одной метрики — отдельный экран, а не растянутая плитка.
            <div class="sys-metrics__bento">
                {metrics
                    .iter()
                    .map(|metric| {
                        view! {
                            <div class="sys-metrics__bento-item">{tile(metric, &series)}</div>
                        }
                    })
                    .collect_view()}
            </div>
        </section>
    }
    .into_any()
}

/// Сводка статусов группы для свёрнутого состояния.
///
/// Свёрнутая группа обязана сообщать, что внутри что-то не так, — иначе
/// сворачивание превращается в способ спрятать проблему.
fn group_summary(metrics: &[MetricValueDto]) -> (usize, usize) {
    let bad = metrics
        .iter()
        .filter(|m| matches!(m.status, MetricStatus::Bad))
        .count();
    let warn = metrics
        .iter()
        .filter(|m| matches!(m.status, MetricStatus::Warn))
        .count();
    (bad, warn)
}

#[component]
pub fn ProjectMetricsPage() -> impl IntoView {
    let ctx = use_context::<AppGlobalContext>().expect("AppGlobalContext not found");

    let data = RwSignal::new(None::<ProjectMetricsDto>);
    let series = RwSignal::new(Series::new());
    let loading = RwSignal::new(false);
    let collecting = RwSignal::new(false);
    let numbers_mode = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let query = RwSignal::new(String::new());

    // Храним именно свёрнутые группы: пустое состояние значит «всё раскрыто»,
    // то есть свежая страница показывает всё, а не прячет всё.
    let collapsed = RwSignal::new(
        ctx.get_form_state(COLLAPSED_STATE_KEY)
            .and_then(|value| serde_json::from_value::<HashSet<String>>(value).ok())
            .unwrap_or_default(),
    );

    // Состояние переживает уход с вкладки: свернув девять групп из десяти,
    // пользователь настроил себе рабочий вид и не должен собирать его заново.
    Effect::new(move |_| {
        let snapshot = collapsed.get();
        if let Ok(value) = serde_json::to_value(&snapshot) {
            ctx.set_form_state(COLLAPSED_STATE_KEY.to_string(), value);
        }
    });

    let load = move || {
        loading.set(true);
        error.set(None);
        spawn_local(async move {
            match api::get_latest().await {
                Ok(response) => {
                    let keys: Vec<String> = response
                        .values
                        .iter()
                        .map(|item| item.key.clone())
                        .collect();
                    data.set(Some(response));
                    loading.set(false);

                    // Ряды тянем вторым запросом: страница уже полезна без
                    // спарклайнов, а на первом старте их всё равно нет.
                    if let Ok(rows) = api::get_series(&keys, SERIES_LIMIT).await {
                        series.set(
                            rows.into_iter()
                                .map(|row| {
                                    let (values, stamps) = row
                                        .points
                                        .into_iter()
                                        .map(|point| (point.value, short_stamp(&point.captured_at)))
                                        .unzip();
                                    (row.key, (values, stamps))
                                })
                                .collect(),
                        );
                    }
                }
                Err(message) => {
                    error.set(Some(message));
                    loading.set(false);
                }
            }
        });
    };

    let collect = move || {
        collecting.set(true);
        spawn_local(async move {
            let result = api::collect_now().await;
            collecting.set(false);
            match result {
                Ok(_) => load(),
                Err(message) => error.set(Some(message)),
            }
        });
    };

    // Счётчик считается отдельно от блока плиток: он реактивен по `query`, но
    // сидит в статическом тулбаре, поэтому перерисовывается только сам.
    let visible_count = move || -> String {
        let Some(response) = data.get() else {
            return String::new();
        };
        let needle = query.get().trim().to_lowercase();
        let shown = response
            .values
            .iter()
            .filter(|item| {
                let label = response
                    .groups
                    .iter()
                    .find(|group| group.code == item.group)
                    .map(|group| group.label.as_str())
                    .unwrap_or("");
                metric_matches(item, label, &needle)
            })
            .count();
        format!("Показано {shown} из {}", response.values.len())
    };

    let ask_ai = move || {
        launch_chat_with_context(
            ctx,
            "sys_metrics".to_string(),
            "Метрики проекта".to_string(),
            Some(AI_PROMPT.to_string()),
        );
    };

    Effect::new(move |_| {
        load();
    });

    view! {
        <PageFrame
            page_id="sys_metrics--dashboard"
            category=PAGE_CAT_DASHBOARD
            class="page--wide sys-metrics"
        >
            <PageHeader
                title="Метрики проекта"
                subtitle="Снимок при каждом запуске бэкенда; сканирований при открытии нет"
            >
                <button class="button button--primary" on:click=move |_| ask_ai()>
                    "Спросить AI"
                </button>
                <button
                    class="button button--ghost"
                    on:click=move |_| numbers_mode.update(|value| *value = !*value)
                >
                    {move || if numbers_mode.get() { "Показать плитки" } else { "Цифрами" }}
                </button>
                <button
                    class="button button--secondary"
                    on:click=move |_| load()
                    disabled=move || loading.get()
                >
                    "Обновить"
                </button>
                <button
                    class="button button--ghost"
                    on:click=move |_| collect()
                    disabled=move || collecting.get()
                >
                    {move || {
                        if collecting.get() { "Пересчёт…" } else { "Пересобрать снимок" }
                    }}
                </button>
            </PageHeader>

            <div class="page__content">
                <div class="sys-metrics__body">
                    <div class="sys-metrics__note">
                        "Метрики кода вшиты в сборку и обновляются pre-commit хуком; остальные "
                        "читаются из уже посчитанных источников — профиля данных, истории "
                        "quality-проверок и реестра роутов. Значение само по себе мало что "
                        "говорит: смотри на дельту к прошлому старту."
                    </div>

                    {move || {
                        data.get()
                            .and_then(|response| {
                                response
                                    .snapshot
                                    .map(|snapshot| {
                                        view! {
                                            <Passport
                                                snapshot=snapshot
                                                previous=response.previous_captured_at
                                            />
                                        }
                                    })
                            })
                    }}

                    {move || {
                        error
                            .get()
                            .map(|message| {
                                view! { <div class="alert alert--error">{message}</div> }
                            })
                    }}

                    // Тулбар живёт вне реактивного блока с плитками: тот блок
                    // читает `query`, и если бы поле поиска рисовалось внутри,
                    // оно пересоздавалось бы на каждый введённый символ и
                    // теряло фокус.
                    <Show when=move || {
                        data.get().map(|d| d.snapshot.is_some()).unwrap_or(false)
                            && !numbers_mode.get()
                    }>
                        <div class="sys-metrics__toolbar">
                            <SearchBox
                                query=query
                                placeholder="Поиск по названию, ключу и группе"
                            />
                            <button
                                class="button button--ghost"
                                on:click=move |_| collapsed.set(HashSet::new())
                            >
                                "Развернуть все"
                            </button>
                            <button
                                class="button button--ghost"
                                on:click=move |_| {
                                    collapsed
                                        .set(
                                            data
                                                .get()
                                                .map(|d| {
                                                    d.groups.iter().map(|g| g.code.clone()).collect()
                                                })
                                                .unwrap_or_default(),
                                        )
                                }
                            >
                                "Свернуть все"
                            </button>
                            <span class="sys-metrics__count">{move || visible_count()}</span>
                        </div>
                    </Show>

                    {move || {
                        if loading.get() && data.get().is_none() {
                            return view! { <div class="sys-metrics__state">"Загрузка…"</div> }
                                .into_any();
                        }
                        let Some(response) = data.get() else {
                            return view! { <div class="sys-metrics__state">"Нет данных."</div> }
                                .into_any();
                        };
                        if response.snapshot.is_none() {
                            return view! {
                                <div class="sys-metrics__state">
                                    "Первый снимок ещё собирается — он снимается примерно через "
                                    "полминуты после старта бэкенда. Обнови страницу или нажми "
                                    "«Пересобрать снимок»."
                                </div>
                            }
                                .into_any();
                        }

                        if numbers_mode.get() {
                            return view! { <NumbersView values=response.values.clone() /> }
                                .into_any();
                        }

                        let points = series.get();
                        let details = response.details.clone();
                        let needle = query.get().trim().to_lowercase();
                        let searching = !needle.is_empty();

                        // Подпись группы участвует в поиске, поэтому нужна рядом
                        // с каждой метрикой.
                        let group_label = |code: &str| -> String {
                            response
                                .groups
                                .iter()
                                .find(|g| g.code == code)
                                .map(|g| g.label.clone())
                                .unwrap_or_default()
                        };

                        let visible: Vec<MetricValueDto> = response
                            .values
                            .iter()
                            .filter(|item| {
                                metric_matches(item, &group_label(&item.group), &needle)
                            })
                            .cloned()
                            .collect();

                        // Порог перешли — наверх, по убыванию тяжести. Блок
                        // внимания подчиняется поиску наравне с остальными:
                        // иначе отфильтрованная страница противоречит счётчику.
                        let mut attention: Vec<MetricValueDto> = visible
                            .iter()
                            .filter(|item| {
                                matches!(item.status, MetricStatus::Bad | MetricStatus::Warn)
                            })
                            .cloned()
                            .collect();
                        attention.sort_by_key(|item| severity(item.status));

                        let groups = response
                            .groups
                            .iter()
                            .map(|group| {
                                let metrics: Vec<MetricValueDto> = visible
                                    .iter()
                                    .filter(|item| item.group == group.code)
                                    .cloned()
                                    .collect();
                                // Детализации привязаны к группе, а не к метрике,
                                // и при активном поиске только шумят.
                                let tables: Vec<DetailTable> = if searching {
                                    Vec::new()
                                } else {
                                    details
                                        .iter()
                                        .filter(|table| detail_group(&table.code) == group.code)
                                        .cloned()
                                        .collect()
                                };
                                if metrics.is_empty() && tables.is_empty() {
                                    return ().into_any();
                                }

                                let code = group.code.clone();
                                let code_for_toggle = code.clone();
                                // При активном поиске группы раскрыты
                                // принудительно: иначе найденное прячется за
                                // свёрнутым заголовком.
                                // `Signal`, а не замыкание: значение читают и
                                // шеврон, и `<Show>`, а замыкание не `Copy`.
                                let is_open = Signal::derive(move || {
                                    searching || !collapsed.get().contains(&code)
                                });
                                let count = metrics.len();
                                let (bad, warn) = group_summary(&metrics);
                                let points = points.clone();

                                view! {
                                    <section class="sys-metrics__group">
                                        <button
                                            class="sys-metrics__group-head"
                                            on:click=move |_| {
                                                collapsed
                                                    .update(|set| {
                                                        if !set.remove(&code_for_toggle) {
                                                            set.insert(code_for_toggle.clone());
                                                        }
                                                    })
                                            }
                                        >
                                            <span class=move || {
                                                // Строкой, а не `class:`: реестр
                                                // UI видит классы только в
                                                // строковых литералах.
                                                if is_open.get() {
                                                    "sys-metrics__chevron \
                                                     sys-metrics__chevron--expanded"
                                                } else {
                                                    "sys-metrics__chevron"
                                                }
                                            }>
                                                {icon("chevron-right")}
                                            </span>
                                            <span class="sys-metrics__group-title">
                                                {group.label.clone()}
                                            </span>
                                            <span class="sys-metrics__group-count">
                                                {format!("{count}")}
                                            </span>
                                            {(bad > 0)
                                                .then(|| {
                                                    view! {
                                                        <span class="badge badge--error">
                                                            {format!("{bad} за порогом")}
                                                        </span>
                                                    }
                                                })}
                                            {(warn > 0)
                                                .then(|| {
                                                    view! {
                                                        <span class="badge badge--warning">
                                                            {format!("{warn} внимание")}
                                                        </span>
                                                    }
                                                })}
                                        </button>
                                        <Show when=move || is_open.get()>
                                            <div class="sys-metrics__group-body">
                                                <div class="sys-metrics__tiles">
                                                    {metrics
                                                        .iter()
                                                        .map(|metric| tile(metric, &points))
                                                        .collect_view()}
                                                </div>
                                                {tables
                                                    .clone()
                                                    .into_iter()
                                                    .map(|table| {
                                                        view! { <DetailTableView table=table /> }
                                                    })
                                                    .collect_view()}
                                            </div>
                                        </Show>
                                    </section>
                                }
                                    .into_any()
                            })
                            .collect_view();

                        let empty = visible.is_empty();

                        view! {
                            {(!searching)
                                .then(|| {
                                    view! {
                                        <AttentionBlock
                                            metrics=attention.clone()
                                            series=points.clone()
                                        />
                                    }
                                })}

                            {empty
                                .then(|| {
                                    view! {
                                        <div class="sys-metrics__state">
                                            "Ни одна метрика не подошла под запрос."
                                        </div>
                                    }
                                })}

                            {groups}
                        }
                            .into_any()
                    }}
                </div>
            </div>
        </PageFrame>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use contracts::system::metrics::MetricDirection;

    fn metric(key: &str, label: &str, group: &str) -> MetricValueDto {
        MetricValueDto {
            key: key.to_string(),
            label: label.to_string(),
            group: group.to_string(),
            unit: "шт".to_string(),
            value: 1.0,
            precision: 0,
            direction: MetricDirection::Lower,
            status: MetricStatus::Neutral,
            warn: None,
            bad: None,
            previous: None,
            delta: None,
            hint: None,
        }
    }

    #[test]
    fn severity_puts_thresholds_first() {
        let mut order = vec![MetricStatus::Ok, MetricStatus::Warn, MetricStatus::Bad];
        order.sort_by_key(|status| severity(*status));
        assert_eq!(
            order,
            vec![MetricStatus::Bad, MetricStatus::Warn, MetricStatus::Ok]
        );
    }

    #[test]
    fn quality_details_land_in_the_data_quality_block() {
        assert_eq!(detail_group("quality.checks"), "data_quality");
        assert_eq!(detail_group("db.top_tables"), "db");
        assert_eq!(detail_group("code.top_files"), "code");
    }

    #[test]
    fn stamp_shortens_rfc3339() {
        assert_eq!(short_stamp("2026-08-15T05:41:35.896+00:00"), "15.08 05:41");
        // Не-RFC строку возвращаем как есть, а не роняем страницу.
        assert_eq!(short_stamp("непонятно"), "непонятно");
    }

    #[test]
    fn search_finds_metric_by_label_key_and_group() {
        // Подпись намеренно без литерального вызова: метрика `smells.unwrap`
        // считает вхождения текстом и посчитала бы собственную тестовую строку.
        let item = metric("code.unwrap.total", "Вызовы unwrap", "code");
        assert!(metric_matches(&item, "Размер кода", "unwrap"));
        assert!(metric_matches(&item, "Размер кода", "code."));
        assert!(metric_matches(&item, "Размер кода", "размер"));
        assert!(!metric_matches(&item, "Размер кода", "миграц"));
    }

    #[test]
    fn empty_query_keeps_everything() {
        let item = metric("db.wal_mb", "WAL", "db");
        assert!(metric_matches(&item, "База данных", ""));
    }

    #[test]
    fn collapsed_set_semantics_default_to_expanded() {
        // Пустое множество свёрнутых = всё раскрыто: свежая страница показывает
        // содержимое, а не прячет его.
        let collapsed: HashSet<String> = HashSet::new();
        assert!(!collapsed.contains("code"));
    }

    #[test]
    fn group_summary_counts_only_thresholds() {
        let mut bad_metric = metric("a", "A", "code");
        bad_metric.status = MetricStatus::Bad;
        let mut warn_metric = metric("b", "B", "code");
        warn_metric.status = MetricStatus::Warn;
        let ok_metric = metric("c", "C", "code");
        let (bad, warn) = group_summary(&[bad_metric, warn_metric, ok_metric]);
        assert_eq!((bad, warn), (1, 1));
    }
}
