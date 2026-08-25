//! Компоненты базы знаний: дерево статей, панель чтения, словарь, разметка.
//!
//! Страницей этот модуль быть перестал. Каркас переехал в
//! `crate::knowledge` — на страницу «Инвентаризация знаний», где дерево статей
//! стало одной вкладкой из шести. Здесь остались части, которые нужны в двух
//! местах сразу: на той вкладке и в отдельной вкладке статьи (`kb_article_*`).
//! Вторая копия любого из них разошлась бы с первой молча.

use super::api::{
    fetch_kb_article, KbArticleDetail, KbArticleSummary, KbTreeNode, KbVocabularyResponse,
};
use super::links::KbLinkedText;
use crate::layout::global_context::AppGlobalContext;
use crate::shared::icons::icon;
use crate::shared::page_frame::PageFrame;
use crate::shared::page_standard::PAGE_CAT_DETAIL;
use leptos::prelude::*;
use leptos::task::spawn_local;
use std::collections::BTreeSet;
use thaw::*;

#[component]
pub fn KnowledgeArticlePage(id: String, #[prop(into)] on_close: Callback<()>) -> impl IntoView {
    let tabs_store =
        leptos::context::use_context::<AppGlobalContext>().expect("AppGlobalContext not found");
    let (article, set_article) = signal::<Option<KbArticleDetail>>(None);
    let (loading, set_loading) = signal(true);
    let (error, set_error) = signal::<Option<String>>(None);
    let id_store = StoredValue::new(id.clone());

    let load = move || {
        let id = id_store.get_value();
        spawn_local(async move {
            set_loading.set(true);
            set_error.set(None);
            match fetch_kb_article(&id).await {
                Ok(payload) => {
                    tabs_store.update_tab_title(
                        &format!("kb_article_{}", payload.id),
                        &format!("KB {}", payload.title),
                    );
                    set_article.set(Some(payload));
                }
                Err(err) => set_error.set(Some(err)),
            }
            set_loading.set(false);
        });
    };

    Effect::new(move |_| load());

    view! {
        <PageFrame page_id="knowledge_base--article" category=PAGE_CAT_DETAIL class="kb-workspace">
            <div class="page__header">
                <div class="page__header-left">
                    <h1 class="page__title">
                        {move || article.get().map(|a| a.title).unwrap_or_else(|| "Статья базы знаний".to_string())}
                    </h1>
                </div>
                <div class="page__header-right">
                    <Space>
                        <Button
                            appearance=ButtonAppearance::Secondary
                            on_click=move |_| load()
                            disabled=Signal::derive(move || loading.get())
                        >
                            "Обновить"
                        </Button>
                        <Button appearance=ButtonAppearance::Secondary on_click=move |_| on_close.run(())>
                            "Закрыть"
                        </Button>
                    </Space>
                </div>
            </div>
            <div class="page__content">
                {move || {
                    if loading.get() {
                        return view! {
                            <Flex gap=FlexGap::Small style="align-items: center; padding: var(--spacing-4xl); justify-content: center;">
                                <Spinner />
                                <span>"Загрузка..."</span>
                            </Flex>
                        }.into_any();
                    }
                    if let Some(err) = error.get() {
                        return view! { <div class="alert alert--error">{err}</div> }.into_any();
                    }
                    if let Some(article) = article.get() {
                        view! {
                            <KnowledgeArticlePanel article=article show_header=false on_open=Callback::new(|_| {}) />
                        }.into_any()
                    } else {
                        view! { <p>"Статья не найдена."</p> }.into_any()
                    }
                }}
            </div>
        </PageFrame>
    }
}

#[derive(Debug, Clone)]
pub struct FlatKbTreeNode {
    level: usize,
    name: String,
    path: String,
    article: Option<KbArticleSummary>,
    is_collapsed: bool,
}

#[component]
pub fn KbTreeRow(
    node: FlatKbTreeNode,
    collapsed_paths: RwSignal<BTreeSet<String>>,
    on_select: Callback<KbArticleSummary>,
) -> impl IntoView {
    let is_article = node.article.is_some();
    let article = node.article.clone();
    let padding = format!("margin-left: {}px;", node.level * 16);
    view! {
        <div style=padding>
            {if is_article {
                let article = article.expect("article node must have article");
                view! {
                    <button
                        class="page__tab"
                        style="border: none; background: transparent; padding: 2px 0; cursor: pointer;"
                        on:click=move |_| on_select.run(article.clone())
                    >
                        {icon("file-text")} {node.name.clone()}
                    </button>
                }.into_any()
            } else {
                let path = node.path.clone();
                let icon_name = if node.is_collapsed {
                    "chevron-right"
                } else {
                    "chevron-down"
                };
                view! {
                    <button
                        class="page__tab"
                        style="border: none; background: transparent; padding: 2px 0; cursor: pointer; font-weight: 600;"
                        on:click=move |_| {
                            collapsed_paths.update(|paths| {
                                if !paths.insert(path.clone()) {
                                    paths.remove(&path);
                                }
                            });
                        }
                    >
                        {icon(icon_name)} {icon("folder")} {node.name.clone()}
                    </button>
                }.into_any()
            }}
        </div>
    }
}

/// Словарь тегов: канонические термины по группам + рабочий список куратора.
#[component]
pub fn KbVocabularyView(vocabulary: Option<KbVocabularyResponse>) -> impl IntoView {
    let Some(vocabulary) = vocabulary else {
        return view! {
            <p style="color: var(--colorNeutralForeground3);">"Словарь тегов загружается..."</p>
        }
        .into_any();
    };

    if vocabulary.terms.is_empty() {
        return view! {
            <p style="color: var(--colorNeutralForeground3);">
                "Словарь не заполнен. Создайте файл " <code>"_vocabulary.md"</code>
                " в каталоге базы знаний — он задаёт канонические теги и их синонимы."
            </p>
        }
        .into_any();
    }

    // Группируем на клиенте: бэкенд отдаёт плоский список, отсортированный по тегу.
    let mut groups: std::collections::BTreeMap<String, Vec<_>> = Default::default();
    for term in vocabulary.terms {
        groups
            .entry(if term.group.is_empty() {
                "прочее".to_string()
            } else {
                term.group.clone()
            })
            .or_default()
            .push(term);
    }

    let outside = vocabulary.tags_outside_vocabulary;
    view! {
        <div class="kb-vocabulary">
            {groups.into_iter().map(|(group, terms)| view! {
                <div class="kb-vocabulary__group">
                    <div class="kb-vocabulary__group-title">{group}</div>
                    {terms.into_iter().map(|term| view! {
                        <div class="kb-vocabulary__term" title=term.description.clone()>
                            <Badge appearance=BadgeAppearance::Tint color=BadgeColor::Informative>
                                {term.tag.clone()}
                            </Badge>
                            <span class="kb-vocabulary__count">{format!("{}", term.articles)}</span>
                            {(!term.aliases.is_empty()).then(|| view! {
                                <span class="kb-vocabulary__aliases">
                                    {format!("= {}", term.aliases.join(", "))}
                                </span>
                            })}
                        </div>
                    }).collect_view()}
                </div>
            }).collect_view()}

            {(!outside.is_empty()).then(|| view! {
                <div class="kb-vocabulary__group kb-vocabulary__group--warning">
                    <div class="kb-vocabulary__group-title">
                        {format!("Теги вне словаря ({})", outside.len())}
                    </div>
                    <p class="kb-vocabulary__hint">
                        "Использованы в статьях, но не описаны в " <code>"_vocabulary.md"</code>
                        ". Добавьте в словарь или замените в статьях на канонические."
                    </p>
                    {outside.into_iter().map(|item| view! {
                        <div class="kb-vocabulary__term">
                            <Badge appearance=BadgeAppearance::Tint color=BadgeColor::Warning>
                                {item.name}
                            </Badge>
                            <span class="kb-vocabulary__count">{format!("{}", item.count)}</span>
                        </div>
                    }).collect_view()}
                </div>
            })}
        </div>
    }
    .into_any()
}

/// Звёзды важности как статичный текст: источник истины — файл, редактировать
/// оценку из веба нельзя (правится в Obsidian, потом «Перечитать базу»).
fn stars_label(stars: Option<u8>) -> String {
    match stars {
        Some(n) => {
            let n = n.clamp(1, 5) as usize;
            format!("{}{}", "★".repeat(n), "☆".repeat(5 - n))
        }
        None => String::new(),
    }
}

fn status_badge(status: &str) -> Option<(&'static str, BadgeColor)> {
    match status {
        "draft" => Some(("черновик", BadgeColor::Warning)),
        "deprecated" => Some(("устарела", BadgeColor::Danger)),
        _ => None,
    }
}

#[component]
pub fn KnowledgeArticlePanel(
    article: KbArticleDetail,
    /// Show the article title as a clickable link (set false when the page header already shows it).
    #[prop(default = true)]
    show_header: bool,
    on_open: Callback<()>,
) -> impl IntoView {
    let tabs_store =
        leptos::context::use_context::<AppGlobalContext>().expect("AppGlobalContext not found");
    let tags = article.tags.clone();
    let content = article.content.clone();
    let display_path_title = article.display_path.clone();
    let summary_text = article.summary.clone();
    let stars = stars_label(article.stars);
    let status = status_badge(&article.status);
    let token_cost = article.token_cost;
    let updated = article.updated.clone();
    let staleness = article.staleness_pct;
    let metrics = article.metrics.clone();
    let corpus = corpus_label(corpus_of(&article.kind, article.is_embedded));
    // Якорь на несуществующий объект — работа куратору: связь молча не работает.
    let unknown_anchors = article.unknown_anchors.join(", ");
    // Граф: связи и обратные ссылки показываем одним списком — направление
    // ребра для навигации значения не имеет.
    let mut graph_links = article.related_articles.clone();
    for link in &article.back_links {
        if !graph_links.iter().any(|l| l.id == link.id) {
            graph_links.push(link.clone());
        }
    }

    view! {
        <Card>
            // ── Compact article header ─────────────────────────────────────
            <div style="margin-bottom: var(--spacing-sm);">
                {if show_header {
                    view! {
                        <div style="margin-bottom: 2px;">
                            <a
                                href="#"
                                class="table__link"
                                style="font-size: 0.95em; font-weight: 600;"
                                on:click=move |ev| { ev.prevent_default(); on_open.run(()); }
                            >
                                {article.title.clone()}
                            </a>
                        </div>
                    }.into_any()
                } else {
                    view! { <span></span> }.into_any()
                }}

                // Ценность и актуальность: звёзды · статус · токены · свежесть · замечания
                <div class="kb-meta">
                    {(!stars.is_empty()).then(|| view! {
                        <span class="kb-meta__stars" title="Важность знания">{stars}</span>
                    })}
                    {status.map(|(label, color)| view! {
                        <Badge appearance=BadgeAppearance::Filled color=color>{label}</Badge>
                    })}
                    {(token_cost > 0).then(|| view! {
                        <span title="Оценка стоимости чтения статьи">{format!("~{} токенов", token_cost)}</span>
                    })}
                    {updated.map(|date| view! { <span>{format!("обновлено {}", date)}</span> })}
                    {staleness.filter(|pct| *pct > 70).map(|pct| view! {
                        <span class="kb-meta__stale" title="Израсходован срок годности знания">
                            {format!("протухает: {}%", pct)}
                        </span>
                    })}
                    {(metrics.open_issues > 0).then(|| view! {
                        <span class="kb-meta__issues" title="Открытые замечания к статье">
                            {format!("{} замечаний", metrics.open_issues)}
                        </span>
                    })}
                    {(!unknown_anchors.is_empty()).then(|| view! {
                        <span class="kb-meta__issues" title="Поле entities ссылается на объект, которого нет в реестре">
                            {format!("якоря вне реестра: {}", unknown_anchors)}
                        </span>
                    })}
                    {(metrics.search_hits + metrics.read_hits + metrics.cited_hits > 0).then(|| view! {
                        <span
                            class="kb-meta__usage"
                            title="Поиск нашёл / модель прочитала / попало в ответ пользователю"
                        >
                            {format!(
                                "поиск {} · чтений {} · цитат {}",
                                metrics.search_hits, metrics.read_hits, metrics.cited_hits,
                            )}
                        </span>
                    })}
                </div>

                {(!summary_text.is_empty()).then(|| view! {
                    <div class="kb-meta__summary">{summary_text}</div>
                })}

                // Single-row: ID · path · type · [tags]
                <div style="font-size: 11px; color: var(--colorNeutralForeground3); display: flex; align-items: center; flex-wrap: wrap; gap: 3px; line-height: 1.6;">
                    <span>"ID"</span>
                    <code style="font-size: 11px; color: var(--colorNeutralForeground2);">{article.id.clone()}</code>
                    <span style="padding: 0 2px; color: var(--colorNeutralStroke1);">"·"</span>
                    <code
                        style="font-size: 11px; color: var(--colorNeutralForeground2); max-width: 260px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;"
                        title=display_path_title
                    >
                        {article.display_path.clone()}
                    </code>
                    <span style="padding: 0 2px; color: var(--colorNeutralStroke1);">"·"</span>
                    <span>{corpus}</span>
                    {if !tags.is_empty() {
                        view! {
                            <span style="padding: 0 2px; color: var(--colorNeutralStroke1);">"·"</span>
                            {tags.into_iter().map(|tag| view! {
                                <Badge appearance=BadgeAppearance::Tint color=BadgeColor::Informative>{tag}</Badge>
                            }).collect_view()}
                        }.into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }}
                </div>

                // Граф связей — единственное видимое лицо `related` в интерфейсе.
                {(!graph_links.is_empty()).then(move || view! {
                    <div class="kb-meta kb-meta--links">
                        <span>"Связано:"</span>
                        {graph_links.into_iter().map(move |link| {
                            let id = link.id.clone();
                            let title = link.title.clone();
                            view! {
                                <a
                                    href="#"
                                    class="table__link"
                                    on:click=move |ev| {
                                        ev.prevent_default();
                                        tabs_store.open_tab(
                                            &format!("kb_article_{}", id),
                                            &format!("KB {}", title),
                                        );
                                    }
                                >
                                    {link.title.clone()}
                                </a>
                            }
                        }).collect_view()}
                    </div>
                })}
            </div>
            // ── Content ────────────────────────────────────────────────────
            <KbMarkdown text=content />
        </Card>
    }
}

// ── Minimal Markdown renderer ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum MdBlock {
    H1(String),
    H2(String),
    H3(String),
    List(Vec<String>),
    Code(Vec<String>),
    Text(String),
    Empty,
}

fn parse_md_blocks(text: &str) -> Vec<MdBlock> {
    let mut blocks: Vec<MdBlock> = Vec::new();
    let mut list_buf: Vec<String> = Vec::new();
    let mut code_buf: Option<Vec<String>> = None;

    let flush_list = |blocks: &mut Vec<MdBlock>, list_buf: &mut Vec<String>| {
        if !list_buf.is_empty() {
            blocks.push(MdBlock::List(std::mem::take(list_buf)));
        }
    };

    for line in text.lines() {
        // Toggle code block.
        if line.starts_with("```") {
            if let Some(lines) = code_buf.take() {
                flush_list(&mut blocks, &mut list_buf);
                blocks.push(MdBlock::Code(lines));
            } else {
                flush_list(&mut blocks, &mut list_buf);
                code_buf = Some(Vec::new());
            }
            continue;
        }

        if let Some(buf) = &mut code_buf {
            buf.push(line.to_string());
            continue;
        }

        // Headings.
        if let Some(rest) = line
            .strip_prefix("#### ")
            .or_else(|| line.strip_prefix("### "))
        {
            flush_list(&mut blocks, &mut list_buf);
            blocks.push(MdBlock::H3(rest.to_string()));
        } else if let Some(rest) = line.strip_prefix("## ") {
            flush_list(&mut blocks, &mut list_buf);
            blocks.push(MdBlock::H2(rest.to_string()));
        } else if let Some(rest) = line.strip_prefix("# ") {
            flush_list(&mut blocks, &mut list_buf);
            blocks.push(MdBlock::H1(rest.to_string()));
        } else if let Some(rest) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
            list_buf.push(rest.to_string());
        } else if line.is_empty() {
            flush_list(&mut blocks, &mut list_buf);
            // Collapse consecutive empties.
            if !matches!(blocks.last(), Some(MdBlock::Empty)) {
                blocks.push(MdBlock::Empty);
            }
        } else {
            flush_list(&mut blocks, &mut list_buf);
            blocks.push(MdBlock::Text(line.to_string()));
        }
    }

    flush_list(&mut blocks, &mut list_buf);
    if let Some(lines) = code_buf.take() {
        blocks.push(MdBlock::Code(lines));
    }

    blocks
}

#[component]
fn KbMarkdown(text: String) -> impl IntoView {
    let blocks = parse_md_blocks(&text);
    view! {
        <div>
            {blocks.into_iter().map(|block| match block {
                MdBlock::H1(t) => view! {
                    <div style="color: var(--colorBrandForeground1); font-size: 1.15em; font-weight: 700; margin: 0.6em 0 0.2em; padding-bottom: 2px; border-bottom: 1px solid var(--colorNeutralStroke2);">
                        <KbLinkedText text=t />
                    </div>
                }.into_any(),
                MdBlock::H2(t) => view! {
                    <div style="color: var(--colorBrandForeground2); font-size: 1.05em; font-weight: 600; margin: 0.5em 0 0.15em;">
                        <KbLinkedText text=t />
                    </div>
                }.into_any(),
                MdBlock::H3(t) => view! {
                    <div style="color: var(--colorNeutralForeground1); font-size: 0.95em; font-weight: 600; margin: 0.4em 0 0.1em;">
                        <KbLinkedText text=t />
                    </div>
                }.into_any(),
                MdBlock::List(items) => view! {
                    <ul style="margin: 0.2em 0 0.2em 1.2em; padding: 0; list-style: disc;">
                        {items.into_iter().map(|item| view! {
                            <li style="margin: 0.1em 0; color: var(--colorNeutralForeground1);">
                                <KbLinkedText text=item />
                            </li>
                        }).collect_view()}
                    </ul>
                }.into_any(),
                MdBlock::Code(lines) => view! {
                    <pre style="background: var(--colorNeutralBackground2); padding: 4px 8px; border-radius: 4px; font-family: var(--font-family-monospace, monospace); font-size: 0.82em; overflow-x: auto; margin: 0.25em 0; white-space: pre;">
                        {lines.join("\n")}
                    </pre>
                }.into_any(),
                MdBlock::Text(t) => view! {
                    <div style="margin: 0.05em 0; line-height: 1.5;">
                        <KbLinkedText text=t />
                    </div>
                }.into_any(),
                MdBlock::Empty => view! {
                    <div style="height: 0.35em;"></div>
                }.into_any(),
            }).collect_view()}
        </div>
    }
}

// ── Tree helpers ──────────────────────────────────────────────────────────────

pub fn flatten_visible_tree(
    nodes: &[KbTreeNode],
    collapsed_paths: &BTreeSet<String>,
) -> Vec<FlatKbTreeNode> {
    let mut result = Vec::new();
    for node in nodes {
        flatten_visible_tree_node(node, 0, collapsed_paths, &mut result);
    }
    result
}

/// Корпус статьи. Пустой `kind` — ответ старого бэкенда, там был только признак
/// «встроенная», и техдок от карты не отличался.
fn corpus_of(kind: &str, is_embedded: bool) -> &str {
    match kind {
        "" if is_embedded => "app",
        "" => "business",
        kind => kind,
    }
}

fn corpus_label(corpus: &str) -> &'static str {
    match corpus {
        "app" => "документация приложения",
        "generated" => "карта из данных",
        _ => "статья организации",
    }
}

/// Дерево одного корпуса: `business` | `app` | `generated`.
pub fn filter_tree_by_corpus(nodes: &[KbTreeNode], corpus: &str) -> Vec<KbTreeNode> {
    nodes
        .iter()
        .filter_map(|node| filter_tree_node_by_corpus(node, corpus))
        .collect()
}

fn filter_tree_node_by_corpus(node: &KbTreeNode, corpus: &str) -> Option<KbTreeNode> {
    if let Some(article) = &node.article {
        return (corpus_of(&article.kind, article.is_embedded) == corpus).then(|| node.clone());
    }

    let children = filter_tree_by_corpus(&node.children, corpus);
    if children.is_empty() {
        None
    } else {
        Some(KbTreeNode {
            name: node.name.clone(),
            path: node.path.clone(),
            node_type: node.node_type.clone(),
            article: None,
            children,
        })
    }
}

fn flatten_visible_tree_node(
    node: &KbTreeNode,
    level: usize,
    collapsed_paths: &BTreeSet<String>,
    result: &mut Vec<FlatKbTreeNode>,
) {
    let is_collapsed = collapsed_paths.contains(&node.path);
    result.push(FlatKbTreeNode {
        level,
        name: node.name.clone(),
        path: node.path.clone(),
        article: node.article.clone(),
        is_collapsed,
    });
    if is_collapsed {
        return;
    }
    for child in &node.children {
        flatten_visible_tree_node(child, level + 1, collapsed_paths, result);
    }
}

pub fn collect_folder_paths(nodes: &[KbTreeNode]) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    for node in nodes {
        collect_folder_paths_node(node, &mut paths);
    }
    paths
}

fn collect_folder_paths_node(node: &KbTreeNode, paths: &mut BTreeSet<String>) {
    if node.article.is_none() && !node.children.is_empty() {
        paths.insert(node.path.clone());
    }
    for child in &node.children {
        collect_folder_paths_node(child, paths);
    }
}

pub fn article_summary(article: &KbArticleDetail) -> KbArticleSummary {
    KbArticleSummary {
        id: article.id.clone(),
        title: article.title.clone(),
        tags: article.tags.clone(),
        related: article.related.clone(),
        source_path: article.source_path.clone(),
        display_path: article.display_path.clone(),
        is_embedded: article.is_embedded,
        kind: article.kind.clone(),
        summary: article.summary.clone(),
        status: article.status.clone(),
        stars: article.stars,
        updated: article.updated.clone(),
        verified: article.verified.clone(),
        ttl_days: article.ttl_days,
        token_cost: article.token_cost,
        staleness_pct: article.staleness_pct,
        unknown_tags: article.unknown_tags.clone(),
        unknown_anchors: article.unknown_anchors.clone(),
        metrics: article.metrics.clone(),
    }
}
