//! AI-сотрудник (a017) — карточка.
//!
//! Персона (имя, аватар, почта, специализация, обязанности, расписание) поверх
//! технического подключения a038. Техника берётся из подключения, здесь не редактируется.

use super::api::{fetch_agent, fetch_connections, fetch_employee_skills, save_agent};
use super::view_model::{ConnOption, LlmAgentDetailsVm, SkillItem};
use crate::shared::icons::icon;
use leptos::prelude::*;
use thaw::*;

#[component]
#[allow(non_snake_case)]
pub fn LlmAgentDetails(
    id: Signal<Option<String>>,
    on_saved: Callback<()>,
    on_cancel: Callback<()>,
) -> impl IntoView {
    let vm = LlmAgentDetailsVm::new();

    // Загрузить список технических подключений (для селекта «Подключение»).
    Effect::new(move |_| {
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok(list) = fetch_connections().await {
                let opts = list
                    .into_iter()
                    .map(|c| ConnOption {
                        id: c.base.id.value().to_string(),
                        name: c.base.description.clone(),
                        allowed_models: c.allowed_models_list(),
                        default_model: c.model_name.clone(),
                    })
                    .collect::<Vec<_>>();
                vm.connections.set(opts);
            }
        });
    });

    // Загрузить сотрудника при редактировании.
    Effect::new(move |_| {
        if let Some(agent_id) = id.get() {
            wasm_bindgen_futures::spawn_local(async move {
                match fetch_agent(&agent_id).await {
                    Ok(agent) => {
                        vm.code.set(agent.base.code);
                        vm.description.set(agent.base.description);
                        vm.comment.set(agent.base.comment.unwrap_or_default());
                        vm.agent_type.set(agent.agent_type.as_str().to_string());
                        vm.system_prompt
                            .set(agent.system_prompt.unwrap_or_default());
                        vm.connection_id
                            .set(agent.connection_id.unwrap_or_default());
                        vm.model_name.set(agent.model_name);
                        vm.avatar.set(agent.avatar.unwrap_or_default());
                        vm.email.set(agent.email.unwrap_or_default());
                        vm.schedule_cron
                            .set(agent.schedule_cron.unwrap_or_default());
                        vm.is_active.set(agent.is_active);
                        vm.is_primary.set(agent.is_primary);
                    }
                    Err(e) => vm.set_error.set(Some(e)),
                }
            });
        }
    });

    // Перезагружать навыки при смене специализации.
    Effect::new(move |_| {
        let at = vm.agent_type.get();
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok(resp) = fetch_employee_skills(&at).await {
                let map = |v: Vec<super::api::SkillDto>| {
                    v.into_iter()
                        .map(|s| SkillItem {
                            id: s.id,
                            title: s.title,
                            description: s.description,
                        })
                        .collect::<Vec<_>>()
                };
                vm.skills_core.set(map(resp.core));
                vm.skills_extended.set(map(resp.extended));
            }
        });
    });

    // Сохранение.
    let handle_save = move |_| {
        let id_value = id.get();
        let dto = vm.build_save_dto(id_value);
        wasm_bindgen_futures::spawn_local(async move {
            match save_agent(dto).await {
                Ok(_) => on_saved.run(()),
                Err(e) => vm.set_error.set(Some(e)),
            }
        });
    };

    let is_edit_mode = Signal::derive(move || id.get().is_some());

    view! {
        <div class="details-form" style="padding: 20px;">
            <Flex justify=FlexJustify::SpaceBetween align=FlexAlign::Center style="margin-bottom: 20px;">
                <h2 style="font-size: 20px; font-weight: bold;">
                    {move || if is_edit_mode.get() { "Редактирование сотрудника" } else { "Новый AI-сотрудник" }}
                </h2>
                <Space>
                    <Button appearance=ButtonAppearance::Primary on_click=handle_save>
                        {icon("save")}
                        " Сохранить"
                    </Button>
                    <Button appearance=ButtonAppearance::Secondary on_click=move |_| on_cancel.run(())>
                        {icon("close")}
                        " Отмена"
                    </Button>
                </Space>
            </Flex>

            {move || {
                vm.error
                    .get()
                    .map(|e| {
                        view! {
                            <div style="padding: 12px; margin-bottom: 16px; background: var(--color-error-50); border: 1px solid var(--color-error-100); border-radius: 8px;">
                                <span style="color: var(--color-error);">{e}</span>
                            </div>
                        }
                    })
            }}

            <div style="display: grid; grid-template-columns: 500px 500px; gap: var(--spacing-md); max-width: 1050px; align-items: start; align-content: start;">
                <Card>
                    <div class="form__group">
                        <label class="form__label">"Аватар"</label>
                        <Input value=vm.avatar placeholder="🧑‍💼 или инициалы/URL" />
                    </div>

                    <div class="form__group">
                        <label class="form__label">
                            "Имя сотрудника"
                            <span style="color: red;">"*"</span>
                        </label>
                        <Input value=vm.description placeholder="Анна — аналитик продаж" />
                    </div>

                    <div class="form__group">
                        <label class="form__label">"Специализация"</label>
                        <select
                            style="height: 32px; padding: 4px 8px; border: 1px solid var(--colorNeutralStroke2); border-radius: 6px; width: 100%; background: var(--color-surface); color: var(--color-text);"
                            prop:value=move || vm.agent_type.get()
                            on:change=move |ev| {
                                vm.agent_type.set(event_target_value(&ev));
                            }
                        >
                            <option value="business_analyst">"Бизнес-аналитик"</option>
                            <option value="sales_analyst">"Аналитик продаж"</option>
                            <option value="marketer">"Маркетолог"</option>
                            <option value="financier">"Финансист"</option>
                            <option value="coordinator_admin">"Координатор-администратор"</option>
                            <option value="plugin_admin">"Разработчик"</option>
                            <option value="system_admin">"Системный администратор"</option>
                            <option value="kb_admin">"Администратор базы знаний"</option>
                            <option value="tester">"Тестировщик"</option>
                        </select>
                        <div style="font-size: 12px; color: var(--colorNeutralForeground3);">
                            "Определяет набор навыков и инструментов сотрудника."
                        </div>
                    </div>

                    <div class="form__group">
                        <label class="form__label">"Почта"</label>
                        <Input value=vm.email placeholder="anna@ai.local" />
                    </div>

                    <div class="form__group">
                        <label class="form__label">"Код"</label>
                        <Input value=vm.code placeholder="EMP-SALES-1" />
                    </div>

                    <div class="form__group">
                        <label class="form__label">"Комментарий"</label>
                        <Textarea value=vm.comment placeholder="Заметка о сотруднике" />
                    </div>

                    <div style="display: flex; align-items: center; gap: 16px; margin-top: 8px;">
                        <label style="display: flex; align-items: center; gap: 8px;">
                            <input
                                type="checkbox"
                                prop:checked=move || vm.is_active.get()
                                on:change=move |ev| vm.is_active.set(event_target_checked(&ev))
                            />
                            <span>"Активен"</span>
                        </label>
                        <label style="display: flex; align-items: center; gap: 8px;">
                            <input
                                type="checkbox"
                                prop:checked=move || vm.is_primary.get()
                                on:change=move |ev| vm.is_primary.set(event_target_checked(&ev))
                            />
                            <span>"Основной (по умолчанию в чате)"</span>
                        </label>
                    </div>
                </Card>

                <Card>
                    <div class="form__group">
                        <label class="form__label">
                            "Подключение"
                            <span style="color: red;">"*"</span>
                        </label>
                        <select
                            style="height: 32px; padding: 4px 8px; border: 1px solid var(--colorNeutralStroke2); border-radius: 6px; width: 100%; background: var(--color-surface); color: var(--color-text);"
                            prop:value=move || vm.connection_id.get()
                            on:change=move |ev| {
                                vm.connection_id.set(event_target_value(&ev));
                                // Сброс закреплённой модели — она относилась к прежнему подключению.
                                vm.model_name.set(String::new());
                            }
                        >
                            <option value="">"— выберите подключение —"</option>
                            <For
                                each=move || vm.connections.get()
                                key=|c| c.id.clone()
                                children=move |c| {
                                    view! { <option value=c.id.clone()>{c.name.clone()}</option> }
                                }
                            />
                        </select>
                        <div style="font-size: 12px; color: var(--colorNeutralForeground3);">
                            "Техническое подключение a038: провайдер, ключ, тюнинг."
                        </div>
                    </div>

                    <div class="form__group">
                        <label class="form__label">"Модель (закреплённая)"</label>
                        <select
                            style="height: 32px; padding: 4px 8px; border: 1px solid var(--colorNeutralStroke2); border-radius: 6px; width: 100%; background: var(--color-surface); color: var(--color-text);"
                            prop:value=move || vm.model_name.get()
                            on:change=move |ev| vm.model_name.set(event_target_value(&ev))
                        >
                            <option value="">"— дефолт подключения —"</option>
                            <For
                                each=move || vm.models_for_selected_connection()
                                key=|m| m.clone()
                                children=move |m| {
                                    view! { <option value=m.clone()>{m.clone()}</option> }
                                }
                            />
                        </select>
                        <div style="font-size: 12px; color: var(--colorNeutralForeground3);">
                            "Пусто → используется модель по умолчанию подключения."
                        </div>
                    </div>

                    <div class="form__group">
                        <label class="form__label">"Должностные обязанности"</label>
                        <Textarea
                            attr:style="min-height : 120px"
                            value=vm.system_prompt
                            placeholder="Ты — аналитик продаж. Отвечаешь за..."
                        />
                    </div>

                    <div class="form__group">
                        <label class="form__label">"Расписание (cron)"</label>
                        <Input value=vm.schedule_cron placeholder="0 */2 * * * * (задел)" />
                        <div style="font-size: 12px; color: var(--colorNeutralForeground3);">
                            "Задел: исполнитель пробуждения добавится отдельной фазой."
                        </div>
                    </div>
                </Card>
            </div>

            // Read-only блок навыков специализации.
            <Card>
                <div style="font-weight: 600; margin-bottom: 8px;">"Навыки специализации"</div>
                <div style="display: grid; grid-template-columns: 1fr 1fr; gap: var(--spacing-md);">
                    <div>
                        <div style="font-size: 13px; color: var(--colorNeutralForeground3); margin-bottom: 4px;">
                            "Основные (активны по умолчанию)"
                        </div>
                        <For
                            each=move || vm.skills_core.get()
                            key=|s| s.id.clone()
                            children=move |s| {
                                view! {
                                    <div style="padding: 6px 0; border-bottom: 1px solid var(--color-border-light);">
                                        <span style="font-weight: 500;">{s.title.clone()}</span>
                                        <div style="font-size: 12px; color: var(--colorNeutralForeground3);">
                                            {s.description.clone()}
                                        </div>
                                    </div>
                                }
                            }
                        />
                    </div>
                    <div>
                        <div style="font-size: 13px; color: var(--colorNeutralForeground3); margin-bottom: 4px;">
                            "Расширенные (по запросу — use_skill)"
                        </div>
                        <For
                            each=move || vm.skills_extended.get()
                            key=|s| s.id.clone()
                            children=move |s| {
                                view! {
                                    <div style="padding: 6px 0; border-bottom: 1px solid var(--color-border-light);">
                                        <span style="font-weight: 500;">{s.title.clone()}</span>
                                        <div style="font-size: 12px; color: var(--colorNeutralForeground3);">
                                            {s.description.clone()}
                                        </div>
                                    </div>
                                }
                            }
                        />
                    </div>
                </div>
            </Card>
        </div>
    }
}
