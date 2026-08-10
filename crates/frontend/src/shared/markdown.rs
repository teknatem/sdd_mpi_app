//! Лёгкий рендер Markdown для ответов LLM и просмотра контекста.
//!
//! Поддерживает: заголовки (#..####), маркированные и нумерованные списки,
//! блоки кода ```` ``` ````, цитаты `>`, простые таблицы `| a | b |`, а также
//! инлайн `**bold**`, `*italic*`, `` `code` ``. Не полноценный CommonMark, но
//! воспроизводит типичное форматирование ответов модели.

use leptos::prelude::*;

/// Пункт списка. Уровень берётся из отступа исходной строки — без него
/// подпункты схлопывались в один уровень и ответ читался как плоская простыня.
#[derive(Debug, Clone, PartialEq)]
struct ListItem {
    depth: usize,
    ordered: bool,
    text: String,
}

#[derive(Debug, Clone)]
enum Block {
    H1(String),
    H2(String),
    H3(String),
    /// Весь список одним блоком — маркированные и нумерованные пункты вперемешку.
    /// Дробить его нельзя: браузер начинает нумерацию каждого нового `<ol>`
    /// с единицы, и «1. … 2. …» превращалось в «1. … 1. …».
    List(Vec<ListItem>),
    Code(Vec<String>),
    Quote(Vec<String>),
    Table(Vec<Vec<String>>),
    Text(String),
    Empty,
}

fn is_table_sep(cells: &[String]) -> bool {
    !cells.is_empty()
        && cells.iter().all(|c| {
            let t = c.trim();
            !t.is_empty() && t.chars().all(|ch| ch == '-' || ch == ':')
        })
}

fn split_row(line: &str) -> Vec<String> {
    let t = line.trim().trim_start_matches('|').trim_end_matches('|');
    t.split('|').map(|c| c.trim().to_string()).collect()
}

fn flush_list(b: &mut Vec<Block>, buf: &mut Vec<ListItem>) {
    if !buf.is_empty() {
        b.push(Block::List(std::mem::take(buf)));
    }
}

fn flush_quote(b: &mut Vec<Block>, buf: &mut Vec<String>) {
    if !buf.is_empty() {
        b.push(Block::Quote(std::mem::take(buf)));
    }
}

fn flush_table(b: &mut Vec<Block>, buf: &mut Vec<Vec<String>>) {
    if !buf.is_empty() {
        b.push(Block::Table(std::mem::take(buf)));
    }
}

fn push_empty(b: &mut Vec<Block>) {
    if !matches!(b.last(), Some(Block::Empty)) {
        b.push(Block::Empty);
    }
}

/// Закрыть незавершённые список и цитату: строка, которая пришла, к ним не относится.
///
/// `blank_pending` — придержанная пустая строка после списка: если продолжения
/// не случилось, она превращается в обычный отбивочный интервал.
fn close_open_blocks(
    b: &mut Vec<Block>,
    list: &mut Vec<ListItem>,
    quote: &mut Vec<String>,
    blank_pending: &mut bool,
) {
    flush_list(b, list);
    if *blank_pending {
        push_empty(b);
        *blank_pending = false;
    }
    flush_quote(b, quote);
}

/// `- текст` / `* текст` / `1. текст` / `1) текст` → `(нумерованный?, текст)`.
fn strip_list_marker(trimmed: &str) -> Option<(bool, String)> {
    if let Some(rest) = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
    {
        return Some((false, rest.to_string()));
    }
    strip_ordered(trimmed).map(|rest| (true, rest))
}

fn parse_blocks(text: &str) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut list: Vec<ListItem> = Vec::new();
    let mut quote: Vec<String> = Vec::new();
    let mut table: Vec<Vec<String>> = Vec::new();
    let mut code: Option<Vec<String>> = None;
    // Пустая строка внутри списка придерживается, а не обрывает его: модели
    // разделяют пункты абзацем, а разрыв сбрасывал нумерацию на «1.».
    let mut blank_pending = false;

    for line in text.lines() {
        // Блок кода — переключатель.
        if line.trim_start().starts_with("```") {
            if let Some(lines) = code.take() {
                blocks.push(Block::Code(lines));
            } else {
                close_open_blocks(&mut blocks, &mut list, &mut quote, &mut blank_pending);
                flush_table(&mut blocks, &mut table);
                code = Some(Vec::new());
            }
            continue;
        }
        if let Some(buf) = &mut code {
            buf.push(line.to_string());
            continue;
        }

        let trimmed = line.trim_start();

        // Таблица.
        if trimmed.starts_with('|') {
            close_open_blocks(&mut blocks, &mut list, &mut quote, &mut blank_pending);
            let cells = split_row(trimmed);
            if !is_table_sep(&cells) {
                table.push(cells);
            }
            continue;
        } else {
            flush_table(&mut blocks, &mut table);
        }

        // Пункт списка — раньше прочих веток: только он умеет пережить пустую
        // строку и подхватить вложенный маркер другого вида.
        if let Some((ordered, item)) = strip_list_marker(trimmed) {
            flush_quote(&mut blocks, &mut quote);
            let depth = list_depth(line);
            // Смена вида маркера на верхнем уровне — это уже другой список.
            if depth == 0 && list.first().map_or(false, |first| first.ordered != ordered) {
                flush_list(&mut blocks, &mut list);
            }
            blank_pending = false;
            list.push(ListItem {
                depth,
                ordered,
                text: item,
            });
            continue;
        }

        // Заголовки.
        if let Some(rest) = trimmed
            .strip_prefix("#### ")
            .or_else(|| trimmed.strip_prefix("### "))
        {
            close_open_blocks(&mut blocks, &mut list, &mut quote, &mut blank_pending);
            blocks.push(Block::H3(rest.to_string()));
        } else if let Some(rest) = trimmed.strip_prefix("## ") {
            close_open_blocks(&mut blocks, &mut list, &mut quote, &mut blank_pending);
            blocks.push(Block::H2(rest.to_string()));
        } else if let Some(rest) = trimmed.strip_prefix("# ") {
            close_open_blocks(&mut blocks, &mut list, &mut quote, &mut blank_pending);
            blocks.push(Block::H1(rest.to_string()));
        } else if let Some(rest) = trimmed.strip_prefix("> ") {
            flush_list(&mut blocks, &mut list);
            if blank_pending {
                push_empty(&mut blocks);
                blank_pending = false;
            }
            quote.push(rest.to_string());
        } else if trimmed.is_empty() {
            if list.is_empty() {
                flush_quote(&mut blocks, &mut quote);
                push_empty(&mut blocks);
            } else {
                blank_pending = true;
            }
        } else {
            close_open_blocks(&mut blocks, &mut list, &mut quote, &mut blank_pending);
            blocks.push(Block::Text(line.to_string()));
        }
    }

    close_open_blocks(&mut blocks, &mut list, &mut quote, &mut blank_pending);
    flush_table(&mut blocks, &mut table);
    if let Some(lines) = code.take() {
        blocks.push(Block::Code(lines));
    }
    blocks
}

/// Уровень вложенности пункта по отступу: два пробела (или таб) = один уровень.
/// Ограничен двумя уровнями — глубже список перестаёт читаться.
fn list_depth(line: &str) -> usize {
    let spaces = line
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum::<usize>();
    (spaces / 2).min(2)
}

/// `123. текст` или `123) текст` → Some("текст").
///
/// Форму со скобкой модели используют не реже точки, а раньше она не
/// распознавалась вовсе и падала в обычный абзац — нумерация выглядела как
/// текст, без отступов и без выравнивания.
fn strip_ordered(line: &str) -> Option<String> {
    let mut digits = 0;
    for c in line.chars() {
        if c.is_ascii_digit() {
            digits += 1;
        } else {
            break;
        }
    }
    if digits == 0 {
        return None;
    }
    let rest = &line[digits..];
    rest.strip_prefix(". ")
        .or_else(|| rest.strip_prefix(") "))
        .map(|s| s.to_string())
}

// ── Инлайн-разметка ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Span {
    Text(String),
    Bold(String),
    Italic(String),
    Code(String),
}

fn find_char(chars: &[char], from: usize, ch: char) -> Option<usize> {
    (from..chars.len()).find(|&i| chars[i] == ch)
}

fn find_double(chars: &[char], from: usize, ch: char) -> Option<usize> {
    let mut i = from;
    while i + 1 < chars.len() {
        if chars[i] == ch && chars[i + 1] == ch {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn parse_inline(input: &str) -> Vec<Span> {
    let chars: Vec<char> = input.chars().collect();
    let mut spans: Vec<Span> = Vec::new();
    let mut buf = String::new();
    let mut i = 0;

    fn flush(buf: &mut String, spans: &mut Vec<Span>) {
        if !buf.is_empty() {
            spans.push(Span::Text(std::mem::take(buf)));
        }
    }

    while i < chars.len() {
        let c = chars[i];
        // `code`
        if c == '`' {
            if let Some(end) = find_char(&chars, i + 1, '`') {
                flush(&mut buf, &mut spans);
                spans.push(Span::Code(chars[i + 1..end].iter().collect()));
                i = end + 1;
                continue;
            }
        }
        // **bold**
        if c == '*' && i + 1 < chars.len() && chars[i + 1] == '*' {
            if let Some(end) = find_double(&chars, i + 2, '*') {
                if end > i + 2 {
                    flush(&mut buf, &mut spans);
                    spans.push(Span::Bold(chars[i + 2..end].iter().collect()));
                    i = end + 2;
                    continue;
                }
            }
        }
        // *italic* (только звёздочка, чтобы не ломать snake_case)
        if c == '*' {
            if let Some(end) = find_char(&chars, i + 1, '*') {
                if end > i + 1 {
                    flush(&mut buf, &mut spans);
                    spans.push(Span::Italic(chars[i + 1..end].iter().collect()));
                    i = end + 1;
                    continue;
                }
            }
        }
        buf.push(c);
        i += 1;
    }
    flush(&mut buf, &mut spans);
    spans
}

fn render_inline(text: &str) -> impl IntoView {
    parse_inline(text)
        .into_iter()
        .map(|span| match span {
            Span::Text(t) => view! { <span>{t}</span> }.into_any(),
            Span::Bold(t) => view! { <strong>{t}</strong> }.into_any(),
            Span::Italic(t) => view! { <em>{t}</em> }.into_any(),
            Span::Code(t) => view! {
                <code style="background: var(--colorNeutralBackground3); padding: 0 4px; border-radius: 4px; font-family: var(--fontFamilyMonospace, monospace); font-size: 0.88em;">
                    {t}
                </code>
            }
            .into_any(),
        })
        .collect_view()
}

/// Отрисовать плоский список как вложенные `<ul>`/`<ol>`.
///
/// Вложенность именно разметкой, а не отступом у `<li>`: тогда нумерацию ведёт
/// браузер, у подсписка свой счётчик, и подпункты не съедают номера верхнего
/// уровня. `level` — глубина вложения, от неё зависит только вид маркера.
fn render_list(items: &[ListItem], level: usize) -> AnyView {
    let base = items.first().map_or(0, |item| item.depth);
    let ordered = items.first().map_or(false, |item| item.ordered);

    // Пункт + всё, что глубже него, до следующего пункта этого же уровня.
    let mut groups: Vec<(&ListItem, &[ListItem])> = Vec::new();
    let mut i = 0;
    while i < items.len() {
        let head = &items[i];
        i += 1;
        let children_from = i;
        while i < items.len() && items[i].depth > base {
            i += 1;
        }
        groups.push((head, &items[children_from..i]));
    }

    let body = groups
        .into_iter()
        .map(|(head, children)| {
            let nested = (!children.is_empty()).then(|| render_list(children, level + 1));
            view! {
                <li style="margin: 0.12em 0;">{render_inline(&head.text)}{nested}</li>
            }
        })
        .collect_view();

    if ordered {
        view! {
            <ol style=format!(
                "margin: 0.2em 0 0.2em 1.4em; padding: 0; list-style-type: {};",
                if level == 0 { "decimal" } else { "lower-alpha" },
            )>
                {body}
            </ol>
        }
        .into_any()
    } else {
        view! {
            <ul style=format!(
                "margin: 0.2em 0 0.2em 1.2em; padding: 0; list-style-type: {};",
                if level == 0 { "disc" } else { "circle" },
            )>
                {body}
            </ul>
        }
        .into_any()
    }
}

#[component]
#[allow(non_snake_case)]
pub fn Markdown(text: String) -> impl IntoView {
    let blocks = parse_blocks(&text);
    view! {
        <div class="md">
            {blocks.into_iter().map(|block| match block {
                Block::H1(t) => view! {
                    <div style="font-size: 1.25em; font-weight: 700; margin: 0.6em 0 0.25em;">{render_inline(&t)}</div>
                }.into_any(),
                Block::H2(t) => view! {
                    <div style="font-size: 1.12em; font-weight: 700; margin: 0.5em 0 0.2em;">{render_inline(&t)}</div>
                }.into_any(),
                Block::H3(t) => view! {
                    <div style="font-size: 1.0em; font-weight: 600; margin: 0.4em 0 0.15em;">{render_inline(&t)}</div>
                }.into_any(),
                Block::List(items) => render_list(&items, 0),
                Block::Code(lines) => view! {
                    <pre style="background: var(--colorNeutralBackground3); padding: 8px 10px; border-radius: 6px; font-family: var(--fontFamilyMonospace, monospace); font-size: 0.85em; overflow-x: auto; margin: 0.35em 0; white-space: pre-wrap; word-break: break-word;">
                        {lines.join("\n")}
                    </pre>
                }.into_any(),
                Block::Quote(lines) => view! {
                    <div style="border-left: 3px solid var(--colorNeutralStroke2); padding: 2px 10px; margin: 0.3em 0; color: var(--colorNeutralForeground2);">
                        {lines.into_iter().map(|l| view! { <div>{render_inline(&l)}</div> }).collect_view()}
                    </div>
                }.into_any(),
                Block::Table(rows) => {
                    let mut iter = rows.into_iter();
                    let header = iter.next();
                    view! {
                        <table style="border-collapse: collapse; margin: 0.4em 0; font-size: 0.92em;">
                            {header.map(|h| view! {
                                <thead>
                                    <tr>
                                        {h.into_iter().map(|c| view! {
                                            <th style="border: 1px solid var(--colorNeutralStroke2); padding: 4px 8px; text-align: left; background: var(--colorNeutralBackground2);">
                                                {render_inline(&c)}
                                            </th>
                                        }).collect_view()}
                                    </tr>
                                </thead>
                            })}
                            <tbody>
                                {iter.map(|row| view! {
                                    <tr>
                                        {row.into_iter().map(|c| view! {
                                            <td style="border: 1px solid var(--colorNeutralStroke2); padding: 4px 8px;">
                                                {render_inline(&c)}
                                            </td>
                                        }).collect_view()}
                                    </tr>
                                }).collect_view()}
                            </tbody>
                        </table>
                    }.into_any()
                }
                Block::Text(t) => view! {
                    <div style="margin: 0.1em 0; line-height: 1.5;">{render_inline(&t)}</div>
                }.into_any(),
                Block::Empty => view! { <div style="height: 0.4em;"></div> }.into_any(),
            }).collect_view()}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::{list_depth, parse_blocks, strip_ordered, Block, ListItem};

    fn list_of(text: &str) -> Vec<ListItem> {
        let Some(Block::List(items)) = parse_blocks(text)
            .into_iter()
            .find(|b| matches!(b, Block::List(_)))
        else {
            panic!("ожидался список");
        };
        items
    }

    fn tops(items: &[ListItem]) -> Vec<&str> {
        items
            .iter()
            .filter(|i| i.depth == 0)
            .map(|i| i.text.as_str())
            .collect()
    }

    /// Модели пишут нумерацию и точкой, и скобкой. Форма со скобкой раньше
    /// не распознавалась и выводилась плоским абзацем.
    #[test]
    fn ordered_items_accept_dot_and_paren() {
        assert_eq!(strip_ordered("1. текст").as_deref(), Some("текст"));
        assert_eq!(strip_ordered("12) текст").as_deref(), Some("текст"));
        assert_eq!(strip_ordered("нет"), None);
        // Без пробела после маркера это не список: «1.5 млн» должно остаться текстом.
        assert_eq!(strip_ordered("1.5 млн"), None);
    }

    #[test]
    fn depth_comes_from_indent_and_is_capped() {
        assert_eq!(list_depth("- пункт"), 0);
        assert_eq!(list_depth("  - подпункт"), 1);
        assert_eq!(list_depth("    - третий"), 2);
        // Глубже двух уровней список нечитаем — упираемся в потолок.
        assert_eq!(list_depth("            - очень глубоко"), 2);
    }

    #[test]
    fn nested_items_keep_their_level() {
        let items = list_of("- верх\n  - вложенный\n- снова верх\n");
        assert_eq!(
            items
                .iter()
                .map(|i| (i.depth, i.text.as_str()))
                .collect::<Vec<_>>(),
            vec![(0, "верх"), (1, "вложенный"), (0, "снова верх")]
        );
        assert!(items.iter().all(|i| !i.ordered));
    }

    #[test]
    fn ordered_nesting_survives_paren_form() {
        let items = list_of("1) первый\n   2) вложенный\n");
        assert_eq!(items[0].depth, 0);
        assert_eq!(items[0].text, "первый");
        assert_eq!(items[1].depth, 1);
    }

    /// Регрессия: пункты, разделённые пустой строкой, попадали в разные `<ol>`,
    /// и каждый нумеровался с единицы — «1. … 1. …» вместо «1. … 2. …».
    #[test]
    fn blank_line_between_items_does_not_restart_numbering() {
        let items = list_of("1. первый\n\n2. второй\n\n3. третий\n");
        assert_eq!(tops(&items), vec!["первый", "второй", "третий"]);
        assert!(items.iter().all(|i| i.ordered));
    }

    /// Та же регрессия: вложенные маркеры под нумерованным пунктом обрывали
    /// список, и следующий номер снова становился первым.
    #[test]
    fn nested_bullets_stay_inside_the_ordered_list() {
        let items = list_of("1. выбери период\n  - минимально\n  - вся история\n\n2. затем план\n");
        assert_eq!(tops(&items), vec!["выбери период", "затем план"]);
        // Подпункты остались в том же блоке — иначе счётчик обнулился бы.
        assert_eq!(items.len(), 4);
        assert!(items[1].depth == 1 && !items[1].ordered);
    }

    /// Пустая строка после списка всё ещё отбивает следующий абзац.
    #[test]
    fn blank_line_after_list_is_kept_as_separator() {
        let blocks = parse_blocks("- пункт\n\nабзац\n");
        assert!(matches!(blocks[0], Block::List(_)));
        assert!(matches!(blocks[1], Block::Empty));
        assert!(matches!(blocks[2], Block::Text(_)));
    }

    /// Смена вида маркера на верхнем уровне — это новый список, а не продолжение.
    #[test]
    fn marker_change_starts_a_new_list() {
        let blocks = parse_blocks("- маркированный\n1. нумерованный\n");
        let lists: Vec<_> = blocks
            .iter()
            .filter(|b| matches!(b, Block::List(_)))
            .collect();
        assert_eq!(lists.len(), 2);
    }
}
