//! Эталонный набор вопросов («голден-сет») для регрессии качества LLM.
//!
//! Зачем: вердикты по живым диалогам показывают качество, но каждый день на разных
//! вопросах — по ним нельзя сказать, что именно изменила правка промпта. Здесь набор
//! **неизменных** вопросов с описанием ожидаемого ответа: прогнали до правки,
//! прогнали после, сравнили. Это дешёвый аналог CI для навыков и промптов.
//!
//! Кейсы — файлы, а не строки в БД: их правят вместе с навыками, читают глазами
//! и держат в системе контроля версий, как и сами навыки.

use super::frontmatter::{parse_list, parse_scalar, split_frontmatter};
use contracts::domain::a017_llm_agent::aggregate::AgentType;

#[derive(Debug, Clone)]
pub struct GoldenCase {
    /// Стабильный идентификатор кейса (имя файла без расширения).
    pub id: String,
    pub title: String,
    /// Вопрос, который задаётся агенту дословно.
    pub question: String,
    /// Специализация сотрудника, от лица которого прогоняется кейс.
    pub agent_type: AgentType,
    /// Признаки правильного ответа: то, с чем судья будет сверять результат.
    pub expect: Vec<String>,
    /// Свободное пояснение из тела файла — контекст для судьи.
    pub notes: String,
}

/// Прочитать каталог кейсов. Отсутствие каталога — не ошибка: голден-сет
/// заводится постепенно, и пустой набор просто означает «прогонять нечего».
pub fn load_cases() -> Result<Vec<GoldenCase>, String> {
    let config = crate::shared::config::load_config()
        .map_err(|error| format!("конфиг недоступен: {error}"))?;
    let dir = crate::shared::config::get_golden_set_path(&config);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }

    let entries =
        std::fs::read_dir(&dir).map_err(|error| format!("{}: {error}", dir.display()))?;

    let mut cases = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let raw = match std::fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(error) => {
                tracing::warn!("[golden_set] {}: {}", path.display(), error);
                continue;
            }
        };

        match parse_case(stem, &raw) {
            Ok(case) => cases.push(case),
            Err(error) => {
                // Битый кейс пропускаем с предупреждением: один неверный файл не
                // должен отменять прогон остальных.
                tracing::warn!("[golden_set] {}: {}", path.display(), error);
            }
        }
    }

    cases.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(cases)
}

fn parse_case(id: &str, raw: &str) -> Result<GoldenCase, String> {
    let (frontmatter, body) = split_frontmatter(raw);
    let frontmatter = frontmatter.ok_or("нет frontmatter")?;

    let question = parse_scalar(&frontmatter, "question")
        .ok_or("обязательное поле 'question' отсутствует")?;
    let expect = parse_list(&frontmatter, "expect").unwrap_or_default();
    if expect.is_empty() {
        // Кейс без признаков правильного ответа не проверяем: судье не с чем
        // сверять, и вердикт выродится в вкусовщину.
        return Err("обязательное поле 'expect' пустое".to_string());
    }

    let agent_type = parse_scalar(&frontmatter, "agent_type")
        .map(|value| AgentType::from_str(&value))
        .unwrap_or_default();

    Ok(GoldenCase {
        id: id.to_string(),
        title: parse_scalar(&frontmatter, "title").unwrap_or_else(|| id.to_string()),
        question,
        agent_type,
        expect,
        notes: body.trim().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_case() {
        let raw = "---\n\
                   title: Выручка за месяц\n\
                   question: Какая выручка по Wildberries за май 2026?\n\
                   agent_type: sales_analyst\n\
                   expect: [сумма в рублях, указан кабинет, период совпадает с запрошенным]\n\
                   ---\n\
                   Проверяем, что агент не подменяет период календарным месяцем.\n";
        let case = parse_case("revenue-month", raw).unwrap();
        assert_eq!(case.id, "revenue-month");
        assert_eq!(case.expect.len(), 3);
        assert!(matches!(case.agent_type, AgentType::SalesAnalyst));
        assert!(case.notes.starts_with("Проверяем"));
    }

    /// Кейс без ожиданий бесполезен: сверять не с чем, вердикт стал бы вкусовщиной.
    #[test]
    fn rejects_case_without_expectations() {
        let raw = "---\nquestion: Что-нибудь про продажи?\n---\n";
        assert!(parse_case("vague", raw).is_err());
    }

    #[test]
    fn rejects_case_without_question() {
        let raw = "---\nexpect: [что-то]\n---\n";
        assert!(parse_case("no-question", raw).is_err());
    }
}
