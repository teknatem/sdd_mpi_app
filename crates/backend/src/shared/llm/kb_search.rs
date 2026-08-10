//! Поиск по базе знаний: BM25F в памяти + расширение по графу `related`.
//!
//! Модуль **чистый и синхронный**: без БД, без конфига, без глобального состояния.
//! Всё — функция от корпуса документов и запроса, поэтому все тесты нативные.
//!
//! До этого поиск умел только точное совпадение по тегу (OR), т.е. статья без
//! нужного тега была невидима для LLM. Теперь основной вход — свободный текст,
//! а теги стали бустом, а не фильтром.

use super::knowledge_base::{KbStatus, KnowledgeDoc};
use std::collections::{HashMap, HashSet};

// ─── Параметры движка ────────────────────────────────────────────────────────

const BM25_K1: f32 = 1.2;
const BM25_B: f32 = 0.75;

const W_TITLE: f32 = 4.0;
const W_TAG: f32 = 3.0;
const W_SUMMARY: f32 = 2.0;
const W_BODY: f32 = 1.0;

// Бусты за тег и заголовок измеряются в «сильных совпадениях терма»: вклад одного
// терма в BM25 не превышает `idf × (k1 + 1)`. Поэтому бусты масштабируются на idf
// запроса — иначе на маленьком корпусе (где idf ≈ 0.1) константа в 2–3 единицы
// полностью подавляет текстовую релевантность, а на большом — теряется в ней.
/// Вклад одного совпавшего канонического тега, в «сильных совпадениях терма».
const TAG_HIT_WEIGHT: f32 = 1.0;
/// Бонус, если запрос покрывает заголовок целиком.
const TITLE_COVER_WEIGHT: f32 = 1.5;

/// Сколько лучших попаданий берём семенами графа.
const GRAPH_SEEDS: usize = 5;
/// Во сколько раз сосед по графу слабее семени.
const GRAPH_DECAY: f32 = 0.35;

/// Отсечка: результат слабее `RELATIVE_FLOOR × лучший` не возвращается.
const RELATIVE_FLOOR: f32 = 0.15;

pub const DEFAULT_LIMIT: usize = 5;
pub const MAX_LIMIT: usize = 10;

/// Кириллические слова длиннее этого свёртываются до префикса.
///
/// Пять, а не шесть: ключевые слова домена короткие (выкуп, заказ, показ), и при
/// шести «выкуп» и «выкупа» так и не сходятся — а это ровно тот промах, из-за
/// которого база «не работала». Плата — редкие пересечения вроде
/// маркетинг/маркетплейс; отказ в сторону полноты выдачи здесь сознательный.
const FOLD_LEN: usize = 5;

// ─── Токенизация ─────────────────────────────────────────────────────────────

const STOPWORDS: &[&str] = &[
    // русский
    "и",
    "в",
    "во",
    "не",
    "что",
    "он",
    "на",
    "я",
    "с",
    "со",
    "как",
    "а",
    "то",
    "все",
    "она",
    "так",
    "его",
    "но",
    "да",
    "ты",
    "к",
    "у",
    "же",
    "вы",
    "за",
    "бы",
    "по",
    "только",
    "ее",
    "мне",
    "было",
    "вот",
    "от",
    "меня",
    "еще",
    "нет",
    "о",
    "из",
    "ему",
    "теперь",
    "когда",
    "даже",
    "ну",
    "вдруг",
    "ли",
    "если",
    "уже",
    "или",
    "ни",
    "быть",
    "был",
    "него",
    "до",
    "вас",
    "нибудь",
    "опять",
    "уж",
    "вам",
    "ведь",
    "там",
    "потом",
    "себя",
    "ничего",
    "ей",
    "может",
    "они",
    "тут",
    "где",
    "есть",
    "надо",
    "ней",
    "для",
    "мы",
    "тебя",
    "их",
    "чем",
    "была",
    "сам",
    "чтоб",
    "без",
    "будто",
    "чего",
    "раз",
    "тоже",
    "себе",
    "под",
    "будет",
    "ж",
    "тогда",
    "кто",
    "этот",
    "того",
    "потому",
    "этого",
    "какой",
    "совсем",
    "ним",
    "здесь",
    "этом",
    "один",
    "почти",
    "мой",
    "тем",
    "чтобы",
    "нее",
    "были",
    "куда",
    "зачем",
    "всех",
    "никогда",
    "можно",
    "при",
    "наконец",
    "два",
    "об",
    "другой",
    "хоть",
    "после",
    "над",
    "больше",
    "тот",
    "через",
    "эти",
    "нас",
    "про",
    "всего",
    "них",
    "какая",
    "много",
    "разве",
    "три",
    "эту",
    "моя",
    "впрочем",
    "хорошо",
    "свою",
    "этой",
    "перед",
    "иногда",
    "лучше",
    "чуть",
    "том",
    "нельзя",
    "такой",
    "им",
    "более",
    "всегда",
    "конечно",
    "всю",
    "между",
    // английский
    "the",
    "of",
    "and",
    "a",
    "to",
    "in",
    "is",
    "it",
    "for",
    "on",
    "as",
    "with",
    "at",
    "by",
    "an",
    "be",
    "this",
    "that",
    "or",
    "from",
    "are",
    "was",
    "were",
    "has",
    "have",
    "had",
    "not",
    "but",
];

/// Разбить текст на поисковые термы: lowercase → `ё`→`е` → сплит по не-алфанумерик
/// → отбросить короткие и стоп-слова → свернуть флексию.
pub fn tokenize(text: &str) -> Vec<String> {
    let stop: HashSet<&str> = STOPWORDS.iter().copied().collect();
    text.to_lowercase()
        .replace('ё', "е")
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| w.chars().count() >= 2)
        .filter(|w| !stop.contains(*w))
        .map(fold_token)
        .filter(|w| !w.is_empty())
        .collect()
}

/// Свёртка русской флексии без стеммера: префикс фиксированной длины.
///
/// Русское словоизменение почти целиком суффиксальное, поэтому шестибуквенный
/// префикс схлопывает комиссия/комиссии/комиссий → `комисс`, воронка/воронки →
/// `воронк`. Коды и идентификаторы (`a012`, `p916`, `nm_id`) защищены наличием
/// цифры или подчёркивания, латиница — целиком (английская флексия здесь не важна).
pub fn fold_token(token: &str) -> String {
    let t = token.trim();
    if t.is_empty() {
        return String::new();
    }
    // Коды сущностей и имена полей должны оставаться точными.
    if t.chars().any(|c| c.is_ascii_digit() || c == '_') {
        return t.to_string();
    }
    if t.is_ascii() {
        return t.to_string();
    }
    if t.chars().count() > FOLD_LEN {
        return t.chars().take(FOLD_LEN).collect();
    }
    t.to_string()
}

// ─── Индекс ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Posting {
    doc: u32,
    /// Частота терма, уже взвешенная по полю, в котором он встретился.
    tf: f32,
}

/// Инвертированный индекс корпуса. Строится при загрузке базы знаний.
#[derive(Debug, Default)]
pub struct SearchIndex {
    postings: HashMap<String, Vec<Posting>>,
    doc_ids: Vec<String>,
    doc_len: Vec<f32>,
    avg_len: f32,
    /// Средний idf по корпусу — запасной масштаб для бустов, когда в запросе
    /// нет текста (поиск только по тегам).
    mean_idf: f32,
}

fn idf_of(n_docs: f32, df: f32) -> f32 {
    (1.0 + (n_docs - df + 0.5) / (df + 0.5)).ln()
}

/// Учесть текст поля с его весом в частотах документа.
fn absorb(text: &str, weight: f32, tf: &mut HashMap<String, f32>, len: &mut f32) {
    for term in tokenize(text) {
        *tf.entry(term).or_insert(0.0) += weight;
        *len += weight;
    }
}

impl SearchIndex {
    pub fn build<'a>(docs: impl Iterator<Item = &'a KnowledgeDoc>) -> Self {
        let mut postings: HashMap<String, Vec<Posting>> = HashMap::new();
        let mut doc_ids: Vec<String> = Vec::new();
        let mut doc_len: Vec<f32> = Vec::new();

        for doc in docs {
            let idx = doc_ids.len() as u32;
            let mut tf: HashMap<String, f32> = HashMap::new();
            let mut len = 0.0f32;

            absorb(&doc.title, W_TITLE, &mut tf, &mut len);
            absorb(&doc.summary, W_SUMMARY, &mut tf, &mut len);
            for tag in doc.canonical_tags.iter().chain(doc.aliases.iter()) {
                absorb(tag, W_TAG, &mut tf, &mut len);
            }
            absorb(&doc.content, W_BODY, &mut tf, &mut len);

            for (term, weight) in tf {
                postings.entry(term).or_default().push(Posting {
                    doc: idx,
                    tf: weight,
                });
            }
            doc_ids.push(doc.id.clone());
            doc_len.push(len.max(1.0));
        }

        let avg_len = if doc_len.is_empty() {
            1.0
        } else {
            doc_len.iter().sum::<f32>() / doc_len.len() as f32
        };

        let n = doc_ids.len() as f32;
        let mean_idf = if postings.is_empty() {
            1.0
        } else {
            postings
                .values()
                .map(|list| idf_of(n, list.len() as f32))
                .sum::<f32>()
                / postings.len() as f32
        };

        Self {
            postings,
            doc_ids,
            doc_len,
            avg_len,
            mean_idf,
        }
    }

    pub fn doc_count(&self) -> usize {
        self.doc_ids.len()
    }

    /// Масштаб бустов: средний вклад одного сильно совпавшего терма запроса.
    /// Делает тег/заголовок соизмеримыми с BM25 на корпусе любого размера.
    fn boost_unit(&self, terms: &[String]) -> f32 {
        let n = self.doc_ids.len() as f32;
        let matched: Vec<f32> = terms
            .iter()
            .filter_map(|t| self.postings.get(t))
            .map(|list| idf_of(n, list.len() as f32))
            .collect();
        let idf = if matched.is_empty() {
            self.mean_idf
        } else {
            matched.iter().sum::<f32>() / matched.len() as f32
        };
        idf * (BM25_K1 + 1.0)
    }

    /// Сырой BM25 по термам запроса: `doc_id → score`.
    fn bm25(&self, terms: &[String]) -> HashMap<&str, f32> {
        let n = self.doc_ids.len() as f32;
        let mut scores: HashMap<u32, f32> = HashMap::new();

        for term in terms {
            let Some(list) = self.postings.get(term) else {
                continue;
            };
            let idf = idf_of(n, list.len() as f32);
            for posting in list {
                let len = self.doc_len[posting.doc as usize];
                let norm = posting.tf * (BM25_K1 + 1.0)
                    / (posting.tf + BM25_K1 * (1.0 - BM25_B + BM25_B * len / self.avg_len));
                *scores.entry(posting.doc).or_insert(0.0) += idf * norm;
            }
        }

        scores
            .into_iter()
            .map(|(idx, score)| (self.doc_ids[idx as usize].as_str(), score))
            .collect()
    }
}

// ─── Запрос и результат ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Query {
    /// Свободный текст вопроса — основной способ поиска.
    pub text: String,
    /// Необязательное уточнение; теги уже нормализованы вызывающим по словарю.
    pub tags: Vec<String>,
    pub limit: usize,
    pub include_drafts: bool,
    pub include_deprecated: bool,
}

impl Default for Query {
    fn default() -> Self {
        Self {
            text: String::new(),
            tags: Vec::new(),
            limit: DEFAULT_LIMIT,
            include_drafts: true,
            include_deprecated: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SearchHit {
    pub id: String,
    pub score: f32,
    /// Заполнено, если статья пришла расширением по графу: `related:<id семени>`.
    pub via: Option<String>,
}

/// Корпус, по которому идёт поиск. Ссылки на структуры `KnowledgeBase`.
pub struct Corpus<'a> {
    pub docs: &'a HashMap<String, KnowledgeDoc>,
    /// Канонический тег → id документов.
    pub tag_index: &'a HashMap<String, Vec<String>>,
    /// Обратные рёбра `related`.
    pub back_links: &'a HashMap<String, Vec<String>>,
}

/// Результат поиска до отсечки по `limit`.
pub struct SearchOutcome {
    pub hits: Vec<SearchHit>,
    /// Сколько статей вообще совпало — чтобы модель знала, что было больше.
    pub total_matched: usize,
}

pub fn search(index: &SearchIndex, corpus: &Corpus<'_>, query: &Query) -> SearchOutcome {
    let terms = tokenize(&query.text);
    let boost_unit = index.boost_unit(&terms);
    let mut scores: HashMap<String, f32> = HashMap::new();

    // 1. BM25 по свободному тексту.
    for (id, score) in index.bm25(&terms) {
        scores.insert(id.to_string(), score);
    }

    // 2. Теги бустят, но НЕ фильтруют: статья без тега остаётся видимой по тексту.
    for tag in &query.tags {
        let Some(ids) = corpus.tag_index.get(tag) else {
            continue;
        };
        for id in ids {
            *scores.entry(id.clone()).or_insert(0.0) += TAG_HIT_WEIGHT * boost_unit;
        }
    }

    // 3. Бонус за покрытие заголовка.
    if !terms.is_empty() {
        let query_set: HashSet<&String> = terms.iter().collect();
        for (id, score) in scores.iter_mut() {
            let Some(doc) = corpus.docs.get(id) else {
                continue;
            };
            let title_terms = tokenize(&doc.title);
            if !title_terms.is_empty() && title_terms.iter().all(|t| query_set.contains(t)) {
                *score += TITLE_COVER_WEIGHT * boost_unit;
            }
        }
    }

    // 4. Модификаторы качества/свежести/статуса — мультипликативные.
    //    Они модулируют релевантность, но никогда её не создают: нерелевантная
    //    свежая пятизвёздочная статья остаётся около нуля.
    let mut direct: Vec<(String, f32)> = Vec::new();
    for (id, base) in scores {
        let Some(doc) = corpus.docs.get(&id) else {
            continue;
        };
        let Some(status_factor) = status_factor(doc.status, query) else {
            continue;
        };
        let final_score = base * quality_factor(doc) * freshness_factor(doc) * status_factor;
        if final_score > 0.0 {
            direct.push((id, final_score));
        }
    }
    direct.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    // 5. Расширение по графу: один шаг от лучших попаданий.
    let expanded = expand_graph(&direct, corpus, query);

    let mut all: Vec<SearchHit> = direct
        .iter()
        .map(|(id, score)| SearchHit {
            id: id.clone(),
            score: *score,
            via: None,
        })
        .collect();
    all.extend(expanded);
    all.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.id.cmp(&b.id)));

    let total_matched = all.len();

    // 6. Отсечка: сначала относительный порог, затем limit.
    let limit = query.limit.clamp(1, MAX_LIMIT);
    let floor = all.first().map(|h| h.score * RELATIVE_FLOOR).unwrap_or(0.0);
    let hits = all
        .into_iter()
        .filter(|h| h.score >= floor)
        .take(limit)
        .collect();

    SearchOutcome {
        hits,
        total_matched,
    }
}

/// Один шаг по графу от лучших семян. Рёбра двунаправленные, и запись `related`
/// резолвится не только в статью с таким id, но и во все статьи с таким тегом —
/// это оживляет уже существующие данные `related:` без правки файлов.
fn expand_graph(direct: &[(String, f32)], corpus: &Corpus<'_>, query: &Query) -> Vec<SearchHit> {
    let already: HashSet<&str> = direct.iter().map(|(id, _)| id.as_str()).collect();
    let mut best: HashMap<String, (f32, String)> = HashMap::new();

    for (seed_id, seed_score) in direct.iter().take(GRAPH_SEEDS) {
        let Some(seed) = corpus.docs.get(seed_id) else {
            continue;
        };
        let neighbours = seed
            .related
            .iter()
            .flat_map(|entry| resolve_related(entry, corpus))
            .chain(
                corpus
                    .back_links
                    .get(seed_id)
                    .into_iter()
                    .flatten()
                    .cloned(),
            );

        for neighbour_id in neighbours {
            if neighbour_id == *seed_id || already.contains(neighbour_id.as_str()) {
                continue;
            }
            let Some(doc) = corpus.docs.get(&neighbour_id) else {
                continue;
            };
            let Some(status_factor) = status_factor(doc.status, query) else {
                continue;
            };
            let score = seed_score * GRAPH_DECAY * status_factor;
            let entry = best
                .entry(neighbour_id)
                .or_insert((0.0, format!("related:{}", seed_id)));
            if score > entry.0 {
                *entry = (score, format!("related:{}", seed_id));
            }
        }
    }

    best.into_iter()
        .map(|(id, (score, via))| SearchHit {
            id,
            score,
            via: Some(via),
        })
        .collect()
}

/// Запись `related` → {статья с таким id} ∪ {статьи с таким каноническим тегом}.
fn resolve_related(entry: &str, corpus: &Corpus<'_>) -> Vec<String> {
    let mut out = Vec::new();
    if corpus.docs.contains_key(entry) {
        out.push(entry.to_string());
    }
    let key = super::kb_vocabulary::normalize_form(entry);
    if let Some(ids) = corpus.tag_index.get(&key) {
        out.extend(ids.iter().cloned());
    }
    out
}

/// `None` — документ исключается из выдачи целиком.
fn status_factor(status: KbStatus, query: &Query) -> Option<f32> {
    match status {
        KbStatus::Active => Some(1.0),
        KbStatus::Draft if query.include_drafts => Some(0.55),
        KbStatus::Draft => None,
        KbStatus::Deprecated if query.include_deprecated => Some(0.4),
        KbStatus::Deprecated => None,
    }
}

/// 1★ → 0.9 … 3★ (или без оценки) → 1.1 … 5★ → 1.3
fn quality_factor(doc: &KnowledgeDoc) -> f32 {
    0.8 + 0.1 * f32::from(doc.stars.unwrap_or(3))
}

/// Протухание понижает скор не более чем на 40 %: устаревшая статья остаётся
/// findable, просто уступает свежей при прочих равных.
fn freshness_factor(doc: &KnowledgeDoc) -> f32 {
    // Embedded-доки едут вместе с бинарником — они по определению свежие.
    if doc.is_embedded {
        return 1.0;
    }
    let staleness = match (doc.age_days, doc.ttl_days) {
        (Some(age), Some(ttl)) if ttl > 0 => (age as f32 / ttl as f32).clamp(0.0, 1.0),
        // Возраст неизвестен — не наказываем и не поощряем.
        _ => 0.5,
    };
    (1.0 - 0.4 * staleness).clamp(0.6, 1.0)
}

// ─── Тесты ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(id: &str, title: &str, tags: &[&str], body: &str) -> KnowledgeDoc {
        KnowledgeDoc {
            id: id.to_string(),
            title: title.to_string(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            canonical_tags: tags.iter().map(|t| t.to_string()).collect(),
            content: body.to_string(),
            ..KnowledgeDoc::default()
        }
    }

    struct Fixture {
        docs: HashMap<String, KnowledgeDoc>,
        tag_index: HashMap<String, Vec<String>>,
        back_links: HashMap<String, Vec<String>>,
        index: SearchIndex,
    }

    impl Fixture {
        fn new(docs: Vec<KnowledgeDoc>) -> Self {
            let mut tag_index: HashMap<String, Vec<String>> = HashMap::new();
            let mut back_links: HashMap<String, Vec<String>> = HashMap::new();
            for d in &docs {
                for t in &d.canonical_tags {
                    tag_index.entry(t.clone()).or_default().push(d.id.clone());
                }
                for r in &d.related {
                    back_links.entry(r.clone()).or_default().push(d.id.clone());
                }
            }
            let index = SearchIndex::build(docs.iter());
            let docs = docs.into_iter().map(|d| (d.id.clone(), d)).collect();
            Self {
                docs,
                tag_index,
                back_links,
                index,
            }
        }

        fn corpus(&self) -> Corpus<'_> {
            Corpus {
                docs: &self.docs,
                tag_index: &self.tag_index,
                back_links: &self.back_links,
            }
        }

        fn run(&self, query: Query) -> Vec<SearchHit> {
            search(&self.index, &self.corpus(), &query).hits
        }
    }

    fn text_query(text: &str) -> Query {
        Query {
            text: text.to_string(),
            ..Query::default()
        }
    }

    #[test]
    fn folds_russian_inflections_but_protects_codes() {
        assert_eq!(fold_token("комиссия"), fold_token("комиссии"));
        assert_eq!(fold_token("комиссия"), fold_token("комиссионный"));
        assert_eq!(fold_token("воронка"), fold_token("воронке"));
        // Коды сущностей и имена полей не режем.
        assert_eq!(fold_token("a012"), "a012");
        assert_eq!(fold_token("p916"), "p916");
        assert_eq!(fold_token("nm_id"), "nm_id");
        // Короткие слова остаются целыми.
        assert_eq!(fold_token("drr"), "drr");
        assert_eq!(fold_token("выкуп"), "выкуп");
        // Ключевое: короткие слова домена всё же сходятся со своими формами —
        // «падает выкуп» обязано находить «лаг выкупа».
        assert_eq!(fold_token("выкуп"), fold_token("выкупа"));
        assert_eq!(fold_token("заказ"), fold_token("заказа"));
        assert_eq!(fold_token("показ"), fold_token("показы"));
    }

    #[test]
    fn tag_boost_stays_commensurate_with_text_across_corpus_sizes() {
        // Бусты масштабируются на idf запроса, поэтому соотношение
        // «релевантная по тексту» / «релевантная по тегу» не должно зависеть
        // от размера корпуса. Раньше константный буст подавлял BM25 на малом
        // корпусе и терялся на большом.
        let ratio_for = |filler: usize| {
            let mut docs = vec![
                doc(
                    "by_text",
                    "А",
                    &["прочее"],
                    "конверсия корзины в заказ падает",
                ),
                doc("by_tag", "Б", &["воронка"], "совершенно посторонний текст"),
            ];
            for i in 0..filler {
                docs.push(doc(
                    &format!("f{i}"),
                    "Ф",
                    &["иное"],
                    "нейтральный наполнитель склад логистика",
                ));
            }
            let f = Fixture::new(docs);
            let hits = f.run(Query {
                text: "конверсия корзины в заказ".into(),
                tags: vec!["воронка".into()],
                limit: MAX_LIMIT,
                ..Query::default()
            });
            let text = hits.iter().find(|h| h.id == "by_text").unwrap().score;
            let tag = hits.iter().find(|h| h.id == "by_tag").unwrap().score;
            text / tag
        };

        let small = ratio_for(1);
        let large = ratio_for(60);
        assert!(
            (small / large).clamp(0.25, 4.0) == small / large,
            "соотношение уехало более чем вчетверо: {small} против {large}"
        );
    }

    #[test]
    fn tokenize_drops_stopwords_and_short_words() {
        let t = tokenize("Почему в текущем месяце падает выкуп");
        assert!(!t.iter().any(|w| w == "в"));
        assert!(t.contains(&"выкуп".to_string()));
        assert!(t.contains(&fold_token("текущем")));
    }

    #[test]
    fn bm25_ranks_specific_over_incidental() {
        let f = Fixture::new(vec![
            doc(
                "buyout",
                "Лаг выкупа",
                &["выкуп"],
                "Выкуп приходит позже заказа, выкуп догоняет к концу месяца.",
            ),
            doc(
                "misc",
                "Разное",
                &["прочее"],
                "Здесь один раз упомянут выкуп и много другого текста про склады и логистику.",
            ),
            doc(
                "far",
                "Комиссии",
                &["комиссии"],
                "Комиссия зависит от категории товара.",
            ),
        ]);
        let hits = f.run(text_query("выкуп"));
        assert_eq!(hits[0].id, "buyout");
        assert!(!hits.iter().any(|h| h.id == "far"));
    }

    #[test]
    fn tag_hit_boosts_but_does_not_filter() {
        let f = Fixture::new(vec![
            doc("tagged", "Заголовок", &["воронка"], "Ничего по теме."),
            doc(
                "untagged",
                "Другое",
                &["прочее"],
                "Конверсия корзины в заказ — ключевая метрика воронки.",
            ),
        ]);
        // Статья без тега обязана находиться по сильному совпадению в теле.
        let hits = f.run(text_query("конверсия корзины в заказ"));
        assert!(
            hits.iter().any(|h| h.id == "untagged"),
            "статья без тега исчезла из выдачи — это ровно тот баг, который чиним"
        );
    }

    #[test]
    fn deprecated_excluded_and_draft_discounted() {
        let mut draft = doc(
            "draft",
            "Воронка черновик",
            &["воронка"],
            "Текст про воронку продаж.",
        );
        draft.status = KbStatus::Draft;
        let mut dep = doc(
            "dep",
            "Воронка старая",
            &["воронка"],
            "Текст про воронку продаж.",
        );
        dep.status = KbStatus::Deprecated;
        let active = doc(
            "active",
            "Воронка",
            &["воронка"],
            "Текст про воронку продаж.",
        );
        let f = Fixture::new(vec![draft, dep, active]);

        let hits = f.run(text_query("воронка продаж"));
        assert!(
            !hits.iter().any(|h| h.id == "dep"),
            "deprecated не исключён"
        );
        let a = hits.iter().find(|h| h.id == "active").unwrap().score;
        let d = hits.iter().find(|h| h.id == "draft").unwrap().score;
        assert!(
            a > d,
            "черновик должен быть понижен относительно активной статьи"
        );

        let hidden = f.run(Query {
            text: "воронка продаж".into(),
            include_drafts: false,
            ..Query::default()
        });
        assert!(!hidden.iter().any(|h| h.id == "draft"));
    }

    #[test]
    fn graph_expansion_pulls_neighbour_once_and_marks_via() {
        let mut seed = doc(
            "seed",
            "Лаг выкупа",
            &["выкуп"],
            "Выкуп догоняет заказ с задержкой.",
        );
        seed.related = vec!["neighbour".into()];
        let neighbour = doc(
            "neighbour",
            "Особенности WB",
            &["wildberries"],
            "Совсем другой текст.",
        );
        let f = Fixture::new(vec![seed, neighbour]);

        let hits = f.run(text_query("выкуп"));
        let n = hits
            .iter()
            .filter(|h| h.id == "neighbour")
            .collect::<Vec<_>>();
        assert_eq!(n.len(), 1, "сосед добавлен дважды");
        assert_eq!(n[0].via.as_deref(), Some("related:seed"));
        assert!(n[0].score < hits[0].score);
    }

    #[test]
    fn bidirectional_related_edges() {
        // У `child` есть ребро на `parent`; поиск, попавший в `parent`,
        // обязан дотянуться до `child` по обратному ребру.
        let parent = doc(
            "parent",
            "Воронка продаж",
            &["воронка"],
            "Каноническая модель воронки.",
        );
        let mut child = doc(
            "child",
            "Диагностика",
            &["диагностика"],
            "Посторонний текст.",
        );
        child.related = vec!["parent".into()];
        let f = Fixture::new(vec![parent, child]);

        let hits = f.run(text_query("каноническая модель воронки"));
        assert!(
            hits.iter().any(|h| h.id == "child" && h.via.is_some()),
            "обратное ребро не сработало"
        );
    }

    #[test]
    fn related_resolves_through_tags() {
        // `related: [воронка]` — это тег, а не id: должен дотянуть все статьи с тегом.
        let mut seed = doc("seed", "Лаг выкупа", &["выкуп"], "Выкуп догоняет заказ.");
        seed.related = vec!["воронка".into()];
        let by_tag = doc("overview", "Обзор", &["воронка"], "Посторонний текст.");
        let f = Fixture::new(vec![seed, by_tag]);

        let hits = f.run(text_query("выкуп"));
        assert!(hits.iter().any(|h| h.id == "overview" && h.via.is_some()));
    }

    #[test]
    fn fresh_high_star_beats_stale_low_star_at_equal_relevance() {
        let body = "Одинаковый текст про воронку продаж и конверсию.";
        let mut good = doc("good", "А", &["воронка"], body);
        good.stars = Some(5);
        good.age_days = Some(0);
        good.ttl_days = Some(180);
        let mut bad = doc("bad", "Б", &["воронка"], body);
        bad.stars = Some(1);
        bad.age_days = Some(400);
        bad.ttl_days = Some(180);
        let f = Fixture::new(vec![good, bad]);

        let hits = f.run(text_query("воронка конверсия"));
        assert_eq!(hits[0].id, "good");
    }

    #[test]
    fn quality_never_manufactures_relevance() {
        let mut star = doc(
            "star",
            "Комиссии",
            &["комиссии"],
            "Про комиссии маркетплейса.",
        );
        star.stars = Some(5);
        let relevant = doc(
            "relevant",
            "Выкуп",
            &["выкуп"],
            "Выкуп и лаг выкупа по когорте.",
        );
        let f = Fixture::new(vec![star, relevant]);

        let hits = f.run(text_query("лаг выкупа"));
        assert_eq!(hits[0].id, "relevant");
        assert!(!hits.iter().any(|h| h.id == "star"));
    }

    #[test]
    fn empty_query_with_tags_matches_legacy_or_semantics() {
        // Защищает `search_by_tags` и его второго потребителя `find_page_help`.
        let f = Fixture::new(vec![
            doc("a", "А", &["user-guide", "page:sales"], "текст"),
            doc("b", "Б", &["user-guide"], "текст"),
            doc("c", "В", &["прочее"], "текст"),
        ]);
        let hits = f.run(Query {
            tags: vec!["user-guide".into(), "page:sales".into()],
            ..Query::default()
        });
        let ids: Vec<&str> = hits.iter().map(|h| h.id.as_str()).collect();
        assert!(ids.contains(&"a") && ids.contains(&"b"));
        assert!(!ids.contains(&"c"));
        // Больше совпавших тегов — выше в выдаче.
        assert_eq!(hits[0].id, "a");
    }

    #[test]
    fn result_cap_and_relative_floor() {
        let mut docs = Vec::new();
        for i in 0..20 {
            docs.push(doc(
                &format!("d{i}"),
                "Воронка",
                &["воронка"],
                "воронка продаж конверсия",
            ));
        }
        let f = Fixture::new(docs);
        let hits = f.run(Query {
            text: "воронка".into(),
            limit: 3,
            ..Query::default()
        });
        assert_eq!(hits.len(), 3, "limit не соблюдён");

        let outcome = search(
            &f.index,
            &f.corpus(),
            &Query {
                text: "воронка".into(),
                limit: 3,
                ..Query::default()
            },
        );
        assert_eq!(
            outcome.total_matched, 20,
            "total_matched считается до отсечки"
        );
    }

    #[test]
    fn limit_is_clamped_to_max() {
        let f = Fixture::new(
            (0..30)
                .map(|i| doc(&format!("d{i}"), "Воронка", &["воронка"], "воронка"))
                .collect(),
        );
        let hits = f.run(Query {
            text: "воронка".into(),
            limit: 999,
            ..Query::default()
        });
        assert!(hits.len() <= MAX_LIMIT);
    }

    #[test]
    fn empty_corpus_and_empty_query_are_safe() {
        let f = Fixture::new(vec![]);
        assert!(f.run(text_query("что угодно")).is_empty());

        let g = Fixture::new(vec![doc("a", "А", &["t"], "текст")]);
        assert!(g.run(Query::default()).is_empty());
    }
}
