use super::ops::OPS;
use crate::usecases::shared::{ImportPage, ImportUseCase};
use leptos::prelude::*;

#[component]
pub fn ImportWidget() -> impl IntoView {
    view! {
        <ImportPage
            page_id="u504_import_from_wildberries--usecase"
            title="Загрузка данных Wildberries"
            use_case=ImportUseCase::Wildberries
            storage_prefix="u504"
            ops=OPS
        />
    }
}
