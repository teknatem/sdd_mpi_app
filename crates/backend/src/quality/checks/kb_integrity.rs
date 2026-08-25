//! ## Проверка: целостность базы знаний
//!
//! Инварианты корпуса статей, применённые ко **всему** корпусу, а не только к
//! тому, что записано инструментом.
//!
//! До этой проверки три правила — `summary` есть, тело не короче
//! `MIN_BODY_CHARS`, структура по `SEC-1` — жили только в валидации записи
//! `kb_propose_article`. На статью, попавшую в базу иначе (правкой в Obsidian,
//! генератором карт, копированием файла), они не распространялись вовсе. Так в
//! курируемом корпусе оказались файлы нулевого размера: правило существовало,
//! но применялось не ко всем.
//!
//! | Метрика | Популяция | Нарушение |
//! |---|---|---|
//! | `structure` | документы с телом | тело не проходит `SEC-1` |
//! | `summary` | все документы | `summary` пуст |
//! | `body_length` | все документы | тело короче порога |
//! | `dangling_links` | записи `related` | запись не ведёт ни в статью, ни в тег, ни в объект |
//! | `unknown_anchors` | записи `entities` | якорь вне реестра сущностей |
//! | `unknown_tags` | теги документов | тег вне словаря |
//!
//! Популяции намеренно разные: у структуры это документы, у висячих ссылок —
//! записи `related`. Общий знаменатель усреднил бы «одна битая ссылка из
//! четырёхсот» и «половина статей без `summary`» в одно ничего не значащее число.

use contracts::quality::{CheckMetric, CheckResult, QualityCheckInfo, ViolationItem};

pub const CHECK_ID: &str = "kb_integrity";

/// Столько же, сколько требует валидация записи: два порога для одного правила
/// означали бы, что статья проходит запись и падает на проверке.
const MIN_BODY_CHARS: usize = 400;

/// Сколько нарушителей показывать поимённо. Список чинят руками, и после двух
/// десятков он перестаёт быть рабочим.
const VIOLATION_SAMPLE_LIMIT: usize = 20;

pub fn info() -> QualityCheckInfo {
    QualityCheckInfo {
        code: String::new(),
        id: CHECK_ID.to_string(),
        name: "Целостность базы знаний".to_string(),
        description: "Применяет ко всему корпусу правила, которые до сих пор проверялись только \
                      при записи через инструмент: структура разделов (SEC-1), наличие summary, \
                      минимальная длина тела, висячие ссылки related, якоря вне реестра \
                      сущностей и теги вне словаря."
            .to_string(),
        category: "База знаний".to_string(),
    }
}

/// Одно нарушение: документ, вид дефекта и подробность для списка.
struct Defect {
    kind: &'static str,
    doc_id: String,
    source: Option<String>,
    detail: String,
}

pub async fn run() -> anyhow::Result<CheckResult> {
    let kb = crate::shared::llm::knowledge_base::kb_read();
    let docs = kb.all_docs();

    let mut defects: Vec<Defect> = Vec::new();
    let (mut structure_bad, mut summary_bad, mut body_bad) = (0i64, 0i64, 0i64);
    let (mut related_total, mut dangling) = (0i64, 0i64);
    let (mut entities_total, mut unknown_anchors) = (0i64, 0i64);
    let (mut tags_total, mut unknown_tags) = (0i64, 0i64);

    for doc in &docs {
        let source = doc.source_path.clone();

        if doc.summary.trim().is_empty() {
            summary_bad += 1;
            defects.push(Defect {
                kind: "missing_summary",
                doc_id: doc.id.clone(),
                source: source.clone(),
                detail: "нет summary: статья приходит в выдачу поиска без описания".to_string(),
            });
        }

        let body_chars = doc.content.chars().count();
        if body_chars < MIN_BODY_CHARS {
            body_bad += 1;
            defects.push(Defect {
                kind: "short_body",
                doc_id: doc.id.clone(),
                source: source.clone(),
                detail: format!("тело {body_chars} символов при пороге {MIN_BODY_CHARS}"),
            });
        }

        // Пустой файл проверять на структуру бессмысленно: он уже пойман длиной,
        // а `SEC-01` добавил бы к нему второе нарушение об одном и том же.
        if body_chars > 0 {
            // Якоря обязательны только у документа, который перерастает порог
            // чтения по частям: у мелкого режим `section` не включается.
            let require_anchors =
                doc.token_cost > crate::shared::llm::kb_tools::SECTIONED_READ_TOKENS;
            let violations = crate::shared::llm::knowledge_base::validate_structure(
                &doc.content,
                require_anchors,
            );
            if !violations.is_empty() {
                structure_bad += 1;
                let summary = violations
                    .iter()
                    .map(|v| {
                        if v.line == 0 {
                            v.code.to_string()
                        } else {
                            format!("{} (строка {})", v.code, v.line)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                defects.push(Defect {
                    kind: "structure",
                    doc_id: doc.id.clone(),
                    source: source.clone(),
                    detail: format!("{}: {}", summary, violations[0].detail),
                });
            }
        }

        related_total += doc.related.len() as i64;
        for link in kb.dangling_links_of(doc) {
            dangling += 1;
            defects.push(Defect {
                kind: "dangling_link",
                doc_id: doc.id.clone(),
                source: source.clone(),
                detail: format!("ссылка «{link}» не ведёт ни в статью, ни в тег, ни в объект"),
            });
        }

        entities_total += doc.entities.len() as i64;
        for anchor in &doc.unknown_anchors {
            unknown_anchors += 1;
            defects.push(Defect {
                kind: "unknown_anchor",
                doc_id: doc.id.clone(),
                source: source.clone(),
                detail: format!("якорь «{anchor}» вне реестра сущностей — привязка не работает"),
            });
        }

        tags_total += doc.tags.len() as i64;
        for tag in &doc.unknown_tags {
            unknown_tags += 1;
            defects.push(Defect {
                kind: "unknown_tag",
                doc_id: doc.id.clone(),
                source: source.clone(),
                detail: format!("тег «{tag}» вне словаря"),
            });
        }
    }

    let total_docs = docs.len() as i64;
    let metrics = vec![
        CheckMetric {
            label: "Документы, нарушающие стандарт разделов (SEC-1)".to_string(),
            population: total_docs,
            violations: structure_bad,
            unit: "документов".to_string(),
        },
        CheckMetric {
            label: "Документы без summary".to_string(),
            population: total_docs,
            violations: summary_bad,
            unit: "документов".to_string(),
        },
        CheckMetric {
            label: format!("Документы короче {MIN_BODY_CHARS} символов"),
            population: total_docs,
            violations: body_bad,
            unit: "документов".to_string(),
        },
        CheckMetric {
            label: "Висячие ссылки related".to_string(),
            population: related_total,
            violations: dangling,
            unit: "ссылок".to_string(),
        },
        CheckMetric {
            label: "Якоря вне реестра сущностей".to_string(),
            population: entities_total,
            violations: unknown_anchors,
            unit: "якорей".to_string(),
        },
        CheckMetric {
            label: "Теги вне словаря".to_string(),
            population: tags_total,
            violations: unknown_tags,
            unit: "тегов".to_string(),
        },
    ];

    // Итог проверки — документы: «пять статей с дефектами из пятидесяти»
    // читается, а сумма разнородных единиц (документы + ссылки + теги) — нет.
    let broken_docs: std::collections::HashSet<&str> =
        defects.iter().map(|d| d.doc_id.as_str()).collect();
    let violations_total = broken_docs.len() as i64;

    // Порядок списка стабилен: иначе один и тот же корпус даёт разную выборку
    // от прогона к прогону, и «починили или нет» становится неотличимо.
    defects.sort_by(|a, b| a.doc_id.cmp(&b.doc_id).then(a.kind.cmp(b.kind)));
    let violations = defects
        .into_iter()
        .take(VIOLATION_SAMPLE_LIMIT)
        .map(|defect| ViolationItem {
            violation_type: defect.kind.to_string(),
            gl_id: None,
            projection_id: Some(defect.doc_id),
            projection_table: defect.source,
            detail: Some(defect.detail),
        })
        .collect();

    Ok(CheckResult {
        check_id: CHECK_ID.to_string(),
        run_at: chrono::Utc::now(),
        population_total: total_docs,
        violations_total,
        metrics,
        violations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Проверка обязана видеть корпус: пустая популяция означает, что она смотрит
    /// не туда, и «ноль нарушений» получается сам собой.
    ///
    /// В тестах в популяцию попадают только встроенные документы: `config.toml`
    /// ищется относительно рабочего каталога, а у теста это каталог крейта —
    /// путь падает на запасной `data/knowledge`, где лежат только
    /// материализованные копии embedded-доков. Курируемый корпус и словарь тегов
    /// проверяются на живом экземпляре, а не здесь.
    #[tokio::test]
    async fn runs_against_the_embedded_corpus() {
        let result = run().await.expect("проверка не падает");
        assert!(
            result.population_total > 0,
            "корпус пуст — проверка смотрит не в тот каталог"
        );
        assert_eq!(result.metrics.len(), 6);
        assert!(result.violations.len() <= VIOLATION_SAMPLE_LIMIT);
    }
}
