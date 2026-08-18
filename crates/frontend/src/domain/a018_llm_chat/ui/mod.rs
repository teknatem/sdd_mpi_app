pub mod context_details;
pub mod details;
pub mod header_button;
pub mod launch;
pub mod list;
pub mod skills_list;
pub mod tools_list;

pub use context_details::LlmContextDetails;
pub use header_button::AiChatHeaderButton;
pub use skills_list::LlmSkillList;
pub use tools_list::LlmToolList;

/// Ключ для `AppGlobalContext.form_states`: первое сообщение, которое нужно
/// автоматически отправить при открытии только что созданного чата.
///
/// Создание чата на странице списка сохраняет вопрос пользователя под этим ключом,
/// а страница деталей чата при загрузке забирает его и отправляет как первое сообщение.
pub fn pending_first_message_key(chat_id: &str) -> String {
    format!("a018_pending_first_msg_{}", chat_id)
}

/// Ключ для `AppGlobalContext.form_states`: счётчик-версия прикреплённого к чату
/// контекста. Шапка (`AiChatHeaderButton`) увеличивает его после добавления
/// документа, а открытая страница деталей чата реагирует на изменение и
/// перезагружает ленту контекста без переоткрытия вкладки.
pub fn context_version_key(chat_id: &str) -> String {
    format!("a018_ctx_version_{}", chat_id)
}
