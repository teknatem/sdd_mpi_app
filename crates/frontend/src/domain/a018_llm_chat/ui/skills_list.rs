//! LLM skills catalog and editable specialization access matrix.

use crate::shared::api_utils::api_base;
use crate::shared::page_frame::PageFrame;
use crate::shared::page_standard::PAGE_CAT_LIST;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Deserialize)]
struct SkillInfo {
    id: String,
    title: String,
    description: String,
    #[serde(default)]
    intents: Vec<String>,
    #[serde(default)]
    tools: Vec<String>,
    #[serde(default)]
    origin: String,
    #[serde(default)]
    digest: String,
    #[serde(default)]
    diagnostic_status: String,
    #[serde(default)]
    resources: Vec<SkillResourceInfo>,
    #[serde(default)]
    tasks: Vec<SkillTaskInfo>,
}

#[derive(Clone, Deserialize)]
struct SkillResourceInfo {
    path: String,
    #[serde(default)]
    kind: String,
}

#[derive(Clone, Deserialize)]
struct SkillTaskInfo {
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    runtime: String,
    #[serde(default)]
    mode: String,
    #[serde(default)]
    input_schema: Option<serde_json::Value>,
}

#[derive(Clone, Deserialize)]
struct Catalog {
    #[serde(default)]
    core_tools: Vec<String>,
    #[serde(default)]
    skills: Vec<SkillInfo>,
    #[serde(default)]
    generation: u64,
    #[serde(default)]
    catalog_digest: String,
    #[serde(default)]
    loaded_at: String,
    #[serde(default)]
    diagnostics: Vec<String>,
}

#[derive(Clone, Deserialize)]
struct SpecializationInfo {
    id: String,
    title: String,
}

#[derive(Clone, Deserialize, Serialize)]
struct MatrixCell {
    specialization: String,
    skill_id: String,
    level: String,
    #[serde(default)]
    source: String,
}

#[derive(Clone, Deserialize, Serialize)]
struct CapabilityCell {
    specialization: String,
    capability: String,
    is_allowed: bool,
    #[serde(default)]
    source: String,
}

#[derive(Clone, Deserialize)]
struct AccessMatrix {
    revision: String,
    #[serde(default)]
    generation: u64,
    #[serde(default)]
    catalog_digest: String,
    specializations: Vec<SpecializationInfo>,
    skills: Vec<SkillInfo>,
    cells: Vec<MatrixCell>,
    capabilities: Vec<CapabilityCell>,
    #[serde(default)]
    capability_specs: Vec<CapabilitySpec>,
    #[serde(default)]
    diagnostics: Vec<String>,
}

#[derive(Clone, Deserialize)]
struct ReloadResult {
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    status: String,
    #[serde(default)]
    generation: u64,
    #[serde(default)]
    catalog_digest: String,
    #[serde(default)]
    added: Vec<String>,
    #[serde(default)]
    changed: Vec<String>,
    #[serde(default)]
    removed: Vec<String>,
    #[serde(default)]
    diagnostics: Vec<String>,
}

#[derive(Clone, Deserialize)]
struct CapabilitySpec {
    id: String,
    title: String,
    #[serde(default)]
    description: String,
}

fn cell_key(specialization: &str, skill_id: &str) -> String {
    format!("{specialization}\u{1f}{skill_id}")
}

fn capability_key(specialization: &str, capability: &str) -> String {
    format!("{specialization}\u{1f}{capability}")
}

fn next_level(level: &str) -> &'static str {
    match level {
        "immediate" => "extended",
        "extended" => "denied",
        _ => "immediate",
    }
}

/// Короткая подпись пилюли уровня (в матрице колонки узкие).
fn level_label(level: &str) -> &'static str {
    match level {
        "immediate" => "Осн",
        "extended" => "Доп",
        _ => "Нет",
    }
}

/// CSS class for the level pill/toggle — standard `.badge .badge--*`.
/// immediate=success, extended=warning, denied=neutral.
fn level_class(level: &str) -> &'static str {
    match level {
        "immediate" => "badge badge--success",
        "extended" => "badge badge--warning",
        _ => "badge badge--neutral",
    }
}

#[component]
#[allow(non_snake_case)]
pub fn LlmSkillList() -> impl IntoView {
    let catalog = RwSignal::new(None::<Catalog>);
    let matrix = RwSignal::new(None::<AccessMatrix>);
    let error = RwSignal::new(None::<String>);
    let active_tab = RwSignal::new("matrix".to_string());
    let filter = RwSignal::new(String::new());
    let pending = RwSignal::new(HashMap::<String, String>::new());
    let capability_pending = RwSignal::new(HashMap::<String, bool>::new());
    let saving = RwSignal::new(false);
    let reloading = RwSignal::new(false);
    let reload_result = RwSignal::new(None::<ReloadResult>);

    Effect::new(move |_| {
        wasm_bindgen_futures::spawn_local(async move {
            match fetch_catalog().await {
                Ok(value) => catalog.set(Some(value)),
                Err(e) => error.set(Some(e)),
            }
            match fetch_access_matrix().await {
                Ok(value) => matrix.set(Some(value)),
                Err(e) => error.set(Some(e)),
            }
        });
    });

    let save = move |_| {
        let Some(snapshot) = matrix.get_untracked() else {
            return;
        };
        let changes = pending.get_untracked();
        let cap_changes = capability_pending.get_untracked();
        let cells: Vec<_> = snapshot
            .cells
            .iter()
            .map(|cell| MatrixCell {
                level: changes
                    .get(&cell_key(&cell.specialization, &cell.skill_id))
                    .cloned()
                    .unwrap_or_else(|| cell.level.clone()),
                source: cell.source.clone(),
                ..cell.clone()
            })
            .collect();
        let capabilities: Vec<_> = snapshot
            .capabilities
            .iter()
            .map(|cell| CapabilityCell {
                is_allowed: cap_changes
                    .get(&capability_key(&cell.specialization, &cell.capability))
                    .copied()
                    .unwrap_or(cell.is_allowed),
                source: cell.source.clone(),
                ..cell.clone()
            })
            .collect();
        saving.set(true);
        wasm_bindgen_futures::spawn_local(async move {
            match put_access_matrix(&snapshot.revision, cells, capabilities).await {
                Ok(updated) => {
                    matrix.set(Some(updated));
                    pending.set(HashMap::new());
                    capability_pending.set(HashMap::new());
                    error.set(None);
                }
                Err(e) => error.set(Some(e)),
            }
            saving.set(false);
        });
    };

    let reload_catalog = move |_| {
        reloading.set(true);
        wasm_bindgen_futures::spawn_local(async move {
            match post_reload().await {
                Ok(result) if result.ok && result.status == "activated" => {
                    reload_result.set(Some(result));
                    match fetch_catalog().await {
                        Ok(value) => catalog.set(Some(value)),
                        Err(e) => error.set(Some(e)),
                    }
                    match fetch_access_matrix().await {
                        Ok(value) => {
                            matrix.set(Some(value));
                            pending.set(HashMap::new());
                            capability_pending.set(HashMap::new());
                            error.set(None);
                        }
                        Err(e) => error.set(Some(e)),
                    }
                }
                Ok(result) => {
                    let message = if result.diagnostics.is_empty() {
                        "Новый каталог отклонён; продолжает действовать предыдущая версия."
                            .to_string()
                    } else {
                        format!("Новый каталог отклонён: {}", result.diagnostics.join("; "))
                    };
                    reload_result.set(Some(result));
                    error.set(Some(message));
                }
                Err(e) => error.set(Some(e)),
            }
            reloading.set(false);
        });
    };

    view! {
        <PageFrame page_id="llm_skills--list" category=PAGE_CAT_LIST class="llm-skills-list">
            <div class="page__header">
                <div class="page__header-left">
                    <h1 class="page__title">"Навыки LLM"</h1>
                    <span class="page__header-meta">"Политика навыков виртуальных сотрудников"</span>
                </div>
                <div class="page__header-actions" style="display:flex;gap:8px;">
                    <button
                        class="btn btn--primary"
                        disabled=move || reloading.get()
                        on:click=reload_catalog
                    >
                        {move || if reloading.get() { "Перезагрузка…" } else { "Перезагрузить skills" }}
                    </button>
                    <button class="btn btn--secondary" on:click=move |_| active_tab.set("catalog".into())>
                        "Каталог"
                    </button>
                    <button class="btn btn--secondary" on:click=move |_| active_tab.set("matrix".into())>
                        "Матрица доступа"
                    </button>
                </div>
            </div>

            <div class="page__content">
                {move || error.get().map(|e| view! {
                    <div class="warning-box" style="background:var(--color-error-50);border-color:var(--color-error-100);margin-bottom:12px;">
                        <span class="warning-box__text" style="color:var(--color-error);">{e}</span>
                    </div>
                })}
                {move || reload_result.get().map(|result| {
                    let digest = result.catalog_digest.chars().take(12).collect::<String>();
                    view! {
                        <div class="warning-box" style="margin-bottom:12px;">
                            <span class="warning-box__text">
                                {format!(
                                    "Reload: {} · generation {} · digest {} · добавлено: {} · изменено: {} · удалено: {}",
                                    result.status,
                                    result.generation,
                                    digest,
                                    result.added.join(", "),
                                    result.changed.join(", "),
                                    result.removed.join(", "),
                                )}
                            </span>
                        </div>
                    }
                })}

                <div style=move || if active_tab.get() == "catalog" { "" } else { "display:none;" }>
                    {move || catalog.get().map(render_catalog)}
                </div>

                <div style=move || if active_tab.get() == "matrix" { "" } else { "display:none;" }>
                    <div class="skills-matrix__toolbar">
                        <input
                            class="form__input skills-matrix__search"
                            placeholder="Фильтр навыков"
                            prop:value=move || filter.get()
                            on:input=move |ev| filter.set(event_target_value(&ev))
                        />
                        <span class="badge badge--neutral">
                            {move || format!("Изменений: {}", pending.get().len() + capability_pending.get().len())}
                        </span>
                        <button
                            class="btn btn--primary"
                            disabled=move || saving.get() || (pending.get().is_empty() && capability_pending.get().is_empty())
                            on:click=save
                        >
                            {move || if saving.get() { "Сохранение…" } else { "Сохранить" }}
                        </button>
                    </div>
                    <div class="skills-matrix__legend">
                        <span class="badge badge--success">"Осн"</span><span>"— сразу"</span>
                        <span class="badge badge--warning">"Доп"</span><span>"— расширенный (неэффективность)"</span>
                        <span class="badge badge--neutral">"Нет"</span><span>"— нет доступа"</span>
                    </div>
                    {move || matrix.get().map(|snapshot| {
                        render_matrix(snapshot, filter.get(), pending, capability_pending)
                    })}
                </div>
            </div>
        </PageFrame>
    }
}

fn render_catalog(catalog: Catalog) -> impl IntoView {
    let core = catalog.core_tools.join(", ");
    let catalog_meta = format!(
        "Generation {} · digest {} · loaded {}",
        catalog.generation,
        catalog.catalog_digest.chars().take(12).collect::<String>(),
        catalog.loaded_at
    );
    let catalog_diagnostics = catalog.diagnostics.clone();
    let cards = catalog
        .skills
        .into_iter()
        .map(|skill| {
            let package_meta = format!(
                "{} · digest: {} · status: {} · resources: {} · tasks: {}",
                if skill.origin.is_empty() {
                    "legacy"
                } else {
                    &skill.origin
                },
                skill.digest.chars().take(12).collect::<String>(),
                if skill.diagnostic_status.is_empty() {
                    "ok"
                } else {
                    &skill.diagnostic_status
                },
                skill.resources.len(),
                skill.tasks.len()
            );
            let resources = skill
                .resources
                .iter()
                .map(|resource| format!("{} ({})", resource.path, resource.kind))
                .collect::<Vec<_>>()
                .join(", ");
            let tasks = skill
                .tasks
                .iter()
                .map(|task| {
                    format!(
                        "{} — {} [{}; {}; schema: {}]",
                        task.id,
                        task.title,
                        task.runtime,
                        task.mode,
                        if task.input_schema.is_some() {
                            "yes"
                        } else {
                            "no"
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let intents = skill
                .intents
                .into_iter()
                .map(|i| badge(i, "primary"))
                .collect_view();
            let tools = skill.tools.join(", ");
            view! {
                <div class="llm-skills__card">
                    <div class="llm-skills__card-head">
                        <strong class="llm-skills__title">{skill.title}</strong>
                        <code class="llm-skills__id">{skill.id}</code>
                    </div>
                    <div class="llm-skills__desc">{skill.description}</div>
                    <div class="llm-skills__meta">
                        <span class="llm-skills__label">"Package:"</span>
                        <span class="llm-skills__tools">{package_meta}</span>
                    </div>
                    <div class="llm-skills__meta">
                        <span class="llm-skills__label">"Интенты:"</span>
                        <span class="llm-skills__badges">{intents}</span>
                    </div>
                    <div class="llm-skills__meta">
                        <span class="llm-skills__label">"Инструменты:"</span>
                        <span class="llm-skills__tools">{tools}</span>
                    </div>
                    {(!resources.is_empty()).then(|| view! {
                        <div class="llm-skills__meta">
                            <span class="llm-skills__label">"Resources:"</span>
                            <span class="llm-skills__tools">{resources}</span>
                        </div>
                    })}
                    {(!tasks.is_empty()).then(|| view! {
                        <div class="llm-skills__meta">
                            <span class="llm-skills__label">"Tasks:"</span>
                            <span class="llm-skills__tools">{tasks}</span>
                        </div>
                    })}
                </div>
            }
        })
        .collect_view();
    view! {
        <div class="llm-skills__core">
            <strong>{catalog_meta}</strong>
        </div>
        {(!catalog_diagnostics.is_empty()).then(|| view! {
            <div class="warning-box" style="margin-bottom:10px;">
                {catalog_diagnostics.join("; ")}
            </div>
        })}
        <div class="llm-skills__core">
            <strong>"Core (всегда активен): "</strong>
            <span class="llm-skills__tools">{core}</span>
        </div>
        {cards}
    }
}

fn render_matrix(
    snapshot: AccessMatrix,
    filter: String,
    pending: RwSignal<HashMap<String, String>>,
    capability_pending: RwSignal<HashMap<String, bool>>,
) -> impl IntoView {
    let generation = snapshot.generation;
    let digest = snapshot.catalog_digest.chars().take(12).collect::<String>();
    let filter = filter.to_lowercase();
    let specializations = snapshot.specializations.clone();
    let cells = snapshot.cells.clone();
    let capabilities = snapshot.capabilities.clone();
    let capability_specs = snapshot.capability_specs.clone();
    let diagnostics = snapshot.diagnostics.clone();
    let visible_skills: Vec<_> = snapshot
        .skills
        .into_iter()
        .filter(|skill| {
            filter.is_empty()
                || skill.title.to_lowercase().contains(&filter)
                || skill.id.to_lowercase().contains(&filter)
        })
        .collect();

    view! {
        <div class="skills-matrix__meta">
            {format!("Каталог: generation {generation}, digest {digest}")}
        </div>
        {(!diagnostics.is_empty()).then(|| {
            let count = diagnostics.len();
            view! {
                // Свёрнуто по умолчанию; каждое сообщение — отдельной строкой.
                <details class="skills-matrix__diag warning-box">
                    <summary class="skills-matrix__diag-summary">
                        {format!("Диагностика каталога ({count})")}
                    </summary>
                    <ul class="skills-matrix__diag-list">
                        {diagnostics.iter().map(|d| view! { <li>{d.clone()}</li> }).collect_view()}
                    </ul>
                </details>
            }
        })}
        <div class="data-matrix-wrapper">
            <table class="data-matrix skills-matrix">
                <thead>
                    <tr>
                        <th class="data-matrix__corner skills-matrix__skill">"Skill"</th>
                        <th class="data-matrix__sticky-head skills-matrix__bulk-head">"Вся строка"</th>
                        {specializations.iter().map(|spec| view! {
                            <th class="data-matrix__sticky-head skills-matrix__spec-col">
                                <div>{spec.title.clone()}</div>
                                <code class="skills-matrix__spec-id">{spec.id.clone()}</code>
                            </th>
                        }).collect_view()}
                    </tr>
                </thead>
                <tbody>
                    <tr class="skills-matrix__cap-row">
                        <td class="data-matrix__rowhead skills-matrix__cap-name">
                            "Функциональное право: публикация артефактов"
                        </td>
                        <td></td>
                        {specializations.iter().map(|spec| {
                            let spec_id = spec.id.clone();
                            let key = capability_key(&spec_id, "artifact_publish");
                            let base = capabilities.iter()
                                .find(|c| c.specialization == spec_id && c.capability == "artifact_publish")
                                .map(|c| c.is_allowed)
                                .unwrap_or(false);
                            view! {
                                <td class="skills-matrix__center">
                                    <input
                                        type="checkbox"
                                        prop:checked=move || capability_pending.get().get(&key).copied().unwrap_or(base)
                                        on:change={
                                            let key = capability_key(&spec.id, "artifact_publish");
                                            move |ev| {
                                                capability_pending.update(|map| {
                                                    map.insert(key.clone(), event_target_checked(&ev));
                                                });
                                            }
                                        }
                                    />
                                </td>
                            }
                        }).collect_view()}
                    </tr>
                    {capability_specs.into_iter()
                        .filter(|capability| capability.id != "artifact_publish")
                        .map(|capability| {
                            let cap_id = capability.id.clone();
                            let cap_title = capability.title.clone();
                            let cap_description = capability.description.clone();
                            let row_specs = specializations.clone();
                            let row_capabilities = capabilities.clone();
                            view! {
                                <tr class="skills-matrix__cap-row">
                                    <td
                                        title=cap_description
                                        class="data-matrix__rowhead skills-matrix__cap-name"
                                    >
                                        {cap_title}
                                        <div><code class="skills-matrix__cap-id">{cap_id.clone()}</code></div>
                                    </td>
                                    <td></td>
                                    {row_specs.into_iter().map(|spec| {
                                        let key = capability_key(&spec.id, &cap_id);
                                        let base = row_capabilities.iter()
                                            .find(|cell| cell.specialization == spec.id && cell.capability == cap_id)
                                            .map(|cell| cell.is_allowed)
                                            .unwrap_or(false);
                                        let key_checked = key.clone();
                                        view! {
                                            <td class="skills-matrix__center">
                                                <input
                                                    type="checkbox"
                                                    prop:checked=move || capability_pending.get().get(&key_checked).copied().unwrap_or(base)
                                                    on:change=move |ev| {
                                                        capability_pending.update(|map| {
                                                            map.insert(key.clone(), event_target_checked(&ev));
                                                        });
                                                    }
                                                />
                                            </td>
                                        }
                                    }).collect_view()}
                                </tr>
                            }
                        })
                        .collect_view()}
                    {visible_skills.into_iter().map(|skill| {
                        let skill_id = skill.id.clone();
                        let row_specs = specializations.clone();
                        let row_cells = cells.clone();
                        let skill_id_bulk = skill_id.clone();
                        view! {
                            <tr>
                                <td title=skill.description.clone() class="data-matrix__rowhead skills-matrix__skill">
                                    <strong>{skill.title}</strong>
                                    <div><code class="skills-matrix__skill-id">{skill_id.clone()}</code></div>
                                </td>
                                <td class="skills-matrix__bulk">
                                    <select class="form__select skills-matrix__bulk-select" on:change={
                                        let specs = specializations.clone();
                                        move |ev| {
                                            let value = event_target_value(&ev);
                                            if value.is_empty() { return; }
                                            pending.update(|map| {
                                                for spec in &specs {
                                                    map.insert(cell_key(&spec.id, &skill_id_bulk), value.clone());
                                                }
                                            });
                                        }
                                    }>
                                        <option value="">"Установить…"</option>
                                        <option value="immediate">"Сразу"</option>
                                        <option value="extended">"Расширенный"</option>
                                        <option value="denied">"Нет доступа"</option>
                                    </select>
                                </td>
                                {row_specs.into_iter().map(|spec| {
                                    let key = cell_key(&spec.id, &skill_id);
                                    let base = row_cells.iter()
                                        .find(|cell| cell.specialization == spec.id && cell.skill_id == skill_id)
                                        .map(|cell| cell.level.clone())
                                        .unwrap_or_else(|| "denied".to_string());
                                    let key_click = key.clone();
                                    let key_style = key.clone();
                                    let key_label = key.clone();
                                    let base_style = base.clone();
                                    let base_label = base.clone();
                                    view! {
                                        <td class="skills-matrix__center">
                                            <button
                                                class=move || {
                                                    let level = pending.get().get(&key_style).cloned().unwrap_or_else(|| base_style.clone());
                                                    level_class(&level)
                                                }
                                                on:click=move |_| {
                                                    let current = pending.get_untracked().get(&key_click).cloned().unwrap_or_else(|| base.clone());
                                                    pending.update(|map| { map.insert(key_click.clone(), next_level(&current).to_string()); });
                                                }
                                            >
                                                {move || {
                                                    let level = pending.get().get(&key_label).cloned().unwrap_or_else(|| base_label.clone());
                                                    level_label(&level)
                                                }}
                                            </button>
                                        </td>
                                    }
                                }).collect_view()}
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
            </table>
        </div>
    }
}

fn badge(text: String, variant: &'static str) -> impl IntoView {
    view! { <span class=format!("badge badge--{variant}")>{text}</span> }
}

async fn fetch_catalog() -> Result<Catalog, String> {
    get_json(&format!("{}/api/llm-skills", api_base())).await
}

async fn fetch_access_matrix() -> Result<AccessMatrix, String> {
    get_json(&format!("{}/api/llm-skills/access-matrix", api_base())).await
}

async fn post_reload() -> Result<ReloadResult, String> {
    use wasm_bindgen::JsCast;
    use web_sys::{Request, RequestInit, RequestMode, Response};
    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(RequestMode::Cors);
    let request =
        Request::new_with_str_and_init(&format!("{}/api/llm-skills/reload", api_base()), &opts)
            .map_err(|e| format!("{e:?}"))?;
    let response: Response = wasm_bindgen_futures::JsFuture::from(
        web_sys::window()
            .ok_or("no window")?
            .fetch_with_request(&request),
    )
    .await
    .map_err(|e| format!("{e:?}"))?
    .dyn_into()
    .map_err(|e| format!("{e:?}"))?;
    let text = wasm_bindgen_futures::JsFuture::from(response.text().map_err(|e| format!("{e:?}"))?)
        .await
        .map_err(|e| format!("{e:?}"))?
        .as_string()
        .ok_or("bad text")?;
    if !response.ok() {
        return Err(format!("HTTP {}: {}", response.status(), text));
    }
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

async fn get_json<T: serde::de::DeserializeOwned>(url: &str) -> Result<T, String> {
    use wasm_bindgen::JsCast;
    use web_sys::{Request, RequestInit, RequestMode, Response};
    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);
    let request = Request::new_with_str_and_init(url, &opts).map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("{e:?}"))?;
    let response: Response = wasm_bindgen_futures::JsFuture::from(
        web_sys::window()
            .ok_or("no window")?
            .fetch_with_request(&request),
    )
    .await
    .map_err(|e| format!("{e:?}"))?
    .dyn_into()
    .map_err(|e| format!("{e:?}"))?;
    let text = wasm_bindgen_futures::JsFuture::from(response.text().map_err(|e| format!("{e:?}"))?)
        .await
        .map_err(|e| format!("{e:?}"))?
        .as_string()
        .ok_or("bad text")?;
    if !response.ok() {
        return Err(format!("HTTP {}: {}", response.status(), text));
    }
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

async fn put_access_matrix(
    revision: &str,
    cells: Vec<MatrixCell>,
    capabilities: Vec<CapabilityCell>,
) -> Result<AccessMatrix, String> {
    use wasm_bindgen::JsCast;
    use web_sys::{Request, RequestInit, RequestMode, Response};
    let opts = RequestInit::new();
    opts.set_method("PUT");
    opts.set_mode(RequestMode::Cors);
    let payload = serde_json::json!({
        "revision": revision,
        "cells": cells,
        "capabilities": capabilities,
    });
    opts.set_body(&wasm_bindgen::JsValue::from_str(&payload.to_string()));
    let url = format!("{}/api/llm-skills/access-matrix", api_base());
    let request = Request::new_with_str_and_init(&url, &opts).map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|e| format!("{e:?}"))?;
    let response: Response = wasm_bindgen_futures::JsFuture::from(
        web_sys::window()
            .ok_or("no window")?
            .fetch_with_request(&request),
    )
    .await
    .map_err(|e| format!("{e:?}"))?
    .dyn_into()
    .map_err(|e| format!("{e:?}"))?;
    let text = wasm_bindgen_futures::JsFuture::from(response.text().map_err(|e| format!("{e:?}"))?)
        .await
        .map_err(|e| format!("{e:?}"))?
        .as_string()
        .ok_or("bad text")?;
    if !response.ok() {
        return Err(format!("HTTP {}: {}", response.status(), text));
    }
    serde_json::from_str(&text).map_err(|e| e.to_string())
}
