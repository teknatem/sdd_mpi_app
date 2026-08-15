//! Main page component for WB Orders details (MVVM Standard)

use super::tabs::{GeneralTab, JsonTab, LineTab, LinksTab, ProjectionsTab, SalesTab};
use super::view_model::WbOrdersDetailsVm;
use crate::layout::global_context::AppGlobalContext;
use crate::shared::components::page_tabs::{PageTabs, TabItem, TabsVariant};
use crate::shared::icons::icon;
use crate::shared::page_frame::PageFrame;
use crate::system::favorites::ui::FavoriteButton;
use leptos::prelude::*;
use thaw::*;

#[component]
pub fn WbOrdersDetails(id: String, #[prop(into)] on_close: Callback<()>) -> impl IntoView {
    let vm = WbOrdersDetailsVm::new();
    let tabs_store =
        leptos::context::use_context::<AppGlobalContext>().expect("AppGlobalContext not found");
    let stored_id = StoredValue::new(id.clone());

    vm.load(id);

    Effect::new({
        let vm = vm.clone();
        move || {
            if let Some(order_data) = vm.order.get() {
                let tab_key = format!("a015_wb_orders_details_{}", stored_id.get_value());
                let tab_title = format!("WB Order {}", order_data.header.document_no);
                tabs_store.update_tab_title(&tab_key, &tab_title);
            }
        }
    });

    Effect::new({
        let vm = vm.clone();
        move || {
            let active_tab = vm.active_tab.get();
            if matches!(active_tab, "json" | "line") && !vm.raw_json_loaded.get() {
                vm.load_raw_json();
            }
            if matches!(active_tab, "json" | "line") && !vm.marketplace_raw_json_loaded.get() {
                vm.load_marketplace_raw_json();
            }
            if matches!(active_tab, "links" | "line") && !vm.finance_reports_loaded.get() {
                vm.load_finance_reports();
            }
            if active_tab == "sales" && !vm.wb_sales_loaded.get() {
                vm.load_wb_sales();
            }
            if active_tab == "projections" && !vm.projections_loaded.get() {
                vm.load_projections();
            }
        }
    });

    let vm_header = vm.clone();
    let vm_tabs = vm.clone();
    let vm_content = vm.clone();

    view! {
        <PageFrame page_id="a015_wb_orders_details" category="detail">
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
                    } else if vm.order.get().is_some() {
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
    vm: WbOrdersDetailsVm,
    favorite_target_id: String,
    on_close: Callback<()>,
) -> impl IntoView {
    let is_posted = vm.is_posted();
    let document_no = vm.document_no();
    let title = Signal::derive(move || format!("WB Order {}", document_no.get()));
    let order = vm.order;
    let tab_key = format!("a015_wb_orders_details_{}", favorite_target_id);

    view! {
        <div class="page__header">
            <div class="page__header-left">
                <FavoriteButton
                    target_kind="a015_wb_orders_details".to_string()
                    target_id=favorite_target_id
                    target_title=title
                    tab_key=tab_key
                />
                <h1 class = "page__title">{move || title.get()}</h1>
                <Show when=move || order.get().is_some()>
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
                <OrderFlowButton vm=vm.clone() />

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
fn OrderFlowButton(vm: WbOrdersDetailsVm) -> impl IntoView {
    let order = vm.order;
    let tabs_store =
        leptos::context::use_context::<AppGlobalContext>().expect("AppGlobalContext not found");

    let on_click = move |_| {
        if let Some(o) = order.get_untracked() {
            let srid = o.header.document_no.clone();
            if srid.is_empty() {
                return;
            }
            let encoded = urlencoding::encode(&srid);
            let key = format!("d402_wb_order_flow_srid_{}", encoded);
            tabs_store.open_tab(&key, "Вся история");
        }
    };

    view! {
        <Show when=move || order.get().is_some()>
            <Button
                appearance=ButtonAppearance::Subtle
                size=ButtonSize::Medium
                on_click=on_click.clone()
            >
                <span class="page-action-button__content">
                    <span class="page-action-button__icon">{icon("list-ordered")}</span>
                    <span class="page-action-button__text">"Вся история"</span>
                </span>
            </Button>
        </Show>
    }
}

#[component]
fn PostButtons(vm: WbOrdersDetailsVm) -> impl IntoView {
    let posting = vm.posting;
    let order = vm.order;

    let on_post = {
        let vm = vm;
        Callback::new(move |_: ()| vm.post())
    };

    view! {
        <Show when=move || order.get().is_some()>
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
        </Show>
    }
}

#[component]
fn TabBar(vm: WbOrdersDetailsVm) -> impl IntoView {
    let active_tab = vm.active_tab;
    let finance_reports_count = vm.finance_reports_count();
    let wb_sales_count = vm.wb_sales_count();
    let projections_count = vm.projections_count();

    view! {
        <PageTabs
            tabs=vec![
                TabItem::new("general", "Общие").with_icon("file-text"),
                TabItem::new("line", "Подробно").with_icon("list"),
                TabItem::new("json", "JSON").with_icon("code"),
                TabItem::new("links", "Связи")
                    .with_icon("link")
                    .with_badge(Signal::derive(move || finance_reports_count.get().to_string())),
                TabItem::new("projections", "Проекции")
                    .with_icon("activity")
                    .with_badge(Signal::derive(move || projections_count.get().to_string())),
                TabItem::new("sales", "Sales")
                    .with_icon("shopping-cart")
                    .with_badge(Signal::derive(move || wb_sales_count.get().to_string())),
            ]
            active=active_tab.into()
            on_select=Callback::new(move |key: &'static str| vm.set_tab(key))
            variant=TabsVariant::Heavy
        />
    }
}

#[component]
fn TabContent(vm: WbOrdersDetailsVm) -> impl IntoView {
    let active_tab = vm.active_tab;
    let vm_general = vm.clone();
    let vm_line = vm.clone();
    let vm_json = vm.clone();
    let vm_links = vm.clone();
    let vm_sales = vm.clone();
    let vm_projections = vm.clone();

    view! {
        {move || match active_tab.get() {
            "general" => view! { <GeneralTab vm=vm_general.clone() /> }.into_any(),
            "line" => view! { <LineTab vm=vm_line.clone() /> }.into_any(),
            "json" => view! { <JsonTab vm=vm_json.clone() /> }.into_any(),
            "links" => view! { <LinksTab vm=vm_links.clone() /> }.into_any(),
            "sales" => view! { <SalesTab vm=vm_sales.clone() /> }.into_any(),
            "projections" => view! { <ProjectionsTab vm=vm_projections.clone() /> }.into_any(),
            _ => view! { <GeneralTab vm=vm_general.clone() /> }.into_any(),
        }}
    }
}
