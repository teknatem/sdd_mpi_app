//! Вкладка «Словарь» — канонические теги и то, что мимо них.
//!
//! Теги вне словаря — рабочий список куратора, а не ошибка: тег появляется в
//! статье раньше, чем в словаре, и вопрос только в том, дошли ли до него руки.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::shared::knowledge_base::api::{fetch_kb_vocabulary, KbVocabularyResponse};
use crate::shared::knowledge_base::ui::KbVocabularyView;

#[component]
pub fn VocabularyTab() -> impl IntoView {
    let vocabulary = RwSignal::new(None::<KbVocabularyResponse>);
    let error = RwSignal::new(None::<String>);

    Effect::new(move |_| {
        spawn_local(async move {
            match fetch_kb_vocabulary().await {
                Ok(payload) => vocabulary.set(Some(payload)),
                Err(message) => error.set(Some(message)),
            }
        });
    });

    view! {
        {move || error.get().map(|message| view! {
            <div class="knowledge-inventory__banner knowledge-inventory__banner--error">{message}</div>
        })}
        <KbVocabularyView vocabulary=vocabulary.get() />
    }
}
