//! Вкладка «Статьи» — то, что раньше и было страницей «База знаний».
//!
//! Переехало целиком: дерево по каталогам с тремя корпусами, чтение статьи с
//! разрешением ссылок, перечитывание базы с диска и пересборка карт. Ничего из
//! этого не потеряно — потеря функциональности была бы не упрощением, а
//! регрессом.
//!
//! Компоненты дерева и панели статьи остались в `shared/knowledge_base/`:
//! их же использует отдельная вкладка статьи (`kb_article_*`), и вторая копия
//! разошлась бы с первой.

use std::collections::BTreeSet;

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::layout::global_context::AppGlobalContext;
use crate::shared::knowledge_base::api::{
    fetch_kb_article, fetch_kb_tree, post_kb_generate, post_kb_reload, KbArticleDetail,
    KbArticleSummary, KbTreeNode,
};
use crate::shared::knowledge_base::ui::{
    article_summary, collect_folder_paths, filter_tree_by_corpus, flatten_visible_tree, KbTreeRow,
    KnowledgeArticlePanel,
};

#[component]
pub fn ArticlesTab() -> impl IntoView {
    let tabs_store =
        leptos::context::use_context::<AppGlobalContext>().expect("AppGlobalContext not found");
    let tree = RwSignal::new(Vec::<KbTreeNode>::new());
    let selected = RwSignal::new(None::<KbArticleDetail>);
    let error = RwSignal::new(None::<String>);
    let notice = RwSignal::new(None::<String>);
    let loading = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let corpus = RwSignal::new("business".to_string());
    let collapsed_paths = RwSignal::new(BTreeSet::<String>::new());

    let load = move || {
        spawn_local(async move {
            loading.set(true);
            error.set(None);
            match fetch_kb_tree().await {
                Ok(payload) => tree.set(payload.roots),
                Err(message) => error.set(Some(message)),
            }
            loading.set(false);
        });
    };
    Effect::new(move |_| load());

    let select_article = Callback::new(move |article: KbArticleSummary| {
        spawn_local(async move {
            error.set(None);
            match fetch_kb_article(&article.id).await {
                Ok(detail) => selected.set(Some(detail)),
                Err(message) => error.set(Some(message)),
            }
        });
    });

    // Статьи правятся в Obsidian снаружи приложения: без явного перечитывания
    // правка видна только после рестарта бэкенда.
    let reload_from_disk = move || {
        spawn_local(async move {
            busy.set(true);
            notice.set(None);
            match post_kb_reload().await {
                Ok(report) => {
                    notice.set(Some(format!(
                        "База перечитана: {} статей, словарь — {} терминов, тегов вне словаря — {}.",
                        report.total_articles, report.vocabulary_terms, report.unknown_tag_count
                    )));
                    load();
                }
                Err(message) => error.set(Some(message)),
            }
            busy.set(false);
        });
    };

    // Карты корпуса `generated` собираются из БД и рантайма: после импорта или
    // установки плагина в базе висят прошлые цифры, пока их не пересобрать.
    let regenerate_maps = move || {
        spawn_local(async move {
            busy.set(true);
            notice.set(None);
            match post_kb_generate().await {
                Ok(report) => {
                    notice.set(Some(format!(
                        "Карты пересобраны: {} файлов, таблиц — {}, плагинов — {}, навыков — {}.{}",
                        report.files.len(),
                        report.tables_profiled,
                        report.plugins,
                        report.skills,
                        if report.errors.is_empty() {
                            String::new()
                        } else {
                            format!(" Ошибки: {}", report.errors.join("; "))
                        }
                    )));
                    load();
                }
                Err(message) => error.set(Some(message)),
            }
            busy.set(false);
        });
    };

    view! {
        <div class="knowledge-inventory__toolbar">
            <div class="knowledge-inventory__chips">
                {["business", "app", "generated"].into_iter().map(|code| {
                    let label = match code {
                        "app" => "Техдоки приложения",
                        "generated" => "Карты из данных",
                        _ => "Статьи о предметной области",
                    };
                    view! {
                        <button
                            class=move || if corpus.get() == code {
                                "knowledge-inventory__chip knowledge-inventory__chip--on"
                            } else {
                                "knowledge-inventory__chip"
                            }
                            on:click=move |_| corpus.set(code.to_string())
                        >{label}</button>
                    }
                }).collect_view()}
            </div>
            <div class="knowledge-inventory__chips">
                <button
                    class="button button--ghost"
                    on:click=move |_| collapsed_paths.set(BTreeSet::new())
                >"Развернуть"</button>
                <button
                    class="button button--ghost"
                    on:click=move |_| {
                        let current = filter_tree_by_corpus(&tree.get(), &corpus.get_untracked());
                        collapsed_paths.set(collect_folder_paths(&current));
                    }
                >"Свернуть"</button>
                <button
                    class="button button--secondary"
                    disabled=move || busy.get()
                    on:click=move |_| reload_from_disk()
                >"Перечитать с диска"</button>
                <button
                    class="button button--secondary"
                    disabled=move || busy.get()
                    on:click=move |_| regenerate_maps()
                >"Пересобрать карты"</button>
            </div>
        </div>

        {move || error.get().map(|message| view! {
            <div class="knowledge-inventory__banner knowledge-inventory__banner--error">{message}</div>
        })}
        {move || notice.get().map(|message| view! {
            <div class="knowledge-inventory__banner">{message}</div>
        })}

        <div class="knowledge-inventory__split">
            <div class="knowledge-inventory__tree">
                {move || {
                    let current = filter_tree_by_corpus(&tree.get(), &corpus.get());
                    if current.is_empty() {
                        return view! {
                            <div class="knowledge-inventory__empty">
                                {match corpus.get().as_str() {
                                    "app" => "Техдоков приложения не найдено.",
                                    "generated" => "Карт нет — соберите их кнопкой «Пересобрать карты».",
                                    _ => "Курируемых статей об организации пока нет.",
                                }}
                            </div>
                        }.into_any();
                    }
                    view! {
                        {flatten_visible_tree(&current, &collapsed_paths.get())
                            .into_iter()
                            .map(|node| view! {
                                <KbTreeRow
                                    node=node
                                    collapsed_paths=collapsed_paths
                                    on_select=select_article
                                />
                            })
                            .collect_view()}
                    }.into_any()
                }}
            </div>

            <div class="knowledge-inventory__article">
                {move || match selected.get() {
                    Some(article) => {
                        let summary = article_summary(&article);
                        let store = tabs_store;
                        view! {
                            <KnowledgeArticlePanel
                                article=article
                                show_header=true
                                on_open=Callback::new(move |_| {
                                    store.open_tab(
                                        &format!("kb_article_{}", summary.id),
                                        &format!("KB {}", summary.title),
                                    );
                                })
                            />
                        }.into_any()
                    }
                    None => view! {
                        <div class="knowledge-inventory__empty">
                            "Выберите статью в дереве слева."
                        </div>
                    }.into_any(),
                }}
            </div>
        </div>
    }
}
