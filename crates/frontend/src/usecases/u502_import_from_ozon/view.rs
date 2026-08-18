use super::api::OPS;
use crate::usecases::common::{ImportPage, ImportUseCase};
use leptos::prelude::*;

#[component]
pub fn ImportWidget() -> impl IntoView {
    view! {
        <ImportPage
            page_id="u502_import_from_ozon--usecase"
            title="Загрузка данных OZON"
            use_case=ImportUseCase::Ozon
            storage_prefix="u502"
            ops=OPS
        />
    }
}
