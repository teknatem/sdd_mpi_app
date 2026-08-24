//! Инструменты LLM для работы с базой знаний: поиск, чтение, создание черновика
//! статьи из диалога и подача замечаний.
//!
//! `search_knowledge` и `get_knowledge` жили в `tool_executor.rs`; перенесены сюда,
//! чтобы определение и исполнение лежали рядом. Имена не менялись — все навыки,
//! которые их перечисляют, продолжают резолвиться.

use super::kb_metrics::{self, ArticleRef};
use super::knowledge_base::{self, kb_read, DocKind, KbStatus};
use super::types::ToolDefinition;
use serde_json::{json, Value};

pub const KB_TOOL_NAMES: &[&str] = &[
    "search_knowledge",
    "get_knowledge",
    "kb_propose_article",
    "kb_report_issue",
    "list_kb_vocabulary",
];

/// Порог, после которого замечания к статье эскалируются в тикет a031_kb_edit.
const ISSUE_ESCALATION_THRESHOLD: i64 = 3;

/// Минимальная длина тела статьи — ниже это заметка, а не знание.
const MIN_BODY_CHARS: usize = 400;
/// Выше этого предлагаем разбить статью на части (но всё равно записываем).
pub(super) const LARGE_ARTICLE_TOKENS: u32 = 6000;

/// Выше этого `get_knowledge` отдаёт ОГЛАВЛЕНИЕ, а не тело статьи.
///
/// Калибровка по корпусу на 2026-08-23: у 48 статей `app` средняя цена чтения
/// 1 316 токенов при максимуме 4 483, у карт `generated` — 1 294 при максимуме
/// 2 992. Порог в 2 000 задевает единицы самых крупных документов и не трогает
/// большинство: дробить статью, которая дешевле одного ответа модели, — лишний
/// раунд без выигрыша.
const SECTIONED_READ_TOKENS: u32 = 2000;

/// Сколько вводки отдавать вместе с оглавлением: её задача — дать понять, о чём
/// статья, чтобы модель выбрала раздел, а не заменить собой чтение.
const INTRO_CHARS: usize = 900;

pub fn kb_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "search_knowledge".into(),
            description: "Поиск по базе знаний организации. ОСНОВНОЙ способ — свободный текст \
                          в `query`: пиши вопрос пользователя своими словами, русский язык \
                          поддерживается. Ищет по заголовкам, тегам и ПОЛНОМУ ТЕКСТУ статей, \
                          затем добирает связанные статьи по графу. Возвращает `summary` и \
                          `token_cost` каждой статьи — читай через get_knowledge только те, \
                          что действительно нужны для ответа. Второй способ — `entities`: \
                          собрать всё, что привязано к объекту системы (a012, p904), без текста."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Свободный текст вопроса. Например: 'почему выкуп в текущем месяце ниже' \
                                        или 'как считается ДРР'."
                    },
                    "tags": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Необязательное уточнение тегами (нормализуются по словарю: 'вб' → 'wildberries'). \
                                        Тег повышает статью в выдаче, но НЕ отсекает остальные."
                    },
                    "entities": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Коды объектов системы ('a012', 'p904', принимается и 'a012_wb_sales'). \
                                        В отличие от тегов — ЖЁСТКИЙ фильтр: вернётся только привязанное \
                                        к этим объектам. Работает и без query."
                    },
                    "corpus": {
                        "type": "string",
                        "enum": ["default", "business", "app", "generated", "all"],
                        "default": "default",
                        "description": "Где искать. 'default' — знание об организации + техдоки приложения. \
                                        'generated' — карты, собранные из БД и рантайма (профиль таблиц, \
                                        плагины, навыки, проверки); в обычную выдачу они не попадают."
                    },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 10, "default": 5 },
                    "include_drafts": {
                        "type": "boolean",
                        "default": true,
                        "description": "Включать черновики (status=draft). Они помечены в выдаче и понижены."
                    }
                }
            }),
        },
        ToolDefinition {
            name: "get_knowledge".into(),
            description: "Получить содержимое статьи базы знаний по id. \
                          id берётся из результата search_knowledge. \
                          Небольшая статья приходит целиком. КРУПНАЯ (в выдаче поиска \
                          помечена `sectioned: true`) приходит ОГЛАВЛЕНИЕМ: выбери нужный \
                          раздел и позови ещё раз с `section` — так ты не платишь контекстом \
                          за то, что не читаешь. Целиком крупную статью берут, только когда \
                          нужны все разделы сразу: `full: true`. \
                          Использовал статью в ответе — сошлись на неё строкой kb://article/<id>."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Идентификатор статьи из search_knowledge, например 'wb-promotions'."
                    },
                    "section": {
                        "type": "string",
                        "description": "Раздел статьи: номер из оглавления ('3') или часть его заголовка \
                                        ('расчёт ДРР'). Само оглавление приходит в ответе без этого параметра."
                    },
                    "full": {
                        "type": "boolean",
                        "default": false,
                        "description": "Отдать крупную статью целиком, минуя оглавление. Ставь, только когда \
                                        нужны все разделы: это самый дорогой вызов инструмента."
                    }
                },
                "required": ["id"]
            }),
        },
        ToolDefinition {
            name: "kb_propose_article".into(),
            description: "Оформить устойчивое новое знание из диалога в статью базы знаний. \
                          Статья создаётся ЧЕРНОВИКОМ (status=draft) и одновременно заводится \
                          тикет a031_kb_edit на проверку человеком. \
                          Пиши ТОЛЬКО бизнес-знание: что означает показатель, какое правило \
                          действует, что с этим делать. ЗАПРЕЩЕНО: SQL, схемы таблиц, имена \
                          полей, пути API — это техническая документация, ей здесь не место. \
                          Проверка: если фраза перестанет быть верной после рефакторинга схемы — \
                          её нельзя писать в базу знаний."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "Заголовок статьи." },
                    "slug": {
                        "type": "string",
                        "description": "Имя файла: латиница/цифры/дефис, без .md. Не задан — выводится из title."
                    },
                    "summary": {
                        "type": "string",
                        "description": "ОДНО предложение (до 300 символов): что даёт статья. Показывается в результатах поиска."
                    },
                    "tags": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Минимум 2 тега. Возьми канонические из list_kb_vocabulary."
                    },
                    "related": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Связи для графа: id существующих статей, канонические теги или коды агрегатов (a012, p916)."
                    },
                    "body": {
                        "type": "string",
                        "description": "Markdown БЕЗ frontmatter, начинается с '# <заголовок>'. Минимум 400 символов."
                    },
                    "stars": {
                        "type": "integer", "minimum": 1, "maximum": 5,
                        "description": "Важность знания: 5 — несущее правило, 2 — черновая заметка."
                    },
                    "ttl_days": {
                        "type": "integer",
                        "description": "Срок годности знания: 90 для правил маркетплейсов, 365 для определений."
                    },
                    "replaces": {
                        "type": "string",
                        "description": "id существующей статьи, если это её замена. Без него перезапись запрещена."
                    }
                },
                "required": ["title", "summary", "tags", "body"]
            }),
        },
        ToolDefinition {
            name: "kb_report_issue".into(),
            description: "Сообщить о проблеме в статье базы знаний: противоречит данным, \
                          устарела, неполна или непонятна. Вызывай, когда сверил статью с \
                          реальными данными и увидел расхождение. Замечания копятся по статье \
                          и попадают к куратору."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "article_id": { "type": "string", "description": "id статьи из search_knowledge / get_knowledge." },
                    "issue_kind": {
                        "type": "string",
                        "enum": ["inaccuracy", "outdated", "contradiction", "gap", "unclear"],
                        "description": "inaccuracy — неверно; outdated — устарело; contradiction — противоречит другой статье или данным; gap — не хватает знания; unclear — непонятно сформулировано."
                    },
                    "severity": { "type": "string", "enum": ["low", "normal", "high"], "default": "normal" },
                    "quote": {
                        "type": "string",
                        "description": "Точная цитата из статьи (до 300 символов), к которой относится замечание."
                    },
                    "body": {
                        "type": "string",
                        "description": "Что именно не так и чем это подтверждается: данные, расчёт, слова пользователя."
                    }
                },
                "required": ["article_id", "issue_kind", "body"]
            }),
        },
        ToolDefinition {
            name: "list_kb_vocabulary".into(),
            description: "Словарь тегов базы знаний: канонические теги по группам, их синонимы \
                          и число статей. Вызывай перед kb_propose_article, чтобы взять теги \
                          из словаря, а не выдумать свои."
                .into(),
            parameters: json!({ "type": "object", "properties": {} }),
        },
    ]
}

pub async fn execute_kb_tool(name: &str, arguments: &str, chat_id: &str, agent_id: &str) -> Value {
    let args = serde_json::from_str::<Value>(arguments).unwrap_or_else(|_| json!({}));
    match name {
        "search_knowledge" => search_knowledge(&args),
        "get_knowledge" => get_knowledge(&args),
        "list_kb_vocabulary" => list_vocabulary(),
        "kb_propose_article" => propose_article(&args, chat_id, agent_id).await,
        "kb_report_issue" => report_issue(&args, chat_id, agent_id).await,
        _ => json!({ "error": format!("Unknown KB tool: {}", name) }),
    }
}

// ─── Поиск ───────────────────────────────────────────────────────────────────

fn search_knowledge(args: &Value) -> Value {
    let query_text = args
        .get("query")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let raw_tags = string_array(args, "tags");
    let raw_entities = string_array(args, "entities");

    if query_text.trim().is_empty() && raw_tags.is_empty() && raw_entities.is_empty() {
        return json!({
            "error": "Нужен 'query' (свободный текст вопроса), 'tags' или 'entities'. Предпочитай query."
        });
    }

    // Якорь принимается под любым именем объекта — приводим к коду реестра.
    // Неизвестное имя оставляем как есть: пусть выдача будет пустой, а не тихо
    // расширенной до всей базы.
    let entities: Vec<String> = raw_entities
        .iter()
        .map(|raw| {
            super::knowledge_base::canonical_anchor(raw).unwrap_or_else(|| raw.trim().to_string())
        })
        .collect();

    let kb = kb_read();
    let tag_refs: Vec<&str> = raw_tags.iter().map(|s| s.as_str()).collect();
    let query = super::kb_search::Query {
        text: query_text.clone(),
        tags: kb.normalize_tags(&tag_refs),
        entities,
        kinds: parse_corpus(args.get("corpus").and_then(|v| v.as_str())),
        limit: args
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(super::kb_search::DEFAULT_LIMIT as u64) as usize,
        include_drafts: args
            .get("include_drafts")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        include_deprecated: false,
    };

    let (hits, total_matched) = kb.search_with_total(&query);

    if hits.is_empty() {
        let probe = if query_text.trim().is_empty() {
            raw_tags.join(" ")
        } else {
            query_text
        };
        return json!({
            "results": [],
            "returned": 0,
            "total_matched": 0,
            "suggested_tags": kb.suggest_tags(&probe, 12),
            "hint": "В базе знаний ничего не найдено. НЕ выдумывай содержание статей и их id — \
                     скажи пользователю прямо, что знания в базе нет, ответь из данных системы и \
                     пометь ответ как неподтверждённый. Если знание устойчиво — предложи оформить \
                     статью через kb_propose_article."
        });
    }

    // Статистика: считаем только ВОЗВРАЩЁННЫЕ статьи, не кандидатов.
    let refs: Vec<ArticleRef> = hits.iter().map(|(doc, _)| ArticleRef::from(*doc)).collect();
    kb_metrics::record_search(&refs);

    let results: Vec<Value> = hits
        .iter()
        .map(|(doc, via)| {
            let mut item = json!({
                "id": doc.id,
                "title": doc.title,
                "summary": doc.summary,
                "tags": doc.canonical_tags,
                "status": doc.status.as_str(),
                "token_cost": doc.token_cost,
                "corpus": doc.kind.as_str(),
            });
            if !doc.anchors.is_empty() {
                item["entities"] = json!(doc.anchors);
            }
            // Предупреждение о четвёртом уровне progressive disclosure: у крупной
            // статьи get_knowledge вернёт оглавление, а не тело. Один булев флаг
            // дешевле, чем выяснение этого лишним вызовом инструмента.
            if doc.token_cost > SECTIONED_READ_TOKENS
                && knowledge_base::outline(&doc.content).len() >= 2
            {
                item["sectioned"] = json!(true);
            }
            if let Some(stars) = doc.stars {
                item["stars"] = json!(stars);
            }
            if let Some(updated) = doc.updated {
                item["updated"] = json!(updated.to_string());
            }
            if let Some(via) = via {
                item["via"] = json!(via);
            }
            if doc.status == KbStatus::Draft {
                item["note"] = json!("Черновик: не проверен человеком, полагайся осторожно.");
            }
            item
        })
        .collect();

    json!({
        "results": results,
        "returned": results.len(),
        "total_matched": total_matched,
        "hint": "Читай через get_knowledge(id) только нужные статьи — ориентируйся на summary и \
                 token_cost. У статьи с `sectioned: true` get_knowledge отдаст оглавление: \
                 возьми оттуда раздел, а не всю статью. Использовал статью в ответе — сошлись \
                 строкой kb://article/<id>."
    })
}

/// `corpus` из аргументов инструмента → список корпусов поиска.
fn parse_corpus(raw: Option<&str>) -> Vec<DocKind> {
    match raw.unwrap_or("default").trim().to_lowercase().as_str() {
        "business" => vec![DocKind::Business],
        "app" => vec![DocKind::App],
        "generated" => vec![DocKind::Generated],
        "all" => vec![DocKind::Business, DocKind::App, DocKind::Generated],
        // Сгенерированные карты в выдачу по умолчанию не попадают: их десятки,
        // они переписываются машиной и утопили бы курируемые статьи.
        _ => vec![DocKind::Business, DocKind::App],
    }
}

/// Оглавление статьи в компактном виде: номер, заголовок, цена раздела.
fn outline_json(sections: &[knowledge_base::DocSection]) -> Vec<Value> {
    sections
        .iter()
        .map(|s| json!({ "n": s.number, "title": s.title, "token_cost": s.token_cost }))
        .collect()
}

/// Раздел по номеру из оглавления или по части заголовка.
///
/// Оба способа нужны: номер модель берёт из выдачи без ошибок, а заголовок
/// переживает переписывание статьи, после которого номера съезжают.
fn resolve_section<'a>(
    sections: &'a [knowledge_base::DocSection],
    raw: &str,
) -> Option<&'a knowledge_base::DocSection> {
    if let Ok(number) = raw.parse::<usize>() {
        return sections.iter().find(|s| s.number == number);
    }
    let needle = raw.to_lowercase();
    sections
        .iter()
        .find(|s| s.title.to_lowercase() == needle)
        .or_else(|| {
            sections
                .iter()
                .find(|s| s.title.to_lowercase().contains(&needle))
        })
}

/// Обрезка по символам, а не байтам: в кириллице второе режет посреди буквы.
fn truncate_chars(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut out: String = text.chars().take(limit).collect();
    out.push('…');
    out
}

/// Чтение статьи с тремя режимами выдачи: `full`, `outline`, `section`.
///
/// Раньше режим был один — тело целиком, и статья на 4 483 токена приходила
/// неделимой, даже когда из неё нужен один раздел. Теперь крупная статья по
/// умолчанию отвечает оглавлением, а тело выдаётся по разделу; поведение мелких
/// статей (а это большинство корпуса) не изменилось.
fn get_knowledge(args: &Value) -> Value {
    let id = args.get("id").and_then(|v| v.as_str()).unwrap_or_default();
    let requested_section = args
        .get("section")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let want_full = args.get("full").and_then(|v| v.as_bool()).unwrap_or(false);
    let kb = kb_read();

    let Some(doc) = kb.get(id) else {
        // Раньше на промахе выдавался список ВСЕХ id базы — это съедало контекст.
        let suggestions: Vec<Value> = kb
            .did_you_mean(id, 5)
            .into_iter()
            .map(|d| json!({ "id": d.id, "title": d.title }))
            .collect();
        return json!({
            "error": format!("Статья '{}' не найдена.", id),
            "did_you_mean": suggestions,
            "hint": "Найди статью через search_knowledge(query=\"...\") — id придёт в результате."
        });
    };

    let sections = knowledge_base::outline(&doc.content);

    let related_articles: Vec<Value> = doc
        .related
        .iter()
        .filter_map(|r| kb.get(r))
        .map(|d| json!({ "id": d.id, "title": d.title }))
        .collect();

    let mut result = json!({
        "id": doc.id,
        "title": doc.title,
        "summary": doc.summary,
        "tags": doc.canonical_tags,
        "status": doc.status.as_str(),
        "token_cost": doc.token_cost,
        "related_articles": related_articles,
        "source_path": doc.source_path,
    });
    if let Some(stars) = doc.stars {
        result["stars"] = json!(stars);
    }
    if let Some(updated) = doc.updated {
        result["updated"] = json!(updated.to_string());
    }
    if let Some(pct) = doc.staleness_pct() {
        if pct > 100 {
            result["stale"] = json!(true);
            result["staleness_pct"] = json!(pct);
        }
    }

    let banner = if doc.status == KbStatus::Deprecated {
        "> ⚠️ Статья помечена как УСТАРЕВШАЯ. Не опирайся на неё без проверки.\n\n"
    } else {
        ""
    };
    let source_line = format!("\n\nИсточник: kb://article/{}", doc.id);

    // Запрошен конкретный раздел.
    if let Some(raw) = requested_section {
        let Some(section) = resolve_section(&sections, raw) else {
            return json!({
                "error": format!("Раздел '{}' в статье '{}' не найден.", raw, doc.id),
                "sections": outline_json(&sections),
                "hint": "Возьми номер или заголовок из `sections`. Нужна вся статья — full=true."
            });
        };
        let content = format!(
            "{}{}{}",
            banner,
            knowledge_base::section_text(&doc.content, section),
            source_line
        );
        // Раздел прочитан — статья считается прочитанной, а `token_cost_last`
        // остаётся ценой статьи целиком: это её свойство, а не цена одного вызова.
        // Фактически доставленное меряет `sys_tool_trace.tokens` (миграция 0224).
        kb_metrics::record_read(&ArticleRef::from(doc));
        result["mode"] = json!("section");
        result["section"] = json!({ "n": section.number, "title": section.title });
        result["sections"] = json!(outline_json(&sections));
        result["delivered_tokens"] = json!(knowledge_base::estimate_tokens(&content));
        result["content"] = json!(content);
        result["hint"] = json!(format!(
            "Раздел {} из {}. Другой раздел — section=\"<номер|заголовок>\", вся статья — full=true.",
            section.number,
            sections.len()
        ));
        return result;
    }

    // Крупная статья без явного запроса — оглавление вместо тела.
    if !want_full && sections.len() >= 2 && doc.token_cost > SECTIONED_READ_TOKENS {
        let intro = format!(
            "{}{}",
            banner,
            truncate_chars(
                knowledge_base::intro_text(&doc.content, &sections),
                INTRO_CHARS
            )
        );
        // Чтение НЕ засчитывается: тела статья не отдала. Иначе `read_hits`
        // перестанет отличать «модель заплатила за текст» от «увидела оглавление»,
        // а на этой разнице стоит вся метрика полезности статьи.
        result["mode"] = json!("outline");
        result["sections"] = json!(outline_json(&sections));
        result["delivered_tokens"] = json!(knowledge_base::estimate_tokens(&intro));
        result["intro"] = json!(intro);
        result["hint"] = json!(format!(
            "Статья крупная ({} токенов) — отдано оглавление. Возьми нужный раздел: \
             get_knowledge(id=\"{}\", section=\"<номер|заголовок>\"). Нужна целиком — full=true. \
             Использовал в ответе — сошлись строкой kb://article/{}.",
            doc.token_cost, doc.id, doc.id
        ));
        return result;
    }

    let content = format!("{}{}{}", banner, doc.content, source_line);
    kb_metrics::record_read(&ArticleRef::from(doc));
    result["mode"] = json!("full");
    result["delivered_tokens"] = json!(knowledge_base::estimate_tokens(&content));
    result["content"] = json!(content);
    result
}

fn list_vocabulary() -> Value {
    let kb = kb_read();
    let mut groups: std::collections::BTreeMap<String, Vec<Value>> = Default::default();
    for term in kb.vocabulary().terms() {
        let group = if term.group.is_empty() {
            "прочее".to_string()
        } else {
            term.group.clone()
        };
        groups.entry(group).or_default().push(json!({
            "tag": term.tag,
            "label": term.label,
            "aliases": term.aliases,
            "description": term.description,
            "articles": kb.tag_count(&term.tag),
        }));
    }

    let mut unknown: Vec<String> = kb
        .all_docs()
        .iter()
        .flat_map(|d| d.unknown_tags.clone())
        .collect();
    unknown.sort();
    unknown.dedup();

    json!({
        "groups": groups,
        "total_terms": kb.vocabulary().len(),
        "tags_outside_vocabulary": unknown,
        "hint": "Для новой статьи бери теги из этого словаря. Тега нет — можно ввести свой, \
                 он попадёт в список на согласование куратору."
    })
}

// ─── Замечания ───────────────────────────────────────────────────────────────

async fn report_issue(args: &Value, chat_id: &str, agent_id: &str) -> Value {
    let article_id = args
        .get("article_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let body = args
        .get("body")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    if body.trim().is_empty() {
        return json!({ "error": "body обязателен: опиши, что именно не так и чем это подтверждается." });
    }

    let (article, doc_title) = {
        let kb = kb_read();
        let Some(doc) = kb.get(article_id) else {
            return json!({
                "error": format!("Статья '{}' не найдена.", article_id),
                "did_you_mean": kb.did_you_mean(article_id, 5)
                    .into_iter().map(|d| json!({"id": d.id, "title": d.title})).collect::<Vec<_>>(),
            });
        };
        (ArticleRef::from(doc), doc.title.clone())
    };

    let severity = args
        .get("severity")
        .and_then(|v| v.as_str())
        .unwrap_or("normal")
        .to_string();
    let issue_kind = args
        .get("issue_kind")
        .and_then(|v| v.as_str())
        .unwrap_or("inaccuracy")
        .to_string();
    let quote: String = args
        .get("quote")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .chars()
        .take(300)
        .collect();

    let outcome = match kb_metrics::record_issue(kb_metrics::NewIssue {
        article: article.clone(),
        issue_kind: issue_kind.clone(),
        severity: severity.clone(),
        body: body.to_string(),
        quote: quote.clone(),
        chat_id: (!chat_id.is_empty()).then(|| chat_id.to_string()),
        agent_id: (!agent_id.is_empty()).then(|| agent_id.to_string()),
        reported_by: "agent".into(),
    })
    .await
    {
        Ok(outcome) => outcome,
        Err(error) => return json!({ "error": format!("Замечание не записано: {}", error) }),
    };

    let mut result = json!({
        "ok": true,
        "issue_id": outcome.issue_id,
        "open_issues": outcome.open_issues,
    });

    // Серьёзное или накопившееся замечание должно попасть в человеческую очередь
    // ревью, а не остаться строкой в таблице.
    if severity == "high" || outcome.open_issues >= ISSUE_ESCALATION_THRESHOLD {
        let summary = format!(
            "Замечание к статье «{}» (kb://article/{}).\n\nТип: {}\nВажность: {}\n\n{}{}\n\n\
             Открытых замечаний к статье: {}.",
            doc_title,
            article.doc_id,
            issue_kind,
            severity,
            if quote.is_empty() {
                String::new()
            } else {
                format!("Цитата: «{}»\n\n", quote)
            },
            body,
            outcome.open_issues
        );
        match create_kb_edit(
            format!("Замечание к статье: {}", doc_title),
            summary,
            contracts::domain::a031_kb_edit::aggregate::KbEditType::Contradiction,
            vec![article.doc_id.clone()],
            chat_id,
            agent_id,
        )
        .await
        {
            Ok(kb_edit_id) => {
                kb_metrics::link_issue_to_kb_edit(&outcome.issue_id, &kb_edit_id).await;
                result["kb_edit_id"] = json!(kb_edit_id);
            }
            Err(error) => {
                result["kb_edit_error"] = json!(error.to_string());
            }
        }
    }

    result["hint"] =
        json!("Замечание записано. Сообщи пользователю, что статья требует уточнения.");
    result
}

// ─── Чат → статья ────────────────────────────────────────────────────────────

/// Результат валидации черновика: что записывать и о чём предупредить.
#[derive(Debug, Default)]
pub struct DraftPlan {
    pub slug: String,
    pub doc_id: String,
    pub uid: String,
    pub content: String,
    pub tags: Vec<String>,
    pub related: Vec<String>,
    pub unknown_tags: Vec<String>,
    pub warnings: Vec<String>,
    pub token_cost: u32,
}

/// Всё, что нужно валидатору из базы знаний — чтобы функция осталась чистой
/// и тестируемой нативно.
pub struct DraftContext {
    pub existing_ids: Vec<String>,
    pub embedded_ids: Vec<String>,
    pub known_tags: Vec<String>,
    pub today: chrono::NaiveDate,
    pub chat_id: String,
}

/// Проверить черновик ДО записи файла.
///
/// `write_kb_document` не валидирует ничего и молча перезаписывает — сюда
/// вынесены все проверки, включая границу «бизнес-знание, а не техдокументация»,
/// которая до сих пор существовала только прозой в промпте навыка.
pub fn validate_draft(args: &Value, ctx: &DraftContext) -> Result<DraftPlan, String> {
    let title = args
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .trim()
        .to_string();
    if title.is_empty() {
        return Err("title обязателен.".into());
    }

    let body = args
        .get("body")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .trim()
        .to_string();
    let summary_raw = args
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .trim()
        .to_string();
    let replaces = args.get("replaces").and_then(|v| v.as_str()).unwrap_or("");

    // 1. slug и путь.
    let slug = match args.get("slug").and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() => normalize_slug(s),
        _ => normalize_slug(&transliterate(&title)),
    };
    if !is_valid_slug(&slug) {
        return Err(format!(
            "Не удалось получить корректное имя файла из заголовка (вышло '{}'). \
             Передай slug явно: латиница, цифры и дефисы, 3–64 символа.",
            slug
        ));
    }

    // 2. Коллизии.
    if ctx.embedded_ids.iter().any(|id| id == &slug) {
        return Err(format!(
            "id '{}' занят встроенной технической документацией приложения. Выбери другой slug.",
            slug
        ));
    }
    if ctx.existing_ids.iter().any(|id| id == &slug) && replaces != slug {
        return Err(format!(
            "Статья '{}' уже существует. Молчаливая перезапись запрещена: либо выбери другой slug, \
             либо передай replaces=\"{}\", если это осознанная замена.",
            slug, slug
        ));
    }

    // 3. Тело.
    if body.starts_with("---") {
        return Err(
            "body не должен содержать frontmatter — он формируется автоматически. \
                    Начни с '# <заголовок>'."
                .into(),
        );
    }
    if !body.starts_with("# ") {
        return Err("body должен начинаться с заголовка первого уровня: '# <заголовок>'.".into());
    }
    if body.chars().count() < MIN_BODY_CHARS {
        return Err(format!(
            "Статья слишком короткая ({} символов, нужно от {}). Это заметка, а не знание: \
             добавь контекст, следствия и что с этим делать.",
            body.chars().count(),
            MIN_BODY_CHARS
        ));
    }

    // 4. Граница «бизнес-знание, а не техдокументация».
    if let Some(found) = detect_technical_leak(&body, &ctx.embedded_ids) {
        return Err(format!(
            "В тексте техническая деталь ({}). База знаний хранит бизнес-знание: что означает \
             показатель и что с ним делать. SQL, схемы таблиц, имена полей и пути API живут в \
             техдокументации. Проверка: если фраза перестанет быть верной после рефакторинга \
             схемы — ей здесь не место.",
            found
        ));
    }

    // 5. summary.
    if summary_raw.is_empty() {
        return Err("summary обязателен: одно предложение о том, что даёт статья.".into());
    }
    if summary_raw.contains('\n') {
        return Err("summary должен быть одной строкой.".into());
    }
    if summary_raw.chars().count() > 300 {
        return Err("summary длиннее 300 символов — сократи до одного предложения.".into());
    }

    // 6. Теги: нормализуем, неизвестные не отклоняем.
    let raw_tags = string_array(args, "tags");
    let mut tags: Vec<String> = Vec::new();
    let mut unknown_tags: Vec<String> = Vec::new();
    for tag in &raw_tags {
        let normalized = super::kb_vocabulary::normalize_form(tag);
        if normalized.is_empty() || tags.contains(&normalized) {
            continue;
        }
        if !ctx.known_tags.contains(&normalized) {
            unknown_tags.push(normalized.clone());
        }
        tags.push(normalized);
    }
    if tags.len() < 2 {
        return Err("Нужно минимум 2 тега. Возьми канонические из list_kb_vocabulary.".into());
    }

    // 7. related: оставляем только резолвимое.
    let mut warnings: Vec<String> = Vec::new();
    let mut related: Vec<String> = Vec::new();
    for entry in string_array(args, "related") {
        let normalized = super::kb_vocabulary::normalize_form(&entry);
        if normalized.is_empty() || related.contains(&normalized) {
            continue;
        }
        let resolvable = ctx.existing_ids.contains(&normalized)
            || ctx.embedded_ids.contains(&normalized)
            || ctx.known_tags.contains(&normalized)
            || is_aggregate_code(&normalized);
        if resolvable {
            related.push(normalized);
        } else {
            warnings.push(format!(
                "связь '{}' ни к чему не привязалась и отброшена",
                entry
            ));
        }
    }

    if !unknown_tags.is_empty() {
        warnings.push(format!(
            "теги вне словаря: {} — куратор согласует их при проверке",
            unknown_tags.join(", ")
        ));
    }

    let token_cost = knowledge_base::estimate_tokens(&body);
    if token_cost > LARGE_ARTICLE_TOKENS {
        warnings.push(format!(
            "статья крупная (~{} токенов) — стоит разбить на части",
            token_cost
        ));
    }

    let stars = args
        .get("stars")
        .and_then(|v| v.as_u64())
        .unwrap_or(3)
        .clamp(1, 5);
    let ttl_days = args
        .get("ttl_days")
        .and_then(|v| v.as_u64())
        .unwrap_or(knowledge_base::DEFAULT_TTL_DAYS as u64);
    let uid = format!("kb-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);

    // `verified:` намеренно пустой — черновик никем не верифицирован.
    let content = format!(
        "---\ntitle: {title}\nuid: {uid}\nstatus: draft\ntags: [{tags}]\nrelated: [{related}]\n\
         aliases: []\nsummary: {summary}\nstars: {stars}\nttl_days: {ttl}\nupdated: {today}\n\
         verified:\nsource_chats: [{chat}]\nauthor: llm\n---\n\n{body}\n",
        title = title,
        uid = uid,
        tags = tags.join(", "),
        related = related.join(", "),
        summary = summary_raw,
        stars = stars,
        ttl = ttl_days,
        today = ctx.today,
        chat = ctx.chat_id,
        body = body,
    );

    Ok(DraftPlan {
        doc_id: slug.clone(),
        slug,
        uid,
        content,
        tags,
        related,
        unknown_tags,
        warnings,
        token_cost,
    })
}

async fn propose_article(args: &Value, chat_id: &str, agent_id: &str) -> Value {
    let ctx = {
        let kb = kb_read();
        let (existing_ids, embedded_ids) =
            kb.all_docs()
                .iter()
                .fold((Vec::new(), Vec::new()), |mut acc, doc| {
                    if doc.is_embedded() {
                        acc.1.push(doc.id.clone());
                    } else {
                        acc.0.push(doc.id.clone());
                    }
                    acc
                });
        DraftContext {
            existing_ids,
            embedded_ids,
            known_tags: kb.vocabulary().terms().map(|t| t.tag.clone()).collect(),
            today: chrono::Utc::now().date_naive(),
            chat_id: chat_id.to_string(),
        }
    };

    let plan = match validate_draft(args, &ctx) {
        Ok(plan) => plan,
        Err(error) => return json!({ "error": error }),
    };

    let path = format!("{}.md", plan.slug);
    let written = match knowledge_base::write_kb_document(&path, &plan.content) {
        Ok(path) => path,
        Err(error) => return json!({ "error": format!("Файл не записан: {}", error) }),
    };
    if let Err(error) = knowledge_base::reload_knowledge_base() {
        tracing::warn!("[kb_tools] база знаний не перечитана после записи: {error}");
    }

    let title = args
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let mut result = json!({
        "ok": true,
        "id": plan.doc_id,
        "uid": plan.uid,
        "path": written.display().to_string(),
        "token_cost": plan.token_cost,
        "unknown_tags": plan.unknown_tags,
        "warnings": plan.warnings,
    });

    // Тикет заводим ПОСЛЕ записи файла: если он не создастся, знание уже сохранено,
    // и терять его из-за проблем с очередью ревью нельзя.
    let ticket_summary = format!(
        "Черновик статьи из диалога: kb://article/{}\n\nФайл: {}\nТеги: {}\n\n{}{}",
        plan.doc_id,
        path,
        plan.tags.join(", "),
        args.get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or_default(),
        if plan.warnings.is_empty() {
            String::new()
        } else {
            format!("\n\nПри проверке учесть:\n- {}", plan.warnings.join("\n- "))
        }
    );
    match create_kb_edit(
        format!("Черновик статьи: {}", title),
        ticket_summary,
        contracts::domain::a031_kb_edit::aggregate::KbEditType::Proposal,
        vec![plan.doc_id.clone()],
        chat_id,
        agent_id,
    )
    .await
    {
        Ok(id) => result["kb_edit_id"] = json!(id),
        Err(error) => {
            result["kb_edit_error"] = json!(error.to_string());
            result["hint_ticket"] =
                json!("Тикет на проверку не создан — сообщи пользователю, что статью нужно завести на ревью вручную.");
        }
    }

    result["hint"] = json!(format!(
        "Черновик создан. Сошлись на него в ответе строкой kb://article/{} — пользователь получит \
         ссылку. Статья станет полноценной после проверки человеком.",
        plan.doc_id
    ));
    result
}

async fn create_kb_edit(
    title: String,
    summary: String,
    edit_type: contracts::domain::a031_kb_edit::aggregate::KbEditType,
    target_articles: Vec<String>,
    chat_id: &str,
    agent_id: &str,
) -> anyhow::Result<String> {
    let agent_uuid = uuid::Uuid::parse_str(agent_id)?;
    let source_chats = if chat_id.is_empty() {
        Vec::new()
    } else {
        vec![chat_id.to_string()]
    };
    let id = crate::domain::a031_kb_edit::service::create_with_chat(
        title,
        summary,
        edit_type,
        target_articles,
        source_chats,
        contracts::domain::a017_llm_agent::aggregate::LlmAgentId::new(agent_uuid),
        None,
    )
    .await?;
    Ok(id.to_string())
}

// ─── Цитирование ─────────────────────────────────────────────────────────────

/// Найти ссылки `kb://article/<id>` в тексте ответа — так учитывается, что
/// статья дошла до человека, а не просто была прочитана моделью.
pub fn extract_cited_ids(text: &str) -> Vec<String> {
    const MARKER: &str = "kb://article/";
    let mut ids = Vec::new();
    let mut rest = text;
    while let Some(pos) = rest.find(MARKER) {
        rest = &rest[pos + MARKER.len()..];
        let id: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if !id.is_empty() && !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

/// Учесть цитирования в ответе ассистента.
pub fn record_citations(text: &str) {
    let ids = extract_cited_ids(text);
    if ids.is_empty() {
        return;
    }
    let kb = kb_read();
    let refs: Vec<ArticleRef> = ids
        .iter()
        .filter_map(|id| kb.get(id))
        .map(ArticleRef::from)
        .collect();
    if !refs.is_empty() {
        kb_metrics::record_citation(&refs);
    }
}

// ─── Вспомогательное ─────────────────────────────────────────────────────────

fn string_array(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Коды объектов системы: `a012`, `p916`, `u508`, `ds03`, `dv001`, `d406`.
/// Префикс — одна-две латинских буквы, дальше только цифры.
fn is_aggregate_code(value: &str) -> bool {
    let split = value
        .char_indices()
        .find(|(_, c)| c.is_ascii_digit())
        .map(|(i, _)| i);
    let Some(split) = split else { return false };
    let (prefix, digits) = value.split_at(split);
    (1..=2).contains(&prefix.len())
        && prefix.chars().all(|c| c.is_ascii_lowercase())
        && (2..=4).contains(&digits.len())
        && digits.chars().all(|c| c.is_ascii_digit())
}

/// Техническая деталь в тексте статьи. Возвращает, что именно нашли.
fn detect_technical_leak(body: &str, embedded_ids: &[String]) -> Option<String> {
    let lower = body.to_lowercase();
    const MARKERS: &[(&str, &str)] = &[
        ("create table", "SQL DDL"),
        ("select ", "SQL-запрос"),
        ("insert into", "SQL-запрос"),
        ("json_extract(", "SQL-функция"),
        ("/api/", "путь API"),
        ("left join", "SQL JOIN"),
    ];
    for (marker, label) in MARKERS {
        if lower.contains(marker) {
            return Some(format!("{}: «{}»", label, marker.trim()));
        }
    }
    for id in embedded_ids {
        if body.contains(id.as_str()) {
            return Some(format!("ссылка на техдокумент «{}»", id));
        }
    }
    None
}

fn is_valid_slug(slug: &str) -> bool {
    let len = slug.chars().count();
    (3..=64).contains(&len)
        && slug
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn normalize_slug(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.trim().to_lowercase().chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            out.push(ch);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

/// Транслитерация ru→lat для имени файла. Без зависимостей: таблица короткая,
/// а тянуть крейт ради одного вызова не стоит.
fn transliterate(input: &str) -> String {
    input
        .chars()
        .map(|ch| {
            let lower = ch.to_lowercase().next().unwrap_or(ch);
            match lower {
                'а' => "a",
                'б' => "b",
                'в' => "v",
                'г' => "g",
                'д' => "d",
                'е' | 'ё' | 'э' => "e",
                'ж' => "zh",
                'з' => "z",
                'и' | 'й' => "i",
                'к' => "k",
                'л' => "l",
                'м' => "m",
                'н' => "n",
                'о' => "o",
                'п' => "p",
                'р' => "r",
                'с' => "s",
                'т' => "t",
                'у' => "u",
                'ф' => "f",
                'х' => "h",
                'ц' => "c",
                'ч' => "ch",
                'ш' => "sh",
                'щ' => "sch",
                'ъ' | 'ь' => "",
                'ы' => "y",
                'ю' => "yu",
                'я' => "ya",
                other => return other.to_string(),
            }
            .to_string()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sections_of(content: &str) -> Vec<knowledge_base::DocSection> {
        knowledge_base::outline(content)
    }

    const SECTIONED: &str =
        "# Статья\n\nВводка.\n\n## Расчёт ДРР\n\nтекст\n\n## Ограничения\n\nтекст\n";

    #[test]
    fn section_resolves_by_number_and_by_title_fragment() {
        let sections = sections_of(SECTIONED);
        assert_eq!(
            resolve_section(&sections, "2").unwrap().title,
            "Ограничения"
        );
        // Заголовок целиком, в другом регистре.
        assert_eq!(resolve_section(&sections, "расчёт ДРР").unwrap().number, 1);
        // Часть заголовка — модель редко цитирует его дословно.
        assert_eq!(resolve_section(&sections, "ДРР").unwrap().number, 1);
        assert!(resolve_section(&sections, "выкуп").is_none());
        assert!(resolve_section(&sections, "9").is_none());
    }

    #[test]
    fn truncate_chars_cuts_by_chars_not_bytes() {
        // Байтовая обрезка кириллицы режет посреди буквы и роняет форматирование.
        let out = truncate_chars("абвгде", 3);
        assert_eq!(out, "абв…");
        assert_eq!(truncate_chars("абв", 3), "абв");
    }

    #[test]
    fn outline_json_carries_number_title_and_price() {
        let rows = outline_json(&sections_of(SECTIONED));
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["n"], json!(1));
        assert_eq!(rows[0]["title"], json!("Расчёт ДРР"));
        assert!(rows[0]["token_cost"].as_u64().unwrap() > 0);
    }

    fn ctx() -> DraftContext {
        DraftContext {
            existing_ids: vec!["funnel-overview".into(), "drr".into()],
            embedded_ids: vec!["general-ledger".into()],
            known_tags: vec!["воронка".into(), "wildberries".into(), "выкуп".into()],
            today: chrono::NaiveDate::from_ymd_opt(2026, 8, 3).unwrap(),
            chat_id: "chat-42".into(),
        }
    }

    fn body(extra: &str) -> String {
        format!(
            "# Лаг выкупа\n\nВыкуп по заказу приходит позже самого заказа, поэтому текущий месяц \
             всегда выглядит хуже завершённых. Это не падение спроса, а незакрытая когорта: часть \
             заказов ещё едет к покупателю и физически не может быть выкуплена.\n\n\
             Сравнивать текущий месяц с прошлым по доле выкупа некорректно до закрытия периода \
             доставки. Корректное сравнение — по когортам одинаковой зрелости: берём заказы \
             первых двух недель месяца и смотрим их выкуп через равное число дней.\n\n\
             Практическое следствие: не принимай решений о снятии товара с продажи по доле \
             выкупа текущего месяца — дождись закрытия когорты.{}",
            extra
        )
    }

    fn draft(overrides: Value) -> Value {
        let mut base = json!({
            "title": "Лаг выкупа",
            "summary": "Почему текущий месяц по выкупу всегда выглядит хуже, чем есть.",
            "tags": ["воронка", "выкуп"],
            "related": ["funnel-overview", "a012"],
            "body": body(""),
        });
        if let (Some(b), Some(o)) = (base.as_object_mut(), overrides.as_object()) {
            for (k, v) in o {
                b.insert(k.clone(), v.clone());
            }
        }
        base
    }

    #[test]
    fn valid_draft_stamps_expected_frontmatter() {
        let plan = validate_draft(&draft(json!({})), &ctx()).unwrap();
        assert_eq!(plan.slug, "lag-vykupa");
        assert!(plan.content.starts_with("---\ntitle: Лаг выкупа\n"));
        assert!(plan.content.contains("status: draft"));
        assert!(plan.content.contains(&format!("uid: {}", plan.uid)));
        assert!(plan.content.contains("source_chats: [chat-42]"));
        assert!(plan.content.contains("author: llm"));
        assert!(plan.content.contains("updated: 2026-08-03"));
        // Черновик никем не верифицирован — поле намеренно пустое.
        assert!(plan.content.contains("\nverified:\n"));
        // token_cost на диск не пишется: он вычисляется при загрузке.
        assert!(!plan.content.contains("token_cost"));
        assert!(plan.warnings.is_empty(), "{:?}", plan.warnings);
    }

    #[test]
    fn transliterates_title_into_slug() {
        assert_eq!(normalize_slug(&transliterate("Лаг выкупа")), "lag-vykupa");
        assert_eq!(
            normalize_slug(&transliterate("Особенности воронки Wildberries")),
            "osobennosti-voronki-wildberries"
        );
        assert_eq!(
            normalize_slug(&transliterate("Что нельзя считать нулём?")),
            "chto-nelzya-schitat-nulem"
        );
    }

    #[test]
    fn rejects_silent_overwrite_but_allows_explicit_replace() {
        let args = draft(json!({ "slug": "funnel-overview" }));
        let error = validate_draft(&args, &ctx()).unwrap_err();
        assert!(error.contains("уже существует"), "{error}");

        let args = draft(json!({ "slug": "funnel-overview", "replaces": "funnel-overview" }));
        assert!(validate_draft(&args, &ctx()).is_ok());
    }

    #[test]
    fn rejects_collision_with_embedded_doc() {
        let args = draft(json!({ "slug": "general-ledger" }));
        let error = validate_draft(&args, &ctx()).unwrap_err();
        assert!(error.contains("встроенной"), "{error}");
    }

    #[test]
    fn rejects_technical_leakage() {
        // Граница «бизнес-знание vs техдокументация» теперь в коде, а не только в промпте.
        for leak in [
            "\n\nSELECT * FROM p916_mp_sales_funnel_turnovers",
            "\n\nCREATE TABLE x (id TEXT)",
            "\n\nДёрни /api/kb/stats",
            "\n\njson_extract(line_json, '$.a')",
        ] {
            let args = draft(json!({ "body": body(leak) }));
            let error = validate_draft(&args, &ctx()).unwrap_err();
            assert!(
                error.contains("техническая деталь"),
                "не поймано: {leak} → {error}"
            );
        }
    }

    #[test]
    fn aggregate_codes_in_prose_are_allowed() {
        // Коды агрегатов законны в бизнес-тексте, в отличие от SQL и путей API.
        let args = draft(json!({ "body": body("\n\nДанные приходят из a036 и попадают в p916.") }));
        assert!(validate_draft(&args, &ctx()).is_ok());
    }

    #[test]
    fn rejects_double_frontmatter_and_missing_heading() {
        let args = draft(json!({ "body": "---\ntitle: x\n---\n\n# Заголовок\n\nтело" }));
        assert!(validate_draft(&args, &ctx())
            .unwrap_err()
            .contains("frontmatter"));

        let args = draft(json!({ "body": "Просто текст без заголовка. ".repeat(30) }));
        assert!(validate_draft(&args, &ctx())
            .unwrap_err()
            .contains("заголовка первого уровня"));
    }

    #[test]
    fn rejects_too_short_body() {
        let args = draft(json!({ "body": "# Коротко\n\nОдна фраза." }));
        assert!(validate_draft(&args, &ctx())
            .unwrap_err()
            .contains("слишком короткая"));
    }

    #[test]
    fn requires_two_tags_and_single_line_summary() {
        let args = draft(json!({ "tags": ["воронка"] }));
        assert!(validate_draft(&args, &ctx())
            .unwrap_err()
            .contains("минимум 2 тега"));

        let args = draft(json!({ "summary": "Первая строка\nвторая строка" }));
        assert!(validate_draft(&args, &ctx())
            .unwrap_err()
            .contains("одной строкой"));
    }

    #[test]
    fn unknown_tags_warn_but_never_reject() {
        let args = draft(json!({ "tags": ["воронка", "совершенно-новый-тег"] }));
        let plan = validate_draft(&args, &ctx()).unwrap();
        assert_eq!(plan.unknown_tags, vec!["совершенно-новый-тег".to_string()]);
        assert!(plan.warnings.iter().any(|w| w.contains("вне словаря")));
        // Тег всё равно попал в статью — отклонять запись из-за тегов нельзя.
        assert!(plan.tags.contains(&"совершенно-новый-тег".to_string()));
    }

    #[test]
    fn unresolvable_related_is_dropped_with_warning() {
        let args = draft(
            json!({ "related": ["funnel-overview", "воронка", "p916", "выдуманная-статья"] }),
        );
        let plan = validate_draft(&args, &ctx()).unwrap();
        assert!(plan.related.contains(&"funnel-overview".to_string())); // id статьи
        assert!(plan.related.contains(&"воронка".to_string())); // тег
        assert!(plan.related.contains(&"p916".to_string())); // код агрегата
        assert!(!plan.related.contains(&"выдуманная-статья".to_string()));
        assert!(plan
            .warnings
            .iter()
            .any(|w| w.contains("выдуманная-статья")));
    }

    #[test]
    fn extract_cited_ids_finds_links_in_answer_text() {
        let text = "Смотри kb://article/funnel-buyout-lag и ещё kb://article/drr. \
                    Повтор kb://article/drr не должен удваиваться.";
        assert_eq!(
            extract_cited_ids(text),
            vec!["funnel-buyout-lag".to_string(), "drr".to_string()]
        );
        assert!(extract_cited_ids("без ссылок").is_empty());
        // Ссылка в скобках/с пунктуацией не должна утаскивать лишние символы.
        assert_eq!(
            extract_cited_ids("(kb://article/drr)."),
            vec!["drr".to_string()]
        );
    }

    #[test]
    fn aggregate_code_detection() {
        assert!(is_aggregate_code("a012"));
        assert!(is_aggregate_code("p916"));
        assert!(is_aggregate_code("dv001"));
        assert!(!is_aggregate_code("воронка"));
        assert!(!is_aggregate_code("a"));
    }
}
