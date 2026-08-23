//! Мелкие части страницы «Процессы», общие для вкладок.
//!
//! Здесь два рода вещей: подписи (перевод машинных значений в человеческие) и
//! три компонента, которые повторяются на разных вкладках — раскрывающийся
//! блок, блок кода и таблица эффектов.
//!
//! Раскрывающийся блок не рендерит содержимое до первого раскрытия, и это не
//! экономия ради экономии: под ним лежит mjs Этапа целиком, а Этапов на
//! странице столько же, сколько заведено в каталоге.

use contracts::processes::{
    ActionMode, DefinitionStatus, EffectRecord, EffectStatus, StageRecord,
};
use leptos::prelude::*;
use serde_json::Value;

use crate::shared::icons::icon;

// ═══════════════════════════════════════════════════════════════════════
// Подписи
// ═══════════════════════════════════════════════════════════════════════

pub fn definition_status_label(status: DefinitionStatus) -> &'static str {
    match status {
        DefinitionStatus::Draft => "черновик",
        DefinitionStatus::Active => "активна",
        DefinitionStatus::Archived => "архив",
    }
}

pub fn definition_status_badge(status: DefinitionStatus) -> &'static str {
    match status {
        DefinitionStatus::Draft => "badge badge--warning",
        DefinitionStatus::Active => "badge badge--success",
        DefinitionStatus::Archived => "badge badge--neutral",
    }
}

pub fn effect_status_label(status: EffectStatus) -> &'static str {
    match status {
        EffectStatus::Planned => "план",
        EffectStatus::InProgress => "не завершено",
        EffectStatus::Executed => "сделано",
        EffectStatus::Failed => "упало",
    }
}

pub fn effect_status_badge(status: EffectStatus) -> &'static str {
    match status {
        EffectStatus::Planned => "badge badge--neutral",
        EffectStatus::InProgress => "badge badge--warning",
        EffectStatus::Executed => "badge badge--success",
        EffectStatus::Failed => "badge badge--error",
    }
}

pub fn mode_label(mode: ActionMode) -> &'static str {
    match mode {
        ActionMode::DryRun => "сухой прогон",
        ActionMode::Execute => "исполнение",
    }
}

/// Отпечаток целиком в строку не влезает и целиком не нужен: он опознаёт
/// «то же самое определение», а для глаза хватает начала.
pub fn short_digest(digest: &str) -> String {
    if digest.is_empty() {
        "—".to_string()
    } else {
        digest.chars().take(12).collect()
    }
}

/// Дедлайн ожидания человеческими единицами: в манифесте он в минутах, но
/// «1440 мин» никому ничего не говорит.
pub fn deadline_label(minutes: i64) -> String {
    let day = 24 * 60;
    if minutes >= day && minutes % day == 0 {
        format!("{} {}", minutes / day, plural(minutes / day, "сутки", "суток", "суток"))
    } else if minutes >= 60 && minutes % 60 == 0 {
        format!("{} {}", minutes / 60, plural(minutes / 60, "час", "часа", "часов"))
    } else {
        format!("{minutes} мин")
    }
}

/// Русское склонение по числу.
pub fn plural(n: i64, one: &'static str, few: &'static str, many: &'static str) -> &'static str {
    let n = n.abs();
    if (11..=14).contains(&(n % 100)) {
        return many;
    }
    match n % 10 {
        1 => one,
        2..=4 => few,
        _ => many,
    }
}

/// «4 Этапа», «6 рёбер» — число вместе со склонённым словом.
pub fn counted(n: usize, one: &'static str, few: &'static str, many: &'static str) -> String {
    format!("{n} {}", plural(n as i64, one, few, many))
}

// ═══════════════════════════════════════════════════════════════════════
// Компоненты
// ═══════════════════════════════════════════════════════════════════════

/// Раскрывающийся блок карточки: заголовок-кнопка и содержимое под ним.
///
/// Содержимое появляется в DOM с первого раскрытия и дальше **остаётся**,
/// скрытое классом. Две причины, и обе про потерю состояния: свёрнуть-
/// развернуть не должно стирать введённый вход сухого прогона и не должно
/// сбрасывать прокрутку в коде Этапа.
#[component]
pub fn Disclosure(
    #[prop(into)] title: String,
    /// Правая подпись заголовка: сколько там строк, полей, версий.
    #[prop(optional, into)]
    hint: String,
    children: ChildrenFn,
) -> impl IntoView {
    let open = RwSignal::new(false);
    let mounted = RwSignal::new(false);
    let has_hint = !hint.is_empty();

    view! {
        <div class="sys-processes__disclosure">
            <button
                type="button"
                class="sys-processes__disclosure-head"
                on:click=move |_| {
                    open.update(|value| *value = !*value);
                    if open.get_untracked() {
                        mounted.set(true);
                    }
                }
            >
                <span class="sys-processes__disclosure-caret">
                    {move || if open.get() { icon("chevron-down") } else { icon("chevron-right") }}
                </span>
                <span class="sys-processes__disclosure-title">{title}</span>
                {has_hint
                    .then(|| view! { <span class="sys-processes__disclosure-hint">{hint}</span> })}
            </button>
            <Show when=move || mounted.get()>
                <div
                    class="sys-processes__disclosure-body"
                    class=("sys-processes__disclosure-body--hidden", move || !open.get())
                >
                    {children()}
                </div>
            </Show>
        </div>
    }
}

/// Моноширинный блок с исходником или JSON.
#[component]
pub fn CodeBlock(#[prop(into)] text: String) -> impl IntoView {
    view! { <pre class="sys-processes__code">{text}</pre> }
}

/// JSON с отступами. Невалидного тут не бывает — значение уже разобрано.
#[component]
pub fn JsonBlock(value: Value) -> impl IntoView {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
    view! { <CodeBlock text=text /> }
}

/// Таблица журнала эффектов. Одна на разбор экземпляра и на вкладку журналов:
/// вопрос «что механизм сделал с миром» в обоих местах один и тот же.
#[component]
pub fn EffectsTable(records: Vec<EffectRecord>) -> impl IntoView {
    let empty = records.is_empty();

    view! {
        <Show when=move || empty>
            <div class="sys-processes__empty">"Эффектов не записано."</div>
        </Show>
        <Show when=move || !empty>
            <div class="table-wrapper">
                <table class="table__data">
                    <thead>
                        <tr>
                            <th class="table__header-cell">"Действие"</th>
                            <th class="table__header-cell">"Режим"</th>
                            <th class="table__header-cell">"Статус"</th>
                            <th class="table__header-cell">"Ключ идемпотентности"</th>
                            <th class="table__header-cell">"Кто"</th>
                            <th class="table__header-cell">"Этап"</th>
                            <th class="table__header-cell">"Начато"</th>
                            <th class="table__header-cell">"Длит., мс"</th>
                        </tr>
                    </thead>
                    <tbody>
                        {records
                            .clone()
                            .into_iter()
                            .map(|record| {
                                let error_text = record.error_text.clone();
                                view! {
                                    <tr class="table__row">
                                        <td class="table__cell">{record.action_name}</td>
                                        <td class="table__cell">{mode_label(record.mode)}</td>
                                        <td class="table__cell">
                                            <span class=effect_status_badge(record.status)>
                                                {effect_status_label(record.status)}
                                            </span>
                                            {error_text
                                                .map(|text| {
                                                    view! {
                                                        <div class="sys-processes__cell-note">{text}</div>
                                                    }
                                                })}
                                        </td>
                                        <td class="table__cell sys-processes__mono">
                                            {record.idempotency_key}
                                        </td>
                                        <td class="table__cell">{record.actor}</td>
                                        <td class="table__cell">
                                            {record.stage_code.unwrap_or_else(|| "—".to_string())}
                                        </td>
                                        <td class="table__cell">{record.started_at}</td>
                                        <td class="table__cell table__cell--right">
                                            {record
                                                .duration_ms
                                                .map(|value| value.to_string())
                                                .unwrap_or_else(|| "—".to_string())}
                                        </td>
                                    </tr>
                                }
                            })
                            .collect_view()}
                    </tbody>
                </table>
            </div>
        </Show>
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Чтение каталога Этапов
// ═══════════════════════════════════════════════════════════════════════

/// Пара «ключ — значение» карточки.
#[component]
pub fn Fact(#[prop(into)] label: String, children: Children) -> impl IntoView {
    view! {
        <div class="sys-processes__fact">
            <div class="sys-processes__fact-key">{label}</div>
            <div class="sys-processes__fact-value">{children()}</div>
        </div>
    }
}

pub fn find_stage<'a>(stages: &'a [StageRecord], code: &str) -> Option<&'a StageRecord> {
    stages.iter().find(|record| record.code == code)
}

/// Ссылка на Этап так, как её читает человек: код и название.
pub fn stage_ref_label(code: &str, stages: &[StageRecord]) -> String {
    match find_stage(stages, code) {
        Some(record) => format!("{code} «{}»", record.definition.manifest.title),
        None => format!("{code} — Этап не заведён"),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Разбор JSON Schema
// ═══════════════════════════════════════════════════════════════════════

/// Одно поле схемы — для человека, а не для валидатора.
#[derive(Debug, Clone)]
pub struct SchemaField {
    pub name: String,
    pub kind: String,
    pub required: bool,
    /// Ограничения одной строкой: шаблон, длина, перечисление.
    pub note: String,
}

/// Поля верхнего уровня JSON Schema.
///
/// Разбор намеренно мелкий: карточка отвечает на вопрос «что подавать на
/// вход», а не воспроизводит валидатор. Схема целиком доступна рядом,
/// раскрывающимся блоком.
pub fn schema_fields(schema: &Value) -> Vec<SchemaField> {
    let required: Vec<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        return Vec::new();
    };

    properties
        .iter()
        .map(|(name, spec)| SchemaField {
            name: name.clone(),
            kind: spec
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("—")
                .to_string(),
            required: required.contains(&name.as_str()),
            note: field_note(spec),
        })
        .collect()
}

fn field_note(spec: &Value) -> String {
    let mut notes: Vec<String> = Vec::new();
    if let Some(pattern) = spec.get("pattern").and_then(Value::as_str) {
        notes.push(format!("шаблон {pattern}"));
    }
    if let Some(format) = spec.get("format").and_then(Value::as_str) {
        notes.push(format!("формат {format}"));
    }
    if let Some(min) = spec.get("minLength").and_then(Value::as_u64) {
        notes.push(format!("минимум {min} символ{}", if min == 1 { "" } else { "ов" }));
    }
    if let Some(values) = spec.get("enum").and_then(Value::as_array) {
        let list = values
            .iter()
            .map(|value| value.as_str().map(str::to_string).unwrap_or_else(|| value.to_string()))
            .collect::<Vec<_>>()
            .join(", ");
        notes.push(format!("одно из: {list}"));
    }
    notes.join(", ")
}

/// Заготовка входа для сухого прогона: обязательные поля схемы с пустыми
/// значениями по типу.
///
/// Пустая строка вместо правдоподобного значения намеренна: подставь мы
/// «сегодня» или первый попавшийся кабинет, прогон пошёл бы по данным, которых
/// человек не выбирал.
pub fn input_skeleton(schema: Option<&Value>) -> String {
    let Some(schema) = schema else {
        return "{}".to_string();
    };
    let fields = schema_fields(schema);
    let mut object = serde_json::Map::new();
    for field in fields.into_iter().filter(|field| field.required) {
        object.insert(field.name, placeholder(&field.kind));
    }
    serde_json::to_string_pretty(&Value::Object(object)).unwrap_or_else(|_| "{}".to_string())
}

fn placeholder(kind: &str) -> Value {
    match kind {
        "integer" | "number" => Value::from(0),
        "boolean" => Value::Bool(false),
        "array" => Value::Array(Vec::new()),
        "object" => Value::Object(serde_json::Map::new()),
        _ => Value::String(String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn deadline_speaks_human() {
        assert_eq!(deadline_label(24 * 60), "1 сутки");
        assert_eq!(deadline_label(48 * 60), "2 суток");
        assert_eq!(deadline_label(90), "90 мин");
        assert_eq!(deadline_label(120), "2 часа");
    }

    #[test]
    fn schema_fields_mark_what_is_required() {
        let schema = json!({
            "type": "object",
            "required": ["connection_id"],
            "properties": {
                "connection_id": { "type": "string", "minLength": 1 },
                "business_date": { "type": "string", "pattern": "^\\d{4}-\\d{2}-\\d{2}$" }
            }
        });
        let fields = schema_fields(&schema);
        assert_eq!(fields.len(), 2);
        let connection = fields
            .iter()
            .find(|f| f.name == "connection_id")
            .expect("поле connection_id разобрано");
        assert!(connection.required);
        let date = fields
            .iter()
            .find(|f| f.name == "business_date")
            .expect("поле business_date разобрано");
        assert!(!date.required);
        assert!(date.note.contains("шаблон"));
    }

    /// Заготовка обязана быть валидным JSON и содержать ровно обязательные
    /// поля: человек правит значения, а не собирает объект с нуля.
    #[test]
    fn skeleton_carries_only_required_fields() {
        let schema = json!({
            "required": ["connection_id", "business_date"],
            "properties": {
                "connection_id": { "type": "string" },
                "business_date": { "type": "string" },
                "note": { "type": "string" }
            }
        });
        let raw = input_skeleton(Some(&schema));
        let value: Value = serde_json::from_str(&raw).expect("заготовка — валидный JSON");
        let object = value.as_object().expect("заготовка — объект");
        assert_eq!(object.len(), 2);
        assert!(object.contains_key("connection_id"));
        assert!(!object.contains_key("note"));
    }

    #[test]
    fn skeleton_without_schema_is_an_empty_object() {
        assert_eq!(input_skeleton(None), "{}");
    }
}
