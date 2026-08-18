mod api;
mod page;
mod view_model;

use leptos::prelude::*;
pub use page::MarketplaceProductDetails;

#[component]
pub fn MarketplaceProductDetailsTab(
    id: String,
    #[prop(into)] on_close: Callback<()>,
) -> impl IntoView {
    let id_opt = if id == "new" || id.is_empty() {
        None
    } else {
        Some(id)
    };

    view! {
        <MarketplaceProductDetails id=id_opt on_close=on_close />
    }
}
