//! DTO дашборда качества LLM (d407).
//!
//! Собирает в один ответ то, что раньше существовало по отдельности и без
//! потребителя: трассу вызовов инструментов (`sys_tool_trace`), вердикты о
//! качестве ответов (`sys_llm_verdict`) и наблюдаемую ценность статей базы
//! знаний (`sys_kb_article_metrics`).

use serde::{Deserialize, Serialize};

/// Инструмент за период: сколько раз звали, сколько раз упал, латентность.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolStat {
    pub tool: String,
    pub calls: i64,
    #[serde(default)]
    pub failures: i64,
    #[serde(default)]
    pub avg_ms: f64,
    #[serde(default)]
    pub max_ms: i64,
}

impl ToolStat {
    pub fn failure_rate(&self) -> f64 {
        if self.calls == 0 {
            return 0.0;
        }
        self.failures as f64 / self.calls as f64
    }
}

/// Итерации цикла инструментов на один ответ. Потолок цикла — 40; значения
/// рядом с ним означают, что ответ дался почти на пределе.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IterationStat {
    #[serde(default)]
    pub avg_iterations: f64,
    #[serde(default)]
    pub max_iterations: i64,
    /// Ответы, потребовавшие 20+ итераций.
    #[serde(default)]
    pub heavy_answers: i64,
    #[serde(default)]
    pub answers: i64,
}

/// Количество вердиктов одного вида в одном источнике оценки.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerdictCount {
    /// audit — живые диалоги; golden — эталонный набор.
    pub source: String,
    /// solved | partial | failed
    pub verdict: String,
    pub n: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureKindCount {
    pub failure_kind: String,
    pub n: i64,
}

/// Разрез вердиктов по навыку: где именно чинить.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVerdictStat {
    pub skill_id: String,
    pub total: i64,
    #[serde(default)]
    pub solved: i64,
    #[serde(default)]
    pub failed: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentCount {
    pub intent: String,
    pub n: i64,
}

/// Наблюдаемая ценность статьи: много поиска и мало чтений — плохой summary;
/// много чтений и мало цитирований — плохая статья.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KbArticleStat {
    pub doc_id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub search_hits: i64,
    #[serde(default)]
    pub read_hits: i64,
    #[serde(default)]
    pub cited_hits: i64,
    #[serde(default)]
    pub open_issue_count: i64,
}

/// Последние вердикты — чтобы метрика не была безымянной: по строке видно,
/// какой именно диалог и почему оценён так.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentVerdict {
    pub chat_id: String,
    #[serde(default)]
    pub chat_title: Option<String>,
    pub source: String,
    pub verdict: String,
    #[serde(default)]
    pub failure_kind: Option<String>,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub skill_id: Option<String>,
    #[serde(default)]
    pub agent_type: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub case_id: Option<String>,
    pub created_at: String,
}

/// Стоимость прогонов за окно, в разрезе валюты.
///
/// Разрез по валюте, а не одна сумма: подключения тарифицируются в разных
/// валютах, и курсами подсистема стоимости не занимается — складывать рубли
/// с долларами в одно число значит показать неверную цифру уверенным тоном.
///
/// `solved_*` считаются только по ответам с вердиктом `solved` — это и есть
/// знаменатель для вопроса «во сколько обходится решённая задача».
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostStat {
    pub currency: String,
    /// Суммарная стоимость в микроединицах валюты (1e-6).
    pub total_micro: i64,
    /// Сколько ответов попало в сумму (у которых стоимость посчитана).
    pub answers: i64,
    /// Стоимость ответов, получивших вердикт `solved`.
    #[serde(default)]
    pub solved_micro: i64,
    #[serde(default)]
    pub solved_answers: i64,
}

impl CostStat {
    /// Стоимость в единицах валюты (для отображения).
    pub fn total(&self) -> f64 {
        self.total_micro as f64 / 1_000_000.0
    }

    pub fn avg_per_answer(&self) -> f64 {
        if self.answers <= 0 {
            return 0.0;
        }
        self.total() / self.answers as f64
    }

    /// Средняя стоимость решённой задачи. `None`, если решённых нет —
    /// ноль здесь читался бы как «бесплатно», а не как «не на чем считать».
    pub fn avg_per_solved(&self) -> Option<f64> {
        (self.solved_answers > 0)
            .then(|| self.solved_micro as f64 / 1_000_000.0 / self.solved_answers as f64)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmQualityOverview {
    pub days: i64,
    pub tools: Vec<ToolStat>,
    #[serde(default)]
    pub iterations: IterationStat,
    pub verdicts: Vec<VerdictCount>,
    pub failure_kinds: Vec<FailureKindCount>,
    pub by_skill: Vec<SkillVerdictStat>,
    pub intents: Vec<IntentCount>,
    pub kb_articles: Vec<KbArticleStat>,
    pub recent_verdicts: Vec<RecentVerdict>,
    #[serde(default)]
    pub costs: Vec<CostStat>,
}
