//! Роутер интентов (Фаза 0).
//!
//! Классифицирует сообщение пользователя в один из известных интентов. На Фазе 0
//! результат только записывается в метаданные сообщения и логируется — поведение
//! пайплайна (набор tools, промпт) пока не меняется.
//!
//! Основной путь — дешёвый LLM-вызов без инструментов (`chat_completion`), который
//! просят вернуть строгий JSON `{ "intent": "...", "confidence": 0.0 }`.
//! Если вызов не удался или ответ не распарсился — fallback на правила/ключевые слова.

use super::types::{ChatMessage, LlmProvider};
use contracts::domain::a017_llm_agent::aggregate::AgentType;

/// Известные интенты уровня сообщения (см. план, §1).
pub const KNOWN_INTENTS: &[&str] = &[
    "func_help",                   // вопрос по функционалу приложения
    "data_query",                  // аналитика по данным (SQL/drilldown/индикаторы)
    "sales_query",                 // продажи/выручка/заказы/маржа (аналитик продаж)
    "marketing_query",             // реклама/воронка/поисковая аналитика/промо (маркетолог)
    "marketplace_funnel_analysis", // сквозная воронка маркетплейса и сравнение этапов
    "finance_query",               // главная книга/сверка выручки/взаиморасчёты (финансист)
    "bi_authoring",                // создание индикатора/дашборда
    "chart_build",                 // построить график/диаграмму по данным
    "table_build",                 // построить таблицу данных (плагин-таблица)
    "plugin_dev",                  // создание/доработка плагина
    "quality_check",               // просмотр/запуск проверок качества
    "quality_check_dev",           // создание/изменение MJS-проверки качества
    "sys_admin",                   // системная диагностика
    "kb_curation",                 // работа с базой знаний
    "mailbox",                     // чтение/отправка почты
    "support",                     // обращение в поддержку: сбой, пожелание, тикет
    "llm_quality_review",          // оценка качества работы агентов по диалогам
    "meta_smalltalk",              // приветствие/уточнение/«что ты умеешь»
];

/// Служебный маркер в начале задания фоновой задачи. Пользователь такого не пишет:
/// это способ для задачи выбрать навык точно, а не надеяться на ключевые слова
/// (текст задания судьи полон слов «ошибка»/«не работает» и без маркера уехал бы
/// в `support`).
pub const SERVICE_INTENT_MARKER: &str = "режим: ";

/// Результат классификации.
#[derive(Debug, Clone)]
pub struct IntentResult {
    pub intent: String,
    pub confidence: f64,
    /// Откуда получен результат — для аналитики/отладки ("llm" | "rules").
    pub source: &'static str,
    pub tokens_used: i32,
    /// Разбивка токенов классификатора. Нужна, чтобы вызов роутера попал в
    /// стоимость ответа: иначе сумма prompt+completion разошлась бы с `tokens_used`.
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub cached_prompt_tokens: i32,
}

impl IntentResult {
    fn new(intent: impl Into<String>, confidence: f64, source: &'static str) -> Self {
        Self {
            intent: intent.into(),
            confidence,
            source,
            tokens_used: 0,
            prompt_tokens: 0,
            completion_tokens: 0,
            cached_prompt_tokens: 0,
        }
    }
}

/// Системный промпт классификатора. Просим строгий JSON без пояснений.
fn classifier_system_prompt() -> String {
    format!(
        "Ты — классификатор запросов пользователя в системе управления маркетплейсами \
         (Wildberries, OZON, Яндекс.Маркет). Определи ЕДИНСТВЕННЫЙ интент сообщения.\n\n\
         Возможные интенты:\n\
         - func_help: как пользоваться приложением, где найти функцию, что делает фича.\n\
         - data_query: общая аналитика по данным — остатки, отчёты, SQL, drilldown, индикаторы (без явной темы продаж/маркетинга/финансов).\n\
         - sales_query: продажи, выручка, заказы, средний чек, маржа/прибыль, динамика продаж.\n\
         - marketing_query: реклама, ДРР, CTR, ставки, воронка продаж, конверсии, выкуп, поисковая аналитика, промо/акции.\n\
         - marketplace_funnel_analysis: рассчитать, проверить или сравнить этапы воронки маркетплейса по данным OZON, Wildberries или Яндекс.Маркета.\n\
         - finance_query: главная книга, обороты по счетам, сверка выручки (fina/ybuh), взаиморасчёты, комиссии.\n\
         - bi_authoring: просьба СОЗДАТЬ индикатор/дашборд/KPI.\n\
         - chart_build: построить ГРАФИК/диаграмму/визуализацию по данным (линия, столбцы, доли).\n\
         - table_build: построить ТАБЛИЦУ данных по данным (колонки/строки, фильтры, сортировка, итоги).\n\
         - plugin_dev: создать/доработать/протестировать плагин (JS).\n\
         - quality_check: посмотреть каталог/результаты или запустить существующую проверку качества данных.\n\
         - quality_check_dev: создать, изменить или опубликовать исполняемую MJS-проверку качества данных.\n\
         - sys_admin: состояние системы, производительность, фоновые задачи, целостность данных.\n\
         - kb_curation: работа с базой знаний — прочитать/исправить статью, тикет правки.\n\
         - mailbox: почта — прочитать входящие письма, найти письмо, ответить или отправить письмо.\n\
         - support: обращение по работе самой программы — что-то не работает/ошибка/сломалось, просьба доработать или идея по улучшению, просьба завести тикет/заявку.\n\
         - meta_smalltalk: приветствие, благодарность, «что ты умеешь», уточнение без конкретной задачи.\n\n\
         Ответь СТРОГО валидным JSON без пояснений и без markdown:\n\
         {{\"intent\": \"<один из: {}>\", \"confidence\": <число 0.0..1.0>}}",
        KNOWN_INTENTS.join(", ")
    )
}

/// Классифицировать сообщение. Никогда не паникует и всегда возвращает результат
/// (в худшем случае — fallback по правилам).
pub async fn classify_intent(
    provider: &dyn LlmProvider,
    user_message: &str,
    recent_summary: &str,
    seed_agent_type: &AgentType,
) -> IntentResult {
    // Очень короткие/пустые реплики — болтовня, без LLM-вызова.
    let trimmed = user_message.trim();
    if trimmed.chars().count() < 3 {
        return IntentResult::new("meta_smalltalk", 0.5, "rules");
    }

    let mut user_block = String::new();
    if !recent_summary.trim().is_empty() {
        user_block.push_str("Краткий контекст последних ходов:\n");
        user_block.push_str(recent_summary.trim());
        user_block.push_str("\n\n");
    }
    user_block.push_str("Сообщение пользователя:\n");
    user_block.push_str(trimmed);

    let messages = vec![
        ChatMessage::system(classifier_system_prompt()),
        ChatMessage::user(user_block),
    ];

    match provider.chat_completion(&messages).await {
        Ok(resp) => match parse_intent_json(&resp.content) {
            Some(mut result) => {
                result.tokens_used = resp.tokens_used.unwrap_or(0);
                result.prompt_tokens = resp.prompt_tokens.unwrap_or(0);
                result.completion_tokens = resp.completion_tokens.unwrap_or(0);
                result.cached_prompt_tokens = resp.cached_prompt_tokens.unwrap_or(0);
                result
            }
            None => {
                tracing::warn!(
                    "[router] не удалось распарсить ответ классификатора, fallback на правила: {}",
                    preview(&resp.content)
                );
                rule_based(trimmed, seed_agent_type)
            }
        },
        Err(e) => {
            tracing::warn!(
                "[router] ошибка LLM-классификатора ({:?}), fallback на правила",
                e
            );
            rule_based(trimmed, seed_agent_type)
        }
    }
}

fn preview(s: &str) -> String {
    s.chars().take(120).collect()
}

/// Маппинг интента (см. `KNOWN_INTENTS`) в тип агента-исполнителя.
/// Обратное к seed-таблице `AgentType → intent`. Используется почтовым конвейером
/// для выбора специалиста по содержимому письма.
pub fn intent_to_agent_type(intent: &str) -> AgentType {
    match intent {
        "kb_curation" => AgentType::KbAdmin,
        // Разработчик один: и плагины, и поддержка пользователей (навык `support`).
        "plugin_dev" | "quality_check_dev" | "support" => AgentType::PluginAdmin,
        "sys_admin" => AgentType::SystemAdmin,
        "sales_query" => AgentType::SalesAnalyst,
        "marketing_query" => AgentType::Marketer,
        "marketplace_funnel_analysis" => AgentType::Marketer,
        "finance_query" => AgentType::Financier,
        // data_query | chart_build | table_build | bi_authoring | func_help — аналитик.
        "data_query" | "quality_check" | "chart_build" | "table_build" | "bi_authoring"
        | "func_help" => AgentType::BusinessAnalyst,
        // meta_smalltalk и всё прочее — общий агент.
        _ => AgentType::CoordinatorAdmin,
    }
}

/// Канонический интент специализации — обратная сторона `intent_to_agent_type`.
///
/// Нужен фоновым сценариям, которые ставят задачу специалисту напрямую: маркер
/// `Режим: <интент>` в начале задания поднимает нужный навык детерминированно,
/// вместо того чтобы гадать по ключевым словам чужой формулировки.
///
/// У координатора `None`: ему доступно всё, и маркер только сузил бы набор.
pub fn default_intent_for_agent_type(agent_type: &AgentType) -> Option<&'static str> {
    Some(match agent_type {
        AgentType::BusinessAnalyst => "data_query",
        AgentType::SalesAnalyst => "sales_query",
        AgentType::Marketer => "marketing_query",
        AgentType::Financier => "finance_query",
        AgentType::SystemAdmin => "sys_admin",
        AgentType::KbAdmin => "kb_curation",
        AgentType::PluginAdmin => "plugin_dev",
        AgentType::Tester => "quality_check",
        AgentType::CoordinatorAdmin => return None,
    })
}

/// Быстрая (rule-based, без LLM) классификация интента для синхронной предактивации
/// навыков перед основным циклом. Полный LLM-роутер по-прежнему идёт конкурентно.
pub fn quick_intent(message: &str, seed_agent_type: &AgentType) -> String {
    quick_intent_result(message, seed_agent_type).intent
}

/// Быстрый результат вместе с confidence — chat selector использует низкую
/// уверенность как признак follow-up, не меняющего текущую специализацию задачи.
pub fn quick_intent_result(message: &str, seed_agent_type: &AgentType) -> IntentResult {
    let trimmed = message.trim();
    if trimmed.chars().count() < 3 {
        return IntentResult::new("meta_smalltalk", 0.5, "rules");
    }
    rule_based(trimmed, seed_agent_type)
}

/// Распарсить `{ "intent": "...", "confidence": ... }` из ответа модели,
/// допуская обрамление markdown-кодом и лишний текст вокруг.
fn parse_intent_json(content: &str) -> Option<IntentResult> {
    // Найти первый '{' и последний '}' — грубое извлечение JSON-объекта.
    let start = content.find('{')?;
    let end = content.rfind('}')?;
    if end <= start {
        return None;
    }
    let json_slice = &content[start..=end];
    let value: serde_json::Value = serde_json::from_str(json_slice).ok()?;

    let intent = value.get("intent")?.as_str()?.trim().to_lowercase();
    if !KNOWN_INTENTS.contains(&intent.as_str()) {
        return None;
    }
    let confidence = value
        .get("confidence")
        .and_then(|c| c.as_f64())
        .unwrap_or(0.6)
        .clamp(0.0, 1.0);

    Some(IntentResult::new(intent, confidence, "llm"))
}

/// Резервная классификация по ключевым словам. Низкая уверенность, чтобы на Фазе 1
/// такие случаи можно было отличать и при необходимости уточнять у пользователя.
fn rule_based(message: &str, seed_agent_type: &AgentType) -> IntentResult {
    let m = message.to_lowercase();

    let any = |needles: &[&str]| needles.iter().any(|n| m.contains(n));

    // Явный служебный маркер важнее любых ключевых слов: задание фоновой задачи
    // само называет свой режим, и угадывать по тексту нечего.
    if let Some(rest) = m.split_once(SERVICE_INTENT_MARKER) {
        let named = rest
            .1
            .trim_start()
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .next()
            .unwrap_or_default();
        if let Some(intent) = KNOWN_INTENTS.iter().find(|known| **known == named) {
            return IntentResult::new(*intent, 0.95, "rules");
        }
    }

    let quality_words = any(&[
        "quality check",
        "quality_check",
        "quality_checks",
        "контрол качества",
        "контроль качества",
        "проверок качества",
        "проверки качества",
        "проверку качества",
    ]);
    if quality_words && any(&["созда", "добав", "измен", "доработ", "опубли", "напиш"])
    {
        return IntentResult::new("quality_check_dev", 0.65, "rules");
    }
    if quality_words {
        return IntentResult::new("quality_check", 0.65, "rules");
    }

    // Поддержка — раньше остальных правил: «не работает график» это обращение о сбое,
    // а не просьба построить график. Исключение — разработка плагинов (свой интент).
    if !any(&["плагин", "plugin"])
        && any(&[
            "тикет",
            "обращени",
            "заявк",
            "поддержк",
            "баг",
            "не работает",
            "не работают",
            "не открывается",
            "не грузит",
            "не загружается",
            "не сохраня",
            "ошибк",
            "сломал",
            "глючит",
            "зависает",
            "доработ",
            "пожелани",
            "неудобно",
        ])
    {
        return IntentResult::new("support", 0.5, "rules");
    }
    if any(&["график", "графік", "диаграмм", "chart", "чарт", "визуализ"])
    {
        return IntentResult::new("chart_build", 0.5, "rules");
    }
    if any(&["таблиц", "table", "грид", "grid", "data-grid"]) {
        return IntentResult::new("table_build", 0.5, "rules");
    }
    if any(&["плагин", "plugin", "виджет"]) {
        return IntentResult::new("plugin_dev", 0.45, "rules");
    }
    if any(&["индикатор", "дашборд", "kpi", "дашбоард", "показател"])
        && any(&["созда", "добав", "сдела", "построй"])
    {
        return IntentResult::new("bi_authoring", 0.45, "rules");
    }
    if any(&[
        "здоров",
        "производительн",
        "фонов",
        "задач",
        "целостност",
        "диагност",
        "health",
    ]) {
        return IntentResult::new("sys_admin", 0.4, "rules");
    }
    if any(&["база знаний", "статья", "статью", "knowledge", "kb"]) {
        return IntentResult::new("kb_curation", 0.4, "rules");
    }
    if any(&[
        "письм",
        "почт",
        "email",
        "e-mail",
        "mail",
        "входящ",
        "отправь письмо",
        "напиши письмо",
    ]) {
        return IntentResult::new("mailbox", 0.45, "rules");
    }
    // Маркетинг раньше продаж: «реклама/воронка/промо» — маркетолог, не общий data_query.
    // Маркетплейс-слово: включая разговорные «wb»/«вб», как пишут продажники (кабинет
    // «WB - SANSTAR» → lowercase содержит "wb"). Латинский "wb" в русских словах не встречается.
    let marketplace_word = any(&[
        "ozon",
        "озон",
        "wildberries",
        "вайлдберриз",
        "вайлдберис",
        "wb",
        "вб",
        "яндекс маркет",
        "маркетплейс",
        // «Кабинет» в этом домене всегда означает кабинет маркетплейса (a006_connection_mp):
        // продажник пишет «по двум кабинетам», не называя площадку.
        "кабинет",
    ]);
    // «Воронка + маркетплейс», а также WB-вопросы по низу воронки (выкуп/отмены/возвраты/топ
    // товаров/конверсии), которые продажник задаёт БЕЗ слова «воронка», но их авторитет — навык
    // воронки (иначе уходят в data-analytics и теряют гардрейлы: funnel_order_count≠order_count,
    // фильтр кабинета, лаг выкупа).
    if any(&[
        "воронка маркетплейс",
        "воронку маркетплейс",
        "funnel marketplace",
        "ozon funnel",
        "wildberries funnel",
        "яндекс маркет funnel",
    ]) || (any(&["воронк", "funnel"]) && marketplace_word)
        || (marketplace_word
            && any(&[
                "выкуп",
                "отмен",
                "возврат",
                "топ товаров",
                "топ-5",
                "топ 5",
                "конверси",
            ]))
    {
        return IntentResult::new("marketplace_funnel_analysis", 0.55, "rules");
    }
    // Слово «воронка» само по себе. Другой воронки, кроме маркетплейсовой, в домене нет,
    // а площадку в переспросе («проверь данные именно по воронке») обычно не повторяют —
    // без этого правила такие уточнения уходили в marketing_query/data-analytics.
    if any(&["воронк", "funnel"]) {
        return IntentResult::new("marketplace_funnel_analysis", 0.5, "rules");
    }
    if any(&[
        "реклам",
        "дрр",
        "ctr",
        "конверс",
        "выкуп",
        "ставк",
        "промо",
        "акци",
        "поисков",
        "джем",
    ]) {
        return IntentResult::new("marketing_query", 0.45, "rules");
    }
    if any(&[
        "главная книга",
        "главную книгу",
        "сверк",
        "взаиморасч",
        "комисс",
        "оборот",
        "дебет",
        "кредит",
        "проводк",
        "реализац",
    ]) {
        return IntentResult::new("finance_query", 0.45, "rules");
    }
    if any(&[
        "выручк",
        "продаж",
        "заказ",
        "маржинальн",
        "маржа",
        "прибыл",
        "средний чек",
    ]) {
        return IntentResult::new("sales_query", 0.45, "rules");
    }
    if any(&[
        "отчёт",
        "отчет",
        "остат",
        "sql",
        "сколько",
        "сумм",
        "возврат",
    ]) {
        return IntentResult::new("data_query", 0.45, "rules");
    }
    if any(&[
        "как ",
        "где ",
        "что такое",
        "что делает",
        "помоги найти",
        "инструкц",
    ]) {
        return IntentResult::new("func_help", 0.4, "rules");
    }

    // Иначе — seed по типу агента (back-compat), низкая уверенность.
    let seeded = match seed_agent_type {
        AgentType::SystemAdmin => "sys_admin",
        AgentType::KbAdmin => "kb_curation",
        // Расплывчатая реплика разработчику — это почти всегда вопрос по системе:
        // просьбы про плагины ловятся явным правилом по слову «плагин» выше.
        AgentType::PluginAdmin => "support",
        AgentType::SalesAnalyst => "sales_query",
        AgentType::Marketer => "marketing_query",
        AgentType::Financier => "finance_query",
        AgentType::Tester => "quality_check",
        _ => "data_query",
    };
    IntentResult::new(seeded, 0.25, "rules")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Задание судьи полно слов «ошибка» и «плохой ответ» — без явного маркера
    /// оно уехало бы в `support` и подняло не тот навык.
    #[test]
    fn service_marker_wins_over_keyword_rules() {
        let trigger = "Режим: llm_quality_review.\n\
                       Ищи неверные ответы, ошибки SQL и повторные неудачные вызовы.";
        let result = quick_intent_result(trigger, &AgentType::CoordinatorAdmin);
        assert_eq!(result.intent, "llm_quality_review");
        assert!(result.confidence > 0.9);
    }

    /// Незнакомый режим не должен молча становиться интентом: иначе опечатка в
    /// задании тихо активировала бы произвольный навык.
    #[test]
    fn unknown_service_marker_falls_through_to_rules() {
        let result = quick_intent_result(
            "Режим: не_существует. Построй график продаж",
            &AgentType::BusinessAnalyst,
        );
        assert_ne!(result.intent, "не_существует");
    }

    /// Маркер режима в поручении a042 обязан приводить исполнителя к его же
    /// специализации. Если кто-то перенаправит `sales_query` на другой тип,
    /// делегирование начнёт молча поднимать чужой навык — ловим здесь.
    ///
    /// Проверяем только биективную часть таблицы: `Tester` намеренно делит
    /// `quality_check` с аналитиком, а у координатора канонического интента нет.
    #[test]
    fn default_intent_round_trips_for_specialists() {
        for agent_type in [
            AgentType::BusinessAnalyst,
            AgentType::SalesAnalyst,
            AgentType::Marketer,
            AgentType::Financier,
            AgentType::SystemAdmin,
            AgentType::KbAdmin,
            AgentType::PluginAdmin,
        ] {
            let intent = default_intent_for_agent_type(&agent_type)
                .unwrap_or_else(|| panic!("нет интента для {:?}", agent_type));
            assert!(
                KNOWN_INTENTS.contains(&intent),
                "интент {intent} отсутствует в KNOWN_INTENTS — маркер не сработает"
            );
            assert_eq!(
                intent_to_agent_type(intent),
                agent_type,
                "интент {intent} ведёт не к своей специализации"
            );
        }
        assert!(default_intent_for_agent_type(&AgentType::CoordinatorAdmin).is_none());
        assert_eq!(
            default_intent_for_agent_type(&AgentType::Tester),
            Some("quality_check")
        );
    }

    #[test]
    fn marketplace_funnel_rule_precedes_generic_marketing() {
        let result =
            quick_intent_result("Сравни воронку OZON за два периода", &AgentType::Marketer);
        assert_eq!(result.intent, "marketplace_funnel_analysis");
        assert!(result.confidence > 0.5);
    }

    #[test]
    fn wb_cabinet_funnel_questions_route_to_funnel_skill() {
        // Продажник пишет «WB», а не «Wildberries»; кабинет — «WB - SANSTAR».
        for q in [
            "Посчитай воронку продаж по кабинету WB - SANSTAR за июль",
            "Какой процент выкупа по WB - SANSTAR за июль 2026?",
            "Топ-5 товаров WB - SANSTAR по заказам за июль",
            "Сколько отмен и возвратов по WB за июль?",
        ] {
            let result = quick_intent_result(q, &AgentType::BusinessAnalyst);
            assert_eq!(
                result.intent, "marketplace_funnel_analysis",
                "ожидали funnel для: {q}"
            );
        }
    }

    /// Реальные формулировки из чата CHAT-75b58343 (оценка 2): все четыре ушли мимо навыка
    /// воронки — «кабинет» не считался маркетплейс-словом, а переспрос без площадки терялся.
    #[test]
    fn cabinet_wording_and_bare_funnel_route_to_funnel_skill() {
        for q in [
            "я загрузил Воронки по двум кабинетам с августа, посмотри что получилось",
            "Проверь данные именно по воронке.",
            "Сколько отмен по 2-м кабинетам за июль?",
        ] {
            let result = quick_intent_result(q, &AgentType::BusinessAnalyst);
            assert_eq!(
                result.intent, "marketplace_funnel_analysis",
                "ожидали funnel для: {q}"
            );
        }
    }

    #[test]
    fn generic_buyout_without_marketplace_stays_marketing() {
        // Без маркетплейс-слова «выкуп» остаётся маркетингом (funnel-правило не срабатывает).
        let result =
            quick_intent_result("Какой у нас процент выкупа?", &AgentType::BusinessAnalyst);
        assert_eq!(result.intent, "marketing_query");
    }

    #[test]
    fn quality_view_and_authoring_are_distinct_intents() {
        assert_eq!(
            quick_intent_result(
                "Покажи последние результаты проверок качества",
                &AgentType::Financier
            )
            .intent,
            "quality_check"
        );
        assert_eq!(
            quick_intent_result(
                "Добавь новую quality check для остатков",
                &AgentType::BusinessAnalyst
            )
            .intent,
            "quality_check_dev"
        );
    }
}
