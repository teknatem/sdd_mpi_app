//! Оболочка чата над реестром Действий.
//!
//! Действия и инструменты — одна библиотека вызываемых операций (`CONTEXT.md`,
//! §«Действие»). Оболочек у неё две: Этап (`processes::stages`) и чат — вот эта.
//! Обе зовут один `actions::run`, поэтому одна операция получает одни и те же
//! гарантии независимо от того, кто её позвал: проверку входа по схеме, ключ
//! идемпотентности и запись в `sys_effect_log`.
//!
//! Чем эта оболочка отличается от оболочки Этапа — ровно тем, что в таблице
//! плана процессов: контекст (чат, агент, собеседник вместо экземпляра и Этапа),
//! вид ошибки (`{ok:false, error}`, который модель читает и исправляется, вместо
//! исключения mjs) и состав ключа идемпотентности.

use contracts::processes::{ActionActor, ActionCall, ActionMode};
use serde_json::{json, Value};

use crate::processes::actions;

/// Кто и в каком ходе зовёт эффект.
#[derive(Debug, Clone)]
pub struct ChatEffect {
    actor: ActionActor,
    /// Диалог и номер хода — основа ключа идемпотентности.
    scope: String,
}

impl ChatEffect {
    /// Собрать контекст эффекта для текущего хода диалога.
    ///
    /// **Номер хода в ключе обязателен.** «Сделай ещё раз» — законная просьба
    /// человека, и она обязана дать новый эффект; повтор же внутри одного хода —
    /// это ретрай модели, и он должен схлопнуться в тот самый эффект, а не
    /// удвоить его. Ключ без номера хода не различал бы эти два случая и выбирал
    /// бы всегда один — какой именно, зависело бы от того, что страшнее.
    ///
    /// Это прямой аналог номера захода в ключе Этапа (`stages::idempotency_key`):
    /// там цикл графа, здесь цикл диалога, задача одна.
    pub fn new(actor: ActionActor, chat_id: &str, turn: u32) -> Self {
        let chat = if chat_id.trim().is_empty() {
            "no-chat"
        } else {
            chat_id.trim()
        };
        Self {
            actor,
            scope: format!("chat:{chat}@{turn}"),
        }
    }

    /// Дописать в провенанс место в цепочке делегирования.
    ///
    /// Цепочку считает сам инструмент (`resolve_chain` по диалогу), а не
    /// оболочка: до разбора аргументов неизвестно, будет ли вызов вообще
    /// делегированием. Аргументам модели тут места нет — глубина выводится из
    /// данных.
    pub fn with_chain(mut self, parent_task_ref: Option<String>, chain_depth: i32) -> Self {
        if let ActionActor::User {
            parent_task_ref: slot,
            depth,
            ..
        } = &mut self.actor
        {
            *slot = parent_task_ref;
            *depth = chain_depth;
        }
        self
    }

    /// Вызвать Действие и привести исход к форме, которую читает модель.
    ///
    /// `suffix` различает несколько вызовов одного Действия внутри хода: без
    /// него две разные quality-проверки подряд собрали бы один ключ, и вторая
    /// вернула бы результат первой как `replayed`.
    pub async fn run(&self, action: &str, input: Value, suffix: &str) -> Value {
        let db = crate::shared::data::db::get_connection();
        let call = ActionCall {
            idempotency_key: format!("{}:{action}:{suffix}", self.scope),
            action: action.to_string(),
            input,
            // Сухой прогон из чата не нужен: человек видит ответ модели и
            // возражает словами. Режим задаёт оболочка, не модель.
            mode: ActionMode::Execute,
            actor: self.actor.clone(),
        };
        match actions::run(db, &call).await {
            Ok(outcome) => {
                let mut result = match outcome.result {
                    Value::Object(map) => map,
                    other => {
                        let mut map = serde_json::Map::new();
                        map.insert("result".to_string(), other);
                        map
                    }
                };
                result.insert("ok".to_string(), Value::Bool(true));
                result.insert("effect_id".to_string(), Value::String(outcome.effect_id));
                // `replayed` модель обязана различать: это не «сделано снова»,
                // а «уже было сделано в этом ходе, вот тот же результат».
                result.insert("replayed".to_string(), Value::Bool(outcome.replayed));
                Value::Object(result)
            }
            Err(error) => json!({ "ok": false, "error": error.to_string() }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor() -> ActionActor {
        ActionActor::User {
            user_id: "u1".into(),
            chat_ref: Some("c1".into()),
            agent_ref: None,
            parent_task_ref: None,
            depth: 0,
        }
    }

    /// Разные ходы — разные ключи, иначе «сделай ещё раз» вернуло бы `replayed`
    /// вместо нового эффекта.
    #[test]
    fn turn_number_separates_keys() {
        let first = ChatEffect::new(actor(), "c1", 1);
        let second = ChatEffect::new(actor(), "c1", 2);
        assert_ne!(first.scope, second.scope);
    }

    /// А внутри хода ключ повторяется: ретрай модели с теми же аргументами
    /// обязан схлопнуться.
    #[test]
    fn same_turn_and_suffix_give_the_same_key() {
        let effect = ChatEffect::new(actor(), "c1", 7);
        assert_eq!(
            format!("{}:run_quality_check:qc1", effect.scope),
            format!("{}:run_quality_check:qc1", effect.scope)
        );
        assert_eq!(effect.scope, "chat:c1@7");
    }

    /// Фоновый сценарий без диалога всё равно обязан дать ключ: эффект без
    /// ключа `actions::run` не примет.
    #[test]
    fn missing_chat_still_yields_a_scope() {
        assert_eq!(ChatEffect::new(actor(), "   ", 1).scope, "chat:no-chat@1");
    }
}
