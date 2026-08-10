//! Рабочий каталог чата: активности, анкеты, планы, журнал шагов.
//!
//! Зачем: история диалога живёт в токен-бюджете и компактится
//! (`history_budget` от `context_window` подключения), а результаты инструментов
//! вообще не переживают ход —
//! контекст пересобирается из текста сообщений. Файл от бюджета не зависит:
//! план и анкета читаются заново каждый ход и подставляются в промпт.
//!
//! Единица работы — **активность**, а не чат. Чат живёт долго и содержит несколько
//! задач («сверь выручку за Q2», затем «построй график воронки»); одна пара
//! intake/plan на чат означала бы, что вторая задача затирает первую.
//!
//! ```text
//! <chat_files_path>/<chat_id>/
//!   current                       указатель на активную активность (одна строка)
//!   001-sverka-vyruchki-q2/
//!     intake.md  plan.md  notes.md
//!     steps/001-query-fina-oboroty.json
//!   002-grafik-voronki-wb/
//!     …
//! ```
//!
//! Две шкалы нумерации: активности на верхнем уровне, шаги — внутри своей
//! активности. Внутри активности имена фиксированы: отметить пункт плана
//! выполненным — это правка документа, а не новый документ.

use std::io;
use std::path::{Path, PathBuf};

// Формы каталога общие с фронтом: UI показывает те же задачи и файлы, что видит модель.
pub use contracts::domain::a018_llm_chat::workspace::{
    ChatActivity as ActivityRef, ChatFile as FileEntry, IntakeQuestion, PlanStep,
};

/// Живые документы активности — перезаписываются, не нумеруются.
pub const INTAKE_FILE: &str = "intake.md";
pub const PLAN_FILE: &str = "plan.md";
pub const NOTES_FILE: &str = "notes.md";

/// Единственные файлы, которые разрешено перезаписывать через `write_chat_file`
/// и править из UI. Журнал шагов — append-only по устройству.
pub const LIVE_DOCUMENTS: &[&str] = &[INTAKE_FILE, PLAN_FILE, NOTES_FILE];

const STEPS_DIR: &str = "steps";
const CURRENT_POINTER: &str = "current";

/// Потолок нумерации обеих шкал (активности и шаги).
const MAX_ORDINAL: u32 = 999;

/// Сколько символов описания оставляем в имени каталога/файла.
const MAX_SLUG_CHARS: usize = 48;

/// Тип шага — закрытый список, чтобы имена файлов оставались предсказуемыми
/// и по одному листингу было видно, из чего собран ответ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepKind {
    /// Результат выборки данных.
    Query,
    /// Производный расчёт по уже полученным данным.
    Calc,
    /// Черновик артефакта (спека графика/таблицы).
    Draft,
    /// Итог для пользователя.
    Report,
}

impl StepKind {
    pub fn as_str(self) -> &'static str {
        match self {
            StepKind::Query => "query",
            StepKind::Calc => "calc",
            StepKind::Draft => "draft",
            StepKind::Report => "report",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "query" => Some(StepKind::Query),
            "calc" => Some(StepKind::Calc),
            "draft" => Some(StepKind::Draft),
            "report" => Some(StepKind::Report),
            _ => None,
        }
    }

    pub const ALL: &'static [&'static str] = &["query", "calc", "draft", "report"];
}

// ─── Нормализация имён ───────────────────────────────────────────────────────

/// Слаг описания для имени каталога/файла.
///
/// Кириллицу сохраняем: каталог должен читаться человеком, который в него зашёл
/// проводником. Режем только то, что ломает путь или листинг.
fn slugify(raw: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in raw.trim().chars() {
        let mapped = if ch.is_alphanumeric() {
            Some(ch.to_lowercase().next().unwrap_or(ch))
        } else if ch == '-' || ch == '_' || ch.is_whitespace() {
            Some('-')
        } else {
            None
        };
        match mapped {
            Some('-') => {
                if !prev_dash && !out.is_empty() {
                    out.push('-');
                    prev_dash = true;
                }
            }
            Some(c) => {
                out.push(c);
                prev_dash = false;
            }
            None => {}
        }
        if out.chars().count() >= MAX_SLUG_CHARS {
            break;
        }
    }
    // Обрезка по границе слова: посимвольный лимит давал хвосты вида
    // «…динамика-и-от» — каталог должен читаться человеком.
    let trimmed = out.trim_matches('-');
    let trimmed = match trimmed.rsplit_once('-') {
        Some((head, _)) if trimmed.chars().count() >= MAX_SLUG_CHARS && !head.is_empty() => head,
        _ => trimmed,
    };
    if trimmed.is_empty() {
        "bez-nazvaniya".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Нормализация имени файла, пришедшего от модели.
///
/// Цель — не дать опечаткой уйти из каталога, а не выстроить песочницу:
/// разрешаем один уровень вложенности (`steps/…`), режем `..` и разделители дисков.
fn safe_relative(raw: &str) -> Option<String> {
    let cleaned = raw.trim().replace('\\', "/");
    if cleaned.is_empty() || cleaned.starts_with('/') || cleaned.contains(':') {
        return None;
    }
    let segments: Vec<&str> = cleaned
        .split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .collect();
    if segments.is_empty() || segments.len() > 2 || segments.iter().any(|s| *s == "..") {
        return None;
    }
    Some(segments.join("/"))
}

/// Разобрать имя каталога активности: `002-grafik-voronki` → (2, "grafik-voronki").
fn parse_ordinal_name(name: &str) -> Option<(u32, String)> {
    let (head, rest) = name.split_once('-')?;
    if head.len() != 3 || !head.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let ordinal = head.parse::<u32>().ok()?;
    Some((ordinal, rest.to_string()))
}

// ─── Корень чата ─────────────────────────────────────────────────────────────

/// Корень рабочего каталога одного чата: активности + указатель `current`.
pub struct ChatWorkspace {
    root: PathBuf,
}

impl ChatWorkspace {
    /// Каталог чата под корнем из конфига. `None`, если конфиг недоступен —
    /// инструменты в этом случае отвечают внятной ошибкой, а не паникуют.
    pub fn for_chat(chat_id: &str) -> Option<Self> {
        let cfg = crate::shared::config::load_config()
            .map_err(|e| tracing::warn!("[chat_workspace] конфиг не загружен: {e}"))
            .ok()?;
        let id = safe_relative(chat_id)?;
        if id.contains('/') {
            return None;
        }
        Some(Self {
            root: crate::shared::config::get_chat_files_path(&cfg).join(id),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Активности по возрастанию номера. Пустой/несуществующий каталог — пустой список.
    pub async fn list_activities(&self) -> io::Result<Vec<ActivityRef>> {
        let mut entries = match tokio::fs::read_dir(&self.root).await {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        let mut found: Vec<(u32, String, String)> = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            if !entry.file_type().await?.is_dir() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(String::from) else {
                continue;
            };
            if let Some((ordinal, description)) = parse_ordinal_name(&name) {
                found.push((ordinal, name, description));
            }
        }
        found.sort_by_key(|(ordinal, _, _)| *ordinal);

        let pointer = self.read_pointer().await;
        let active_name = resolve_active_name(pointer.as_deref(), &found);
        Ok(found
            .into_iter()
            .map(|(ordinal, name, description)| ActivityRef {
                is_active: Some(&name) == active_name.as_ref(),
                name,
                ordinal,
                description,
            })
            .collect())
    }

    /// Активная активность: `current`, при его отсутствии или битой ссылке —
    /// активность с наибольшим номером.
    pub async fn active(&self) -> io::Result<Option<Activity>> {
        let activities = self.list_activities().await?;
        Ok(activities
            .into_iter()
            .find(|a| a.is_active)
            .map(|a| self.activity(&a.name)))
    }

    /// Активность по имени каталога (без проверки существования — её делает вызывающий).
    pub fn activity(&self, name: &str) -> Activity {
        Activity {
            dir: self.root.join(name),
            name: name.to_string(),
        }
    }

    /// Завести новую активность и сделать её активной. Номер присваивает бэкенд:
    /// модель иначе вынуждена делать list → инкремент → write, а это лишний
    /// round-trip и гонка при нескольких вызовах на одной итерации.
    pub async fn start_activity(&self, description: &str) -> io::Result<Activity> {
        let existing = self.list_activities().await?;
        let next = existing.last().map(|a| a.ordinal).unwrap_or(0) + 1;
        if next > MAX_ORDINAL {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Достигнут потолок в {MAX_ORDINAL} активностей на чат"),
            ));
        }
        let name = format!("{:03}-{}", next, slugify(description));
        let activity = self.activity(&name);
        tokio::fs::create_dir_all(activity.dir.join(STEPS_DIR)).await?;
        self.set_active(&name).await?;
        Ok(activity)
    }

    /// Переключить указатель на существующую активность.
    pub async fn set_active(&self, name: &str) -> io::Result<()> {
        tokio::fs::create_dir_all(&self.root).await?;
        tokio::fs::write(self.root.join(CURRENT_POINTER), name).await
    }

    /// Активная активность, а если активностей нет — завести первую.
    ///
    /// Ленивое создание намеренно: требовать от модели явного `start_activity`
    /// перед любой записью значит ломать всё остальное на забытом вызове.
    pub async fn ensure_active(&self, fallback_description: &str) -> io::Result<Activity> {
        match self.active().await? {
            Some(activity) => Ok(activity),
            None => self.start_activity(fallback_description).await,
        }
    }

    async fn read_pointer(&self) -> Option<String> {
        tokio::fs::read_to_string(self.root.join(CURRENT_POINTER))
            .await
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    /// Блок для системного промпта: активная задача целиком, остальные — именами.
    ///
    /// `None` на пустом каталоге — не засорять промпт на приветствиях.
    pub async fn render_for_prompt(&self) -> Option<String> {
        let activities = self.list_activities().await.ok()?;
        if activities.is_empty() {
            return None;
        }
        let active = activities.iter().find(|a| a.is_active)?;
        let activity = self.activity(&active.name);

        let mut out = format!("Рабочий каталог. Активная задача: {}\n", active.name);
        for doc in LIVE_DOCUMENTS {
            let body = activity.read_to_string(doc).await;
            match body {
                Some(text) if !text.trim().is_empty() => {
                    out.push_str(&format!("--- {} ---\n{}\n", doc, text.trim_end()));
                }
                // Про отсутствующие анкету и план говорим прямо: это подсказка их завести.
                _ if *doc == NOTES_FILE => {}
                _ => out.push_str(&format!("--- {} --- (не заполнен)\n", doc)),
            }
        }

        let steps = activity.list_steps().await.unwrap_or_default();
        if steps.is_empty() {
            out.push_str("Шагов пока нет.\n");
        } else {
            out.push_str(&format!("Шаги: {}\n", steps.join(", ")));
        }

        let others: Vec<&str> = activities
            .iter()
            .filter(|a| !a.is_active)
            .map(|a| a.name.as_str())
            .collect();
        if !others.is_empty() {
            out.push_str(&format!(
                "Другие задачи в этом чате: {}. Читать их файлы — read_chat_file(\"<задача>/plan.md\"), \
                 вернуться к задаче — switch_activity(\"<задача>\").\n",
                others.join(", ")
            ));
        }
        Some(out)
    }
}

// ─── Уточняющие вопросы анкеты ───────────────────────────────────────────────

/// Разобрать вопросы и ответы из frontmatter анкеты.
///
/// Читаем настоящим YAML (структура вложенная), а вот пишем ответ текстовой
/// заплаткой — см. `upsert_answer`. Кривой frontmatter не ошибка: просто нет
/// вопросов.
pub fn parse_questions(intake: &str) -> Vec<IntakeQuestion> {
    let (Some(frontmatter), _) = super::frontmatter::split_frontmatter(intake) else {
        return Vec::new();
    };
    let Ok(doc) = serde_yaml::from_str::<serde_yaml::Value>(&frontmatter) else {
        return Vec::new();
    };

    let answers = doc.get("answers").and_then(|v| v.as_mapping());
    let answer_of = |id: &str| -> Option<String> {
        answers
            .and_then(|m| m.get(serde_yaml::Value::String(id.to_string())))
            .and_then(|v| match v {
                serde_yaml::Value::String(s) => Some(s.clone()),
                serde_yaml::Value::Bool(b) => Some(b.to_string()),
                serde_yaml::Value::Number(n) => Some(n.to_string()),
                _ => None,
            })
            .filter(|s| !s.trim().is_empty())
    };

    let Some(items) = doc.get("questions").and_then(|v| v.as_sequence()) else {
        return Vec::new();
    };
    items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            // Терпим и старую форму — простую строку: модель писала так,
            // пока схема не была описана в промпте.
            if let Some(text) = item.as_str() {
                let id = format!("q{}", index + 1);
                let answer = answer_of(&id);
                return Some(IntakeQuestion {
                    id,
                    text: text.to_string(),
                    options: Vec::new(),
                    answer,
                });
            }
            let map = item.as_mapping()?;
            let get = |key: &str| {
                map.get(serde_yaml::Value::String(key.to_string()))
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            };
            let text = get("text").or_else(|| get("question"))?;
            let id = get("id")
                .or_else(|| get("field"))
                .unwrap_or_else(|| format!("q{}", index + 1));
            let options = map
                .get(serde_yaml::Value::String("options".to_string()))
                .and_then(|v| v.as_sequence())
                .map(|seq| {
                    seq.iter()
                        .filter_map(|o| o.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let answer = answer_of(&id);
            Some(IntakeQuestion {
                id,
                text,
                options,
                answer,
            })
        })
        .collect()
}

/// Вписать ответ в блок `answers:` анкеты, не трогая остальной файл.
///
/// Именно заплатка, а не переписывание YAML: в анкете живут комментарии модели
/// (например расшифровки id кабинетов), и пересериализация их бы потеряла.
pub fn upsert_answer(intake: &str, question_id: &str, answer: &str) -> String {
    let (frontmatter, body) = super::frontmatter::split_frontmatter(intake);
    let frontmatter = frontmatter.unwrap_or_default();

    let mut answers: Vec<(String, String)> = Vec::new();
    let mut kept: Vec<&str> = Vec::new();
    let mut in_answers = false;

    for line in frontmatter.lines() {
        let is_top_level_key = !line.starts_with([' ', '\t', '-']) && line.contains(':');
        if in_answers {
            // Блок продолжается, пока строки вложены глубже верхнего уровня.
            if line.trim().is_empty() || !is_top_level_key {
                if let Some((k, v)) = line.trim().split_once(':') {
                    let key = k.trim().trim_matches('"').to_string();
                    let value = v.trim().trim_matches('"').to_string();
                    if !key.is_empty() {
                        answers.push((key, value));
                    }
                }
                continue;
            }
            in_answers = false;
        }
        // Ключ `answers` в любой форме: блоком, пустым инлайном (`answers: {}`)
        // или с инлайновой картой. Не распознав инлайн, мы дописывали второй
        // ключ `answers` — YAML с дублем не парсится, и вопросы пропадали из UI.
        if is_top_level_key && line.split(':').next().map(str::trim) == Some("answers") {
            let inline = line.split_once(':').map(|(_, v)| v.trim()).unwrap_or("");
            for pair in inline
                .trim_start_matches('{')
                .trim_end_matches('}')
                .split(',')
            {
                if let Some((k, v)) = pair.split_once(':') {
                    let key = k.trim().trim_matches(['"', '\'']).to_string();
                    let value = v.trim().trim_matches(['"', '\'']).to_string();
                    if !key.is_empty() {
                        answers.push((key, value));
                    }
                }
            }
            // Блок продолжается на следующих строках только у формы `answers:`.
            in_answers = inline.is_empty();
            continue;
        }
        kept.push(line);
    }

    match answers.iter_mut().find(|(k, _)| k == question_id) {
        Some((_, value)) => *value = answer.to_string(),
        None => answers.push((question_id.to_string(), answer.to_string())),
    }

    let mut out = String::from("---\n");
    for line in kept {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("answers:\n");
    for (key, value) in answers {
        // Кавычки: ответ пользователя — произвольный текст, в нём бывает двоеточие.
        out.push_str(&format!("  {}: \"{}\"\n", key, value.replace('"', "'")));
    }
    out.push_str("---\n");
    out.push_str(&body);
    out
}

/// Записать ответ на вопрос в анкету активной задачи.
pub async fn answer_question(chat_id: &str, question_id: &str, answer: &str) -> io::Result<()> {
    let ws = ChatWorkspace::for_chat(chat_id)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Каталог чата недоступен"))?;
    let activity = ws
        .active()
        .await?
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Активной задачи нет"))?;
    let (current, _) = activity.read(INTAKE_FILE, 0, usize::MAX).await?;
    activity
        .write(INTAKE_FILE, &upsert_answer(&current, question_id, answer))
        .await
}

// ─── План: разбор и правка статусов ──────────────────────────────────────────

/// Разобрать план в список пунктов.
///
/// Формат остаётся markdown-чекбоксами — так план уже пишет модель (см. `core.md`)
/// и правит человек из UI. Пункты нумеруются позиционно (`s1`, `s2`, …): номер в
/// самом файле пришлось бы синхронизировать при каждой вставке строки.
///
/// Ссылка на шаг журнала распознаётся в тексте пункта по имени файла — модель и
/// так упоминает его («см. steps/003-calc-margin.json»), отдельного синтаксиса
/// для этого вводить не нужно.
pub fn parse_plan_steps(plan: &str) -> Vec<PlanStep> {
    let mut steps = Vec::new();
    for line in plan.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        else {
            continue;
        };
        let (done, title) = if let Some(t) = rest
            .strip_prefix("[x]")
            .or_else(|| rest.strip_prefix("[X]"))
        {
            (true, t)
        } else if let Some(t) = rest.strip_prefix("[ ]") {
            (false, t)
        } else {
            continue;
        };
        let title = title.trim().to_string();
        if title.is_empty() {
            continue;
        }
        steps.push(PlanStep {
            id: format!("s{}", steps.len() + 1),
            step_ref: extract_step_ref(&title),
            title,
            done,
        });
    }
    steps
}

/// Вытащить имя файла журнала шагов из текста пункта.
///
/// Принимаем и полный путь (`steps/003-…json`), и голое имя файла: в тексте
/// модель пишет то так, то иначе, а сверять нужно одно и то же.
fn extract_step_ref(title: &str) -> Option<String> {
    title
        .split(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | ',' | ';' | '«' | '»' | '"'))
        .filter_map(|token| {
            let token = token.trim_matches(|c: char| matches!(c, '.' | ':' | '`'));
            let name = token.rsplit('/').next()?;
            (name.ends_with(".json") && name.len() > 5).then(|| name.to_string())
        })
        .next()
}

/// Переставить статус пункта плана, не трогая остальной файл.
///
/// Заплатка по строке, а не пересборка markdown: в плане живут заголовки,
/// комментарии и вложенные уточнения модели — пересериализация их потеряет.
pub fn set_step_status(plan: &str, step_id: &str, done: bool) -> Option<String> {
    let mut ordinal = 0usize;
    let mut found = false;
    let mut out = String::with_capacity(plan.len() + 8);

    for line in plan.lines() {
        let trimmed = line.trim_start();
        let is_item = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
            .map(|rest| {
                rest.starts_with("[x]") || rest.starts_with("[X]") || rest.starts_with("[ ]")
            })
            .unwrap_or(false);

        if is_item {
            ordinal += 1;
            if format!("s{ordinal}") == step_id {
                let indent_len = line.len() - trimmed.len();
                let (indent, rest) = line.split_at(indent_len);
                let marker = if done { "[x]" } else { "[ ]" };
                // Первые два символа — маркер списка («- » или «* »), затем чекбокс.
                let (bullet, body) = rest.split_at(2);
                let body = body
                    .strip_prefix("[x]")
                    .or_else(|| body.strip_prefix("[X]"))
                    .or_else(|| body.strip_prefix("[ ]"))
                    .unwrap_or(body);
                out.push_str(indent);
                out.push_str(bullet);
                out.push_str(marker);
                out.push_str(body);
                out.push('\n');
                found = true;
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }

    found.then_some(out)
}

/// Пункты, отмеченные выполненными, которые ссылаются на несуществующий шаг журнала.
///
/// Проверяется только явная ссылка: требовать файл от КАЖДОГО закрытого пункта
/// нельзя — часть шагов плана не производит данных вовсе («уточнить период у
/// пользователя»), и такое правило дало бы поток ложных срабатываний. Зато
/// выдуманное имя файла — однозначный признак того, что работа не сделана.
pub fn plan_drift(steps: &[PlanStep], journal: &[String]) -> Vec<PlanStep> {
    let known: Vec<&str> = journal
        .iter()
        .filter_map(|path| path.rsplit('/').next())
        .collect();
    steps
        .iter()
        .filter(|step| step.done)
        .filter(|step| {
            step.step_ref
                .as_deref()
                .is_some_and(|name| !known.contains(&name))
        })
        .cloned()
        .collect()
}

/// Пункты плана и расхождения по активной задаче чата.
pub async fn plan_state(chat_id: &str) -> io::Result<(Vec<PlanStep>, Vec<PlanStep>)> {
    let Some(ws) = ChatWorkspace::for_chat(chat_id) else {
        return Ok((Vec::new(), Vec::new()));
    };
    let Some(activity) = ws.active().await? else {
        return Ok((Vec::new(), Vec::new()));
    };
    let Ok((plan, _)) = activity.read(PLAN_FILE, 0, usize::MAX).await else {
        return Ok((Vec::new(), Vec::new()));
    };
    let steps = parse_plan_steps(&plan);
    let journal = activity.list_steps().await.unwrap_or_default();
    let drift = plan_drift(&steps, &journal);
    Ok((steps, drift))
}

/// Переставить статус пункта плана активной задачи.
pub async fn update_plan_step(
    chat_id: &str,
    step_id: &str,
    done: bool,
) -> io::Result<Vec<PlanStep>> {
    let ws = ChatWorkspace::for_chat(chat_id)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Каталог чата недоступен"))?;
    let activity = ws
        .active()
        .await?
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Активной задачи нет"))?;
    let (plan, _) = activity.read(PLAN_FILE, 0, usize::MAX).await?;
    let updated = set_step_status(&plan, step_id, done).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("Пункта '{step_id}' в плане нет"),
        )
    })?;
    activity.write(PLAN_FILE, &updated).await?;
    Ok(parse_plan_steps(&updated))
}

// ─── Доступ для UI ───────────────────────────────────────────────────────────

/// Каталог чата для UI: задачи, файлы активной, её уточняющие вопросы и план.
pub async fn view_for_chat(
    chat_id: &str,
) -> io::Result<(
    Vec<ActivityRef>,
    Vec<FileEntry>,
    Vec<IntakeQuestion>,
    Vec<PlanStep>,
)> {
    let Some(ws) = ChatWorkspace::for_chat(chat_id) else {
        return Ok((Vec::new(), Vec::new(), Vec::new(), Vec::new()));
    };
    let activities = ws.list_activities().await?;
    let (files, questions, plan_steps) = match ws.active().await? {
        Some(activity) => {
            let files = activity.list().await.unwrap_or_default();
            let questions = match activity.read(INTAKE_FILE, 0, usize::MAX).await {
                Ok((intake, _)) => parse_questions(&intake),
                Err(_) => Vec::new(),
            };
            let plan_steps = match activity.read(PLAN_FILE, 0, usize::MAX).await {
                Ok((plan, _)) => parse_plan_steps(&plan),
                Err(_) => Vec::new(),
            };
            (files, questions, plan_steps)
        }
        None => (Vec::new(), Vec::new(), Vec::new()),
    };
    Ok((activities, files, questions, plan_steps))
}

/// Разобрать путь вида `<задача>/<файл>` (файл может быть `steps/NNN-…`).
fn split_activity_path(path: &str) -> Option<(String, String)> {
    let cleaned = path.trim().replace('\\', "/");
    let (activity, rest) = cleaned.split_once('/')?;
    if activity.is_empty() || rest.is_empty() {
        return None;
    }
    Some((activity.to_string(), rest.to_string()))
}

/// Прочитать файл каталога целиком (для UI — окно там не нужно).
pub async fn read_file(chat_id: &str, path: &str) -> io::Result<(String, bool)> {
    let ws = ChatWorkspace::for_chat(chat_id)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Каталог чата недоступен"))?;
    let (activity_name, rel) = split_activity_path(path)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Ожидается <задача>/<файл>"))?;
    let activity = ws.activity(&activity_name);
    let (content, _) = activity.read(&rel, 0, usize::MAX).await?;
    Ok((content, LIVE_DOCUMENTS.contains(&rel.as_str())))
}

/// Правка живого документа из UI. Анкету быстрее поправить формой, чем диалогом.
pub async fn write_file(chat_id: &str, path: &str, content: &str) -> io::Result<()> {
    let ws = ChatWorkspace::for_chat(chat_id)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Каталог чата недоступен"))?;
    let (activity_name, rel) = split_activity_path(path)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Ожидается <задача>/<файл>"))?;
    ws.activity(&activity_name).write(&rel, content).await
}

/// Переключить активную задачу из UI.
pub async fn switch_activity(chat_id: &str, name: &str) -> io::Result<()> {
    let ws = ChatWorkspace::for_chat(chat_id)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Каталог чата недоступен"))?;
    let activities = ws.list_activities().await?;
    if !activities.iter().any(|a| a.name == name) {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Задачи '{name}' в этом чате нет"),
        ));
    }
    ws.set_active(name).await
}

/// Указатель `current`, либо активность с наибольшим номером.
/// Битая ссылка не должна ломать чат — это тот же fallback.
fn resolve_active_name(pointer: Option<&str>, found: &[(u32, String, String)]) -> Option<String> {
    if let Some(name) = pointer {
        if found.iter().any(|(_, dir, _)| dir == name) {
            return Some(name.to_string());
        }
    }
    found.last().map(|(_, name, _)| name.clone())
}

// ─── Каталог одной активности ────────────────────────────────────────────────

/// Каталог одной задачи: живые документы + журнал шагов.
pub struct Activity {
    dir: PathBuf,
    name: String,
}

impl Activity {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Вложенный каталог внутри активности — задел под субагентов
    /// (`agents/<name>`): те же файловые операции работают без изменений.
    pub fn child(&self, rel: &str) -> Option<Activity> {
        let rel = safe_relative(rel)?;
        Some(Activity {
            dir: self.dir.join(&rel),
            name: format!("{}/{}", self.name, rel),
        })
    }

    /// Чтение с окном: смещение и потолок в символах, плюс флаг обрезки —
    /// как в `read_skill_resource`, чтобы модель не считала выборку полной.
    pub async fn read(
        &self,
        name: &str,
        offset: usize,
        max_chars: usize,
    ) -> io::Result<(String, bool)> {
        let rel = safe_relative(name)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Некорректное имя файла"))?;
        let body = tokio::fs::read_to_string(self.dir.join(&rel)).await?;
        let mut chars = body.chars().skip(offset);
        let window: String = chars.by_ref().take(max_chars).collect();
        let truncated = chars.next().is_some();
        Ok((window, truncated))
    }

    /// Перезапись живого документа. Журнал шагов сюда не попадает.
    pub async fn write(&self, name: &str, content: &str) -> io::Result<()> {
        let rel = safe_relative(name)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Некорректное имя файла"))?;
        if !LIVE_DOCUMENTS.contains(&rel.as_str()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "Перезаписывать можно только {}. Результат работы сохраняй через save_step.",
                    LIVE_DOCUMENTS.join(", ")
                ),
            ));
        }
        tokio::fs::create_dir_all(&self.dir).await?;
        tokio::fs::write(self.dir.join(&rel), content).await
    }

    /// Новый шаг журнала. Возвращает фактическое имя файла — номер присваивает бэкенд.
    pub async fn save_step(
        &self,
        kind: StepKind,
        description: &str,
        content: &str,
        ext: &str,
    ) -> io::Result<String> {
        let steps_dir = self.dir.join(STEPS_DIR);
        tokio::fs::create_dir_all(&steps_dir).await?;
        let next = self.next_step_ordinal().await?;
        if next > MAX_ORDINAL {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Достигнут потолок в {MAX_ORDINAL} шагов на задачу"),
            ));
        }
        let ext = {
            let cleaned: String = ext
                .trim()
                .trim_start_matches('.')
                .chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .collect();
            if cleaned.is_empty() {
                "json".to_string()
            } else {
                cleaned.to_ascii_lowercase()
            }
        };
        let file = format!(
            "{:03}-{}-{}.{}",
            next,
            kind.as_str(),
            slugify(description),
            ext
        );
        tokio::fs::write(steps_dir.join(&file), content).await?;
        Ok(format!("{}/{}", STEPS_DIR, file))
    }

    /// Имена шагов по возрастанию номера. Имена самодокументируемы, поэтому
    /// в промпт уходит список имён, а не содержимое.
    pub async fn list_steps(&self) -> io::Result<Vec<String>> {
        let mut entries = match tokio::fs::read_dir(self.dir.join(STEPS_DIR)).await {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        let mut found: Vec<(u32, String)> = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let Some(name) = entry.file_name().to_str().map(String::from) else {
                continue;
            };
            let ordinal = parse_ordinal_name(&name).map(|(n, _)| n).unwrap_or(0);
            found.push((ordinal, name));
        }
        found.sort();
        Ok(found.into_iter().map(|(_, name)| name).collect())
    }

    /// Все файлы активности: живые документы + журнал.
    pub async fn list(&self) -> io::Result<Vec<FileEntry>> {
        let mut out = Vec::new();
        for doc in LIVE_DOCUMENTS {
            if let Ok(meta) = tokio::fs::metadata(self.dir.join(doc)).await {
                out.push(FileEntry {
                    path: (*doc).to_string(),
                    bytes: meta.len(),
                    is_live_document: true,
                });
            }
        }
        for step in self.list_steps().await? {
            let path = format!("{}/{}", STEPS_DIR, step);
            let bytes = tokio::fs::metadata(self.dir.join(&path))
                .await
                .map(|m| m.len())
                .unwrap_or(0);
            out.push(FileEntry {
                path,
                bytes,
                is_live_document: false,
            });
        }
        Ok(out)
    }

    /// Положить файл, только если его ещё нет. Возвращает `true`, если положили.
    ///
    /// Так навык приносит свой шаблон анкеты в новую задачу, не затирая уже
    /// заполненную пользователем или моделью.
    pub async fn seed_if_absent(&self, name: &str, content: &str) -> io::Result<bool> {
        let rel = safe_relative(name)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Некорректное имя файла"))?;
        let path = self.dir.join(&rel);
        if tokio::fs::metadata(&path).await.is_ok() {
            return Ok(false);
        }
        tokio::fs::create_dir_all(&self.dir).await?;
        tokio::fs::write(path, content).await?;
        Ok(true)
    }

    async fn read_to_string(&self, name: &str) -> Option<String> {
        tokio::fs::read_to_string(self.dir.join(name)).await.ok()
    }

    /// Следующий номер шага. Считаем от максимума, а не от количества: дыры в
    /// нумерации (удалённый вручную файл) не должны приводить к перезаписи.
    async fn next_step_ordinal(&self) -> io::Result<u32> {
        let max = self
            .list_steps()
            .await?
            .iter()
            .filter_map(|name| parse_ordinal_name(name).map(|(n, _)| n))
            .max()
            .unwrap_or(0);
        Ok(max + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Изолированный корень под тест: конфиг не трогаем.
    fn workspace(root: PathBuf) -> ChatWorkspace {
        ChatWorkspace { root }
    }

    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "chat_ws_{}_{}_{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    const PLAN: &str = "# План сверки\n\
                        \n\
                        - [x] Поднять обороты fina (steps/001-query-oboroty.json)\n\
                        - [ ] Сверить с ybuh\n\
                          - вложенное уточнение, не пункт\n\
                        * [x] Отчёт готов\n\
                        \n\
                        Просто абзац, тоже не пункт.\n";

    #[test]
    fn plan_parses_checkboxes_and_ignores_prose() {
        let steps = parse_plan_steps(PLAN);
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].id, "s1");
        assert!(steps[0].done);
        assert_eq!(steps[0].step_ref.as_deref(), Some("001-query-oboroty.json"));
        assert_eq!(steps[1].id, "s2");
        assert!(!steps[1].done);
        assert_eq!(steps[1].step_ref, None);
        // Маркер `*` — такой же пункт: модель пишет и так.
        assert!(steps[2].done);
    }

    #[test]
    fn step_status_patch_keeps_the_rest_of_the_file() {
        let updated = set_step_status(PLAN, "s2", true).unwrap();
        assert!(updated.contains("- [x] Сверить с ybuh"));
        // Соседние пункты и проза не тронуты.
        assert!(updated.contains("- [x] Поднять обороты fina"));
        assert!(updated.contains("вложенное уточнение, не пункт"));
        assert!(updated.contains("Просто абзац, тоже не пункт."));
        assert!(updated.contains("# План сверки"));

        // Обратный переход тоже работает, и повторная правка идемпотентна.
        let reverted = set_step_status(&updated, "s2", false).unwrap();
        assert!(reverted.contains("- [ ] Сверить с ybuh"));
        assert_eq!(set_step_status(&reverted, "s2", false).unwrap(), reverted);
    }

    #[test]
    fn unknown_step_id_is_rejected_rather_than_silently_ignored() {
        assert!(set_step_status(PLAN, "s99", true).is_none());
        assert!(set_step_status(PLAN, "мусор", true).is_none());
    }

    #[test]
    fn drift_is_a_reference_to_a_step_that_was_never_saved() {
        let steps = parse_plan_steps(PLAN);

        // Файл на месте — расхождения нет.
        let journal = vec!["steps/001-query-oboroty.json".to_string()];
        assert!(plan_drift(&steps, &journal).is_empty());

        // Журнал пуст: закрытый пункт ссылается на несуществующий шаг.
        let drift = plan_drift(&steps, &[]);
        assert_eq!(drift.len(), 1);
        assert_eq!(drift[0].id, "s1");
    }

    #[test]
    fn done_step_without_reference_is_not_drift() {
        // Не каждый пункт производит данные («уточнить период у пользователя»),
        // поэтому требовать файл от всех закрытых пунктов нельзя.
        let steps = parse_plan_steps("- [x] Уточнить период у пользователя\n");
        assert!(plan_drift(&steps, &[]).is_empty());
    }

    #[test]
    fn slug_keeps_cyrillic_and_collapses_separators() {
        assert_eq!(
            slugify("Сверка выручки  fina/ybuh за Q2"),
            "сверка-выручки-finaybuh-за-q2"
        );
        assert_eq!(slugify("   "), "bez-nazvaniya");
        assert_eq!(slugify("--Тест--"), "тест");
    }

    #[test]
    fn safe_relative_blocks_escapes_and_allows_one_level() {
        assert_eq!(safe_relative("plan.md").as_deref(), Some("plan.md"));
        assert_eq!(
            safe_relative("steps/001-query-a.json").as_deref(),
            Some("steps/001-query-a.json")
        );
        assert_eq!(safe_relative("../../etc/passwd"), None);
        assert_eq!(safe_relative("C:/windows/system32"), None);
        assert_eq!(safe_relative("/absolute"), None);
        assert_eq!(safe_relative("a/b/c"), None);
    }

    #[test]
    fn active_falls_back_to_highest_ordinal() {
        let found = vec![
            (1, "001-a".to_string(), "a".to_string()),
            (2, "002-b".to_string(), "b".to_string()),
        ];
        // Указателя нет.
        assert_eq!(resolve_active_name(None, &found).as_deref(), Some("002-b"));
        // Указатель битый — тот же fallback, а не паника и не пустота.
        assert_eq!(
            resolve_active_name(Some("003-udalili"), &found).as_deref(),
            Some("002-b")
        );
        // Валидный указатель уважаем: пользователь мог вернуться к прежней задаче.
        assert_eq!(
            resolve_active_name(Some("001-a"), &found).as_deref(),
            Some("001-a")
        );
        assert_eq!(resolve_active_name(Some("001-a"), &[]), None);
    }

    #[tokio::test]
    async fn activities_are_numbered_and_switchable() {
        let ws = workspace(temp_root("activities"));

        let first = ws.start_activity("Сверка выручки Q2").await.unwrap();
        assert_eq!(first.name(), "001-сверка-выручки-q2");
        let second = ws.start_activity("График воронки WB").await.unwrap();
        assert_eq!(second.name(), "002-график-воронки-wb");

        // Новая активность становится активной.
        assert_eq!(
            ws.active().await.unwrap().unwrap().name(),
            "002-график-воронки-wb"
        );

        // Возврат к прежней задаче.
        ws.set_active("001-сверка-выручки-q2").await.unwrap();
        assert_eq!(
            ws.active().await.unwrap().unwrap().name(),
            "001-сверка-выручки-q2"
        );

        let listed = ws.list_activities().await.unwrap();
        assert_eq!(listed.len(), 2);
        assert!(listed[0].is_active);
        assert!(!listed[1].is_active);
    }

    #[tokio::test]
    async fn ensure_active_creates_first_activity_lazily() {
        let ws = workspace(temp_root("lazy"));
        assert!(ws.active().await.unwrap().is_none());
        let activity = ws.ensure_active("Разбор воронки").await.unwrap();
        assert_eq!(activity.name(), "001-разбор-воронки");
        // Повторный вызов не плодит активности.
        let again = ws.ensure_active("что-то другое").await.unwrap();
        assert_eq!(again.name(), "001-разбор-воронки");
        assert_eq!(ws.list_activities().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn steps_are_numbered_per_activity_and_survive_gaps() {
        let ws = workspace(temp_root("steps"));
        let a = ws.start_activity("Задача А").await.unwrap();
        let b = ws.start_activity("Задача Б").await.unwrap();

        let s1 = a
            .save_step(StepKind::Query, "обороты fina", "{}", "json")
            .await
            .unwrap();
        let s2 = a
            .save_step(StepKind::Calc, "дельта по кабинетам", "{}", "json")
            .await
            .unwrap();
        assert_eq!(s1, "steps/001-query-обороты-fina.json");
        assert_eq!(s2, "steps/002-calc-дельта-по-кабинетам.json");

        // Нумерация шагов своя в каждой активности.
        let b1 = b
            .save_step(StepKind::Draft, "спека графика", "{}", "json")
            .await
            .unwrap();
        assert_eq!(b1, "steps/001-draft-спека-графика.json");

        // Дыра в нумерации не приводит к перезаписи: считаем от максимума.
        tokio::fs::remove_file(a.dir().join(&s1)).await.unwrap();
        let s3 = a
            .save_step(StepKind::Report, "итог", "текст", "md")
            .await
            .unwrap();
        assert_eq!(s3, "steps/003-report-итог.md");
    }

    #[tokio::test]
    async fn write_accepts_only_live_documents() {
        let ws = workspace(temp_root("write"));
        let a = ws.start_activity("Задача").await.unwrap();
        assert!(a.write(PLAN_FILE, "- [ ] шаг").await.is_ok());
        // Журнал шагов append-only: перезаписать его нельзя.
        assert!(a.write("steps/001-query-a.json", "{}").await.is_err());
        assert!(a.write("../escape.md", "x").await.is_err());
    }

    #[tokio::test]
    async fn read_reports_truncation() {
        let ws = workspace(temp_root("read"));
        let a = ws.start_activity("Задача").await.unwrap();
        a.write(NOTES_FILE, "абвгде").await.unwrap();

        let (window, truncated) = a.read(NOTES_FILE, 0, 3).await.unwrap();
        assert_eq!(window, "абв");
        assert!(truncated);

        let (rest, truncated) = a.read(NOTES_FILE, 3, 100).await.unwrap();
        assert_eq!(rest, "где");
        assert!(!truncated);
    }

    const INTAKE_WITH_QUESTIONS: &str = r#"---
period_from: 2025-08-01
connections_mp_refs:
  - 1386a311-1e26-4676-b696-8d577a119eec # WB - SANSTAR
questions:
  - id: breakdown
    text: По каждому кабинету отдельно или суммарно?
    options: [по кабинетам, суммарно]
  - id: focus
    text: Абсолюты или конверсии?
---

Тело анкеты.
"#;

    #[test]
    fn questions_parse_with_and_without_options() {
        let questions = parse_questions(INTAKE_WITH_QUESTIONS);
        assert_eq!(questions.len(), 2);
        assert_eq!(questions[0].id, "breakdown");
        assert_eq!(questions[0].options, vec!["по кабинетам", "суммарно"]);
        assert!(questions[0].answer.is_none());
        // Без options — UI нарисует поле ввода.
        assert!(questions[1].options.is_empty());
    }

    /// Модель писала вопросы простым списком строк до того, как схема попала
    /// в промпт. Такие анкеты должны продолжать работать.
    #[test]
    fn plain_string_questions_are_still_understood() {
        let intake = "---\nquestions:\n  - Суммарно или по кабинетам?\n---\n";
        let questions = parse_questions(intake);
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].id, "q1");
        assert!(questions[0].options.is_empty());
    }

    /// Ответ пишется заплаткой: комментарии модели в анкете должны уцелеть.
    #[test]
    fn answering_preserves_the_rest_of_the_intake() {
        let patched = upsert_answer(INTAKE_WITH_QUESTIONS, "breakdown", "по кабинетам");
        assert!(patched.contains("# WB - SANSTAR"), "потерян комментарий");
        assert!(patched.contains("period_from: 2025-08-01"));
        assert!(patched.contains("Тело анкеты."));
        assert!(patched.contains("answers:"));

        let questions = parse_questions(&patched);
        assert_eq!(questions[0].answer.as_deref(), Some("по кабинетам"));
        assert!(questions[1].answer.is_none());

        // Повторный ответ заменяет прежний, а не плодит дубли.
        let again = upsert_answer(&patched, "breakdown", "суммарно");
        assert_eq!(again.matches("breakdown:").count(), 1);
        assert_eq!(
            parse_questions(&again)[0].answer.as_deref(),
            Some("суммарно")
        );

        // Второй ответ добавляется рядом, не затирая первый.
        let both = upsert_answer(&again, "focus", "конверсии");
        let parsed = parse_questions(&both);
        assert_eq!(parsed[0].answer.as_deref(), Some("суммарно"));
        assert_eq!(parsed[1].answer.as_deref(), Some("конверсии"));
    }

    /// Регрессия: модель пишет пустой блок как `answers: {}` в одну строку.
    /// Не распознав инлайн, патчер дописывал ВТОРОЙ ключ `answers` — такой YAML
    /// не парсится, и все вопросы исчезали из интерфейса.
    #[test]
    fn inline_empty_answers_map_is_replaced_not_duplicated() {
        let intake =
            "---\nquestions:\n  - id: breakdown\n    text: Как считать?\nanswers: {}\n---\nТело.\n";
        let patched = upsert_answer(intake, "breakdown", "суммарно");
        assert_eq!(patched.matches("answers").count(), 1, "дубль ключа answers");

        let questions = parse_questions(&patched);
        assert_eq!(questions.len(), 1, "вопросы должны остаться видны");
        assert_eq!(questions[0].answer.as_deref(), Some("суммарно"));
    }

    #[test]
    fn inline_answers_map_with_values_is_preserved() {
        let intake = "---\nquestions:\n  - id: a\n    text: A\n  - id: b\n    text: B\nanswers: {a: \"да\"}\n---\n";
        let patched = upsert_answer(intake, "b", "нет");
        let questions = parse_questions(&patched);
        assert_eq!(questions[0].answer.as_deref(), Some("да"));
        assert_eq!(questions[1].answer.as_deref(), Some("нет"));
    }

    #[test]
    fn answer_with_colon_survives_round_trip() {
        let patched = upsert_answer(INTAKE_WITH_QUESTIONS, "focus", "конверсии: cart и buyout");
        assert_eq!(
            parse_questions(&patched)[1].answer.as_deref(),
            Some("конверсии: cart и buyout")
        );
    }

    #[test]
    fn slug_cuts_on_word_boundary() {
        let long = slugify("Анализ воронки продаж WB с 2025-08 динамика и отклонения");
        assert!(long.chars().count() <= MAX_SLUG_CHARS);
        // Раньше получалось «…-и-от»: обрубок последнего слова.
        assert!(!long.ends_with("-от"), "обрезано посреди слова: {long}");
        assert!(!long.ends_with('-'));
    }

    #[tokio::test]
    async fn prompt_block_is_absent_until_there_is_work() {
        let ws = workspace(temp_root("prompt"));
        assert!(ws.render_for_prompt().await.is_none());

        let a = ws.start_activity("Сверка Q2").await.unwrap();
        a.write(PLAN_FILE, "- [x] поднять обороты").await.unwrap();
        a.save_step(StepKind::Query, "обороты", "{}", "json")
            .await
            .unwrap();
        ws.start_activity("Воронка WB").await.unwrap();
        ws.set_active("001-сверка-q2").await.unwrap();

        let block = ws.render_for_prompt().await.unwrap();
        assert!(block.contains("Активная задача: 001-сверка-q2"));
        assert!(block.contains("- [x] поднять обороты"));
        assert!(block.contains("001-query-обороты.json"));
        // Незаполненная анкета названа прямо — это подсказка её завести.
        assert!(block.contains("intake.md --- (не заполнен)"));
        // Прошлая задача видна, иначе возвращается та же амнезия.
        assert!(block.contains("002-воронка-wb"));
    }
}
