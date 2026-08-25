//! Вкладка «Сводка» — §6 в шести блоках.
//!
//! Блоки идут по убыванию срочности, а не по порядку в документе: первым —
//! то, что сломано (недостижимость), потом объём, потом здоровье и ценность.
//! Список важнее числа: «3 недостижимые поверхности» — повод пожать плечами,
//! их имена — задача.

use leptos::prelude::*;

use crate::knowledge::view_model::InventoryVm;

#[component]
pub fn SummaryTab(vm: InventoryVm) -> impl IntoView {
    view! {
        {move || {
            let Some(data) = vm.data.get() else { return ().into_any() };
            let s = data.summary.clone();
            let snapshot = data.snapshot.clone();
            let previous = data.previous.clone();

            // Дельта строится только между снимками одной версии классификатора:
            // разреза, которого в прошлой версии не было, задним числом не
            // существует, и разность по нему была бы выдумкой.
            let delta = previous.as_ref().and_then(|prev| {
                (prev.classifier_version == snapshot.classifier_version)
                    .then(|| snapshot.unit_count as i64 - prev.unit_count as i64)
            });

            view! {
                <div class="knowledge-inventory__meta">
                    <span>"Снимок: " {snapshot.captured_at.clone()}</span>
                    <span>"Повод: " {snapshot.trigger.clone()}</span>
                    <span>"Классификатор v" {snapshot.classifier_version}</span>
                    <span>"Сбор: " {snapshot.collect_ms} " мс"</span>
                </div>

                <section class="knowledge-inventory__block">
                    <h2 class="knowledge-inventory__block-title">"Достижимость"</h2>
                    <p class="knowledge-inventory__hint">
                        "«Есть» и «доступно» — разные числа. Разница между ними и есть \
                         главный результат инвентаризации."
                    </p>
                    <NamedList
                        label="Поверхности, до которых не дотягивается ни один инструмент"
                        items=s.unreachable_surfaces.clone()
                        empty="Все поверхности отдаются хотя бы одним инструментом"
                        bad=true
                    />
                    <NamedList
                        label="Инструменты навыков, которых нет в каталоге (молча выбрасываются)"
                        items=s.phantom_tools.clone()
                        empty="Все имена инструментов резолвятся"
                        bad=true
                    />
                    <NamedList
                        label="Инструменты вне ядра и вне навыков (мёртвый код)"
                        items=s.orphan_tools.clone()
                        empty="Мёртвых инструментов нет"
                        bad=false
                    />
                    <NamedList
                        label="Заявленная достижимость разошлась с фактической"
                        items=s.reachability_mismatches.clone()
                        empty="Реестр согласован с маппингом инструментов"
                        bad=false
                    />
                </section>

                <section class="knowledge-inventory__block">
                    <h2 class="knowledge-inventory__block-title">"Объём"</h2>
                    <div class="knowledge-inventory__tiles">
                        <Tile label="Единиц всего" value=snapshot.unit_count.to_string()
                              note=delta.map(|d| format!("{d:+} к прошлому снимку")).unwrap_or_default() />
                        <Tile label="Хранимых" value=s.stored_units.to_string() note=String::new() />
                        <Tile label="Вычисляемых" value=s.computed_units.to_string() note=String::new() />
                        <Tile label="Токенов хранимого" value=s.stored_tokens.to_string()
                              note="у вычисляемых цены нет — складывать нельзя".into() />
                        <Tile label="Статей о предметной области" value=s.articles_business.to_string() note=String::new() />
                        <Tile label="Техдоков приложения" value=s.articles_app.to_string() note=String::new() />
                        <Tile label="Карт из БД" value=s.articles_generated.to_string() note=String::new() />
                    </div>
                </section>

                <section class="knowledge-inventory__block">
                    <h2 class="knowledge-inventory__block-title">"Здоровье и ценность"</h2>
                    <div class="knowledge-inventory__tiles">
                        <Tile label="Висячих ссылок related" value=s.dangling_links.to_string() note=String::new() />
                        <Tile label="Якорей вне реестра" value=s.unknown_anchors.to_string() note=String::new() />
                        <Tile label="Тегов вне словаря" value=s.unknown_tags.to_string() note=String::new() />
                        <Tile label="Просроченных статей" value=s.stale_articles.to_string() note=String::new() />
                        <Tile label="Черновиков" value=s.drafts.to_string() note=String::new() />
                        <Tile label="Осиротевшей статистики" value=s.orphaned_metrics.to_string()
                              note="след переименования файла".into() />
                        <Tile label="Ни разу не тронуто" value=s.never_touched.to_string()
                              note="ни поиска, ни чтения, ни цитирования".into() />
                        <Tile label="Найдено, но не прочитано" value=s.searched_not_read.to_string()
                              note="признак плохого summary".into() />
                        <Tile label="Прочитано, но не процитировано" value=s.read_not_cited.to_string()
                              note="признак плохой статьи".into() />
                    </div>
                </section>
            }.into_any()
        }}
    }
}

/// Плитка «число + подпись». Число крупное, пояснение мелкое и необязательное.
#[component]
fn Tile(label: &'static str, value: String, note: String) -> impl IntoView {
    let has_note = !note.is_empty();
    view! {
        <div class="knowledge-inventory__tile">
            <div class="knowledge-inventory__tile-value">{value}</div>
            <div class="knowledge-inventory__tile-label">{label}</div>
            {has_note.then(|| view! {
                <div class="knowledge-inventory__tile-note">{note}</div>
            })}
        </div>
    }
}

/// Поимённый список — то, что нельзя заменить числом.
///
/// Пустой список показывается тоже: «нарушений нет» — такой же результат
/// проверки, как и список нарушений, и молчание вместо него читается как сбой.
#[component]
fn NamedList(
    label: &'static str,
    items: Vec<String>,
    empty: &'static str,
    bad: bool,
) -> impl IntoView {
    let count = items.len();
    let count_class = if count > 0 && bad {
        "knowledge-inventory__count knowledge-inventory__count--bad"
    } else {
        "knowledge-inventory__count"
    };
    let body = if count == 0 {
        view! { <div class="knowledge-inventory__named-empty">{empty}</div> }.into_any()
    } else {
        view! {
            <ul class="knowledge-inventory__named-list">
                {items.into_iter().map(|item| view! { <li>{item}</li> }).collect_view()}
            </ul>
        }
        .into_any()
    };

    view! {
        <div class="knowledge-inventory__named">
            <div class="knowledge-inventory__named-head">
                <span class="knowledge-inventory__named-label">{label}</span>
                <span class=count_class>{count}</span>
            </div>
            {body}
        </div>
    }
}
