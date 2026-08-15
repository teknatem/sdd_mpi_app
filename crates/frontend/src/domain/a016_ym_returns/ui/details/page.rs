//! Main page component for YM Returns details (MVVM Standard, mirrors a015_wb_orders)

use super::tabs::{GeneralTab, JsonTab, LinesTab, ProjectionsTab};
use super::view_model::YmReturnDetailsVm;
use crate::layout::global_context::AppGlobalContext;
use crate::shared::components::page_tabs::{PageTabs, TabItem, TabsVariant};
use crate::shared::icons::icon;
use crate::shared::page_frame::PageFrame;
use crate::system::favorites::ui::FavoriteButton;
use leptos::prelude::*;
use thaw::*;

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
    let vm_content = vm.clone();

    let projections_count = vm.projections_count();
    let tab_items = vec![
        TabItem::new("general", "Общие").with_icon("file-text"),
        TabItem::new("lines", "Товары").with_icon("list"),
        TabItem::new("projections", "Проекции")
            .with_icon("layers")
            .with_badge(Signal::derive(move || projections_count.get().to_string())),
        TabItem::new("json", "JSON").with_icon("code"),
    ];
    let active_tab = vm.active_tab;
    let on_select_tab = Callback::new({
        let vm = vm.clone();
        move |key: &'static str| vm.set_tab(key)
    });

    view! {
        <PageFrame page_id="a016_ym_returns--detail" category="detail">
            <Header
                vm=vm_header
                favorite_target_id=stored_id.get_value()
                on_close=on_close
            />

            <PageTabs
                tabs=tab_items
                active=active_tab.into()
                on_select=on_select_tab
                variant=TabsVariant::Heavy
            />

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
