use super::api::OPS;
use crate::usecases::common::{ImportPage, ImportUseCase};
use leptos::prelude::*;

#[component]
pub fn ImportWidget() -> impl IntoView {
    view! {
        <ImportPage
            page_id="u503_import_from_yandex--usecase"
            title="Загрузка данных Yandex Market"
            use_case=ImportUseCase::Yandex
            storage_prefix="u503"
            ops=OPS
        />
    }
}
