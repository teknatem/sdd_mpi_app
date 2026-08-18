//! LLM Artifact Details - View Component

use super::api::{fetch_artifact, update_artifact, LlmArtifactDto};
use super::view_model::LlmArtifactDetailsVm;
use crate::shared::icons::icon;
use crate::shared::page_frame::PageFrame;
use contracts::domain::common::AggregateId;
use leptos::prelude::*;
use thaw::*;

#[component]
#[allow(non_snake_case)]
pub fn LlmArtifactDetails(id: String, on_close: Callback<()>) -> impl IntoView {
    let vm = LlmArtifactDetailsVm::new();
    let artifact_id = id.clone();
    let active_tab = RwSignal::new("general");

    Effect::new({
        let artifact_id = artifact_id.clone();
        move |_| {
            let artifact_id = artifact_id.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match fetch_artifact(&artifact_id).await {
                    Ok(artifact) => {
                        vm.artifact.set(Some(artifact));
                        vm.error.set(None);
                    }
                    Err(e) => {
                        vm.error.set(Some(e));
                    }
                }
            });
        }
    });

    let handle_save = move |_| {
        let Some(artifact) = vm.artifact.get() else {
            return;
        };
        vm.is_saving.set(true);
        let dto = LlmArtifactDto {
            id: Some(artifact.base.id.as_string()),
            code: Some(artifact.base.code.clone()),
            description: artifact.base.description.clone(),
            comment: artifact.base.comment.clone(),
            chat_id: artifact.chat_id.as_string(),
            agent_id: artifact.agent_id.as_string(),
            sql_query: artifact.sql_query.clone(),
            query_params: artifact.query_params.clone(),
            visualization_config: artifact.visualization_config.clone(),
        };
        wasm_bindgen_futures::spawn_local(async move {
            match update_artifact(dto).await {
                Ok(_) => {
                    vm.is_saving.set(false);
                    vm.is_editing.set(false);
                    vm.error.set(None);
                }
                Err(e) => {
                    vm.is_saving.set(false);
                    vm.error.set(Some(format!("Ошибка сохранения: {}", e)));
                }
            }
        });
    };

    view! {
        <PageFrame page_id="a019_llm_artifact_details" category="detail">
            // Header
            <div class="page__header">
                <div class="page__header-left">
                    <h1 class="page__title">
                        {move || {
                            vm.artifact
                                .get()
                                .map(|a| a.base.description.clone())
                                .unwrap_or_else(|| "Загрузка...".to_string())
                        }}
                    </h1>
                    {move || {
                        vm.artifact.get().map(|a| {
                            let (label, color) = match a.status.as_str() {
                                "active"     => ("Активен",  BadgeColor::Success),
                                "deprecated" => ("Устарел",  BadgeColor::Warning),
                                "failed"     => ("Ошибка",   BadgeColor::Danger),
                                _            => ("Черновик", BadgeColor::Informative),
                            };
                            view! {
                                <Badge appearance=BadgeAppearance::Filled color=color>
                                    {label}
                                </Badge>
                            }
                        })
                    }}
                </div>
                <div class="page__header-right">
                    <Button
                        appearance=ButtonAppearance::Primary
                        disabled=Signal::derive(move || vm.is_saving.get() || !vm.is_editing.get())
                        on_click=handle_save
                    >
                        {icon("save")}
                        {move || if vm.is_saving.get() { " Сохранение..." } else { " Сохранить" }}
                    </Button>
                    <Button appearance=ButtonAppearance::Secondary on_click=move |_| on_close.run(())>
                        "Закрыть"
                    </Button>
                </div>
            </div>

            // Tabs
            <div class="page__tabs">
                <button
                    class="page__tab"
                    class:page__tab--active=move || active_tab.get() == "general"
                    on:click=move |_| active_tab.set("general")
                >
                    {icon("file-text")} " Общее"
                </button>
                <button
                    class="page__tab"
                    class:page__tab--active=move || active_tab.get() == "sql"
                    on:click=move |_| active_tab.set("sql")
                >
                    {icon("code")} " SQL"
                </button>
                <button
                    class="page__tab"
                    class:page__tab--active=move || active_tab.get() == "viz"
                    on:click=move |_| active_tab.set("viz")
                >
                    {icon("bar-chart")} " Визуализация"
                </button>
                <button
                    class="page__tab"
                    class:page__tab--active=move || active_tab.get() == "meta"
                    on:click=move |_| active_tab.set("meta")
                >
                    {icon("info")} " Метаданные"
                </button>
            </div>

            // Content
            <div class="page__content">
                {move || {
                    if let Some(err) = vm.error.get() {
                        return view! {
                            <div style="padding: var(--spacing-lg); background: var(--color-error-50); border: 1px solid var(--color-error-100); border-radius: var(--radius-sm); color: var(--color-error);">
                                <strong>"Ошибка: "</strong>{err}
                            </div>
                        }.into_any();
                    }
                    match active_tab.get() {
                        "general" => render_general_tab(vm).into_any(),
                        "sql"     => render_sql_tab(vm).into_any(),
                        "viz"     => render_viz_tab(vm).into_any(),
                        "meta"    => render_meta_tab(vm).into_any(),
                        _         => render_general_tab(vm).into_any(),
                    }
                }}
            </div>
        </PageFrame>
    }
}

fn render_general_tab(vm: LlmArtifactDetailsVm) -> impl IntoView {
    view! {
        {move || {
            vm.artifact.get().map(|artifact| {
                let code = RwSignal::new(artifact.base.code.clone());
                let description = RwSignal::new(artifact.base.description.clone());
                let comment = RwSignal::new(artifact.base.comment.clone().unwrap_or_default());
                let status = artifact.status.as_str().to_string();

                view! {
                    <div style="display: flex; flex-direction: column; gap: 16px; max-width: 800px;">
                        <div>
                            <label style="display: block; font-weight: 600; margin-bottom: 8px;">"Код"</label>
                            <Input value=code disabled=true />
                        </div>

                        <div>
                            <label style="display: block; font-weight: 600; margin-bottom: 8px;">"Название"</label>
                            <Input value=description disabled=Signal::derive(move || !vm.is_editing.get()) />
                        </div>

                        <div>
                            <label style="display: block; font-weight: 600; margin-bottom: 8px;">"Комментарий"</label>
                            <Textarea value=comment attr:style="width: 100%; min-height: 100px;" />
                        </div>

                        <div>
                            <label style="display: block; font-weight: 600; margin-bottom: 8px;">"Статус"</label>
                            <div style="padding: 8px 12px; background: var(--colorNeutralBackground3); border-radius: 4px;">
                                {status}
                            </div>
                        </div>

                        <Button
                            appearance=ButtonAppearance::Secondary
                            on_click=move |_| vm.is_editing.set(!vm.is_editing.get())
                        >
                            {move || if vm.is_editing.get() { "Отменить редактирование" } else { "Редактировать" }}
                        </Button>
                    </div>
                }
            })
        }}
    }
}

fn render_sql_tab(vm: LlmArtifactDetailsVm) -> impl IntoView {
    view! {
        {move || {
            vm.artifact.get().map(|artifact| {
                let sql_query = RwSignal::new(artifact.sql_query.clone());
                let query_params = RwSignal::new(artifact.query_params.clone().unwrap_or_default());

                view! {
                    <div>
                        <label style="display: block; font-weight: 600; margin-bottom: 8px;">"SQL Запрос"</label>
                        <Textarea value=sql_query attr:style="width: 100%; min-height: 400px; font-family: monospace;" />

                        <label style="display: block; font-weight: 600; margin-top: 16px; margin-bottom: 8px;">"Параметры запроса (JSON)"</label>
                        <Textarea value=query_params attr:style="width: 100%; min-height: 100px; font-family: monospace;" />
                    </div>
                }
            })
        }}
    }
}

fn render_viz_tab(vm: LlmArtifactDetailsVm) -> impl IntoView {
    view! {
        {move || {
            vm.artifact.get().map(|artifact| {
                let viz_config = RwSignal::new(artifact.visualization_config.clone().unwrap_or_default());

                view! {
                    <div>
                        <label style="display: block; font-weight: 600; margin-bottom: 8px;">"Конфигурация визуализации (JSON)"</label>
                        <Textarea value=viz_config attr:style="width: 100%; min-height: 400px; font-family: monospace;" />
                    </div>
                }
            })
        }}
    }
}

fn render_meta_tab(vm: LlmArtifactDetailsVm) -> impl IntoView {
    view! {
        {move || {
            vm.artifact.get().map(|artifact| {
                let chat_id = artifact.chat_id.as_string();
                let agent_id = artifact.agent_id.as_string();
                let artifact_type = artifact.artifact_type.as_str().to_string();
                let execution_count = artifact.execution_count.to_string();
                let created_at = artifact.base.metadata.created_at.format("%Y-%m-%d %H:%M:%S").to_string();
                let updated_at = artifact.base.metadata.updated_at.format("%Y-%m-%d %H:%M:%S").to_string();

                view! {
                    <div style="display: flex; flex-direction: column; gap: 16px; max-width: 800px;">
                        <div>
                            <label style="display: block; font-weight: 600; margin-bottom: 8px;">"ID Чата"</label>
                            <div style="padding: 8px 12px; background: var(--colorNeutralBackground3); border-radius: 4px; font-family: monospace;">
                                {chat_id}
                            </div>
                        </div>

                        <div>
                            <label style="display: block; font-weight: 600; margin-bottom: 8px;">"ID Агента"</label>
                            <div style="padding: 8px 12px; background: var(--colorNeutralBackground3); border-radius: 4px; font-family: monospace;">
                                {agent_id}
                            </div>
                        </div>

                        <div>
                            <label style="display: block; font-weight: 600; margin-bottom: 8px;">"Тип артефакта"</label>
                            <div style="padding: 8px 12px; background: var(--colorNeutralBackground3); border-radius: 4px;">
                                {artifact_type}
                            </div>
                        </div>

                        <div>
                            <label style="display: block; font-weight: 600; margin-bottom: 8px;">"Количество выполнений"</label>
                            <div style="padding: 8px 12px; background: var(--colorNeutralBackground3); border-radius: 4px;">
                                {execution_count}
                            </div>
                        </div>

                        <div>
                            <label style="display: block; font-weight: 600; margin-bottom: 8px;">"Создан"</label>
                            <div style="padding: 8px 12px; background: var(--colorNeutralBackground3); border-radius: 4px;">
                                {created_at}
                            </div>
                        </div>

                        <div>
                            <label style="display: block; font-weight: 600; margin-bottom: 8px;">"Обновлён"</label>
                            <div style="padding: 8px 12px; background: var(--colorNeutralBackground3); border-radius: 4px;">
                                {updated_at}
                            </div>
                        </div>
                    </div>
                }
            })
        }}
    }
}
