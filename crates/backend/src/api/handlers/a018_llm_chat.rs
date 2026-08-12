use axum::{
    body::Body,
    extract::{Multipart, Path, Query},
    http::{header, StatusCode},
    response::Response,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::domain::a018_llm_chat;
use crate::domain::a018_llm_chat::job_store::{self, LlmJobStatus};
use crate::system::auth::extractor::CurrentUser;
use contracts::domain::a018_llm_chat::aggregate::{
    LlmChat, LlmChatDetail, LlmChatListItem, LlmChatMessage, ToolTraceEntry,
};
use contracts::domain::a018_llm_chat::context::{AddContextRequest, ContextPackageSummary};
use contracts::domain::a018_llm_chat::workspace::{
    AnswerQuestionRequest, ChatFileContent, ChatWorkspaceView, SaveChatFileRequest,
};

/// Тело POST .../workspace/active.
#[derive(Deserialize)]
pub struct SetActiveActivityRequest {
    pub name: String,
}

#[derive(Deserialize)]
pub struct LlmChatListParams {
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

#[derive(Deserialize)]
pub struct SetModelRequest {
    pub model_name: String,
}

#[derive(Serialize)]
pub struct LlmChatPaginatedResponse {
    pub items: Vec<LlmChat>,
    pub total: u64,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

/// GET /api/a018-llm-chat
pub async fn list_all() -> Result<Json<Vec<LlmChat>>, axum::http::StatusCode> {
    match a018_llm_chat::service::list_all().await {
        Ok(v) => Ok(Json(v)),
        Err(e) => {
            tracing::error!("a018 list_all failed: {e}");
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// GET /api/a018-llm-chat/with-stats
pub async fn list_with_stats(
    CurrentUser(claims): CurrentUser,
) -> Result<Json<Vec<LlmChatListItem>>, axum::http::StatusCode> {
    match a018_llm_chat::service::list_with_stats(&claims.sub, claims.is_admin).await {
        Ok(v) => Ok(Json(v)),
        Err(e) => {
            tracing::error!("a018 list_with_stats failed: {e}");
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// GET /api/a018-llm-chat/list
pub async fn list_paginated(
    Query(params): Query<LlmChatListParams>,
) -> Result<Json<LlmChatPaginatedResponse>, axum::http::StatusCode> {
    let limit = params.limit.unwrap_or(100).clamp(10, 10000);
    let offset = params.offset.unwrap_or(0);
    let page = (offset / limit) as u64;

    match a018_llm_chat::service::list_paginated(page, limit).await {
        Ok((items, total)) => {
            let page_size = limit as usize;
            let page = (offset as usize) / page_size;
            let total_pages = ((total as usize) + page_size - 1) / page_size;

            Ok(Json(LlmChatPaginatedResponse {
                items,
                total,
                page,
                page_size,
                total_pages,
            }))
        }
        Err(e) => {
            tracing::error!("a018 list_paginated failed: {e}");
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// GET /api/a018-llm-chat/:id
pub async fn get_by_id(
    Path(id): Path<String>,
) -> Result<Json<LlmChatDetail>, axum::http::StatusCode> {
    match a018_llm_chat::service::get_by_id(&id).await {
        Ok(Some(v)) => Ok(Json(v)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("a018 get_by_id({id}) failed: {e}");
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// DELETE /api/a018-llm-chat/:id
pub async fn delete(Path(id): Path<String>) -> Result<(), axum::http::StatusCode> {
    match a018_llm_chat::service::delete(&id).await {
        Ok(()) => Ok(()),
        Err(e) => {
            tracing::error!("a018 delete({id}) failed: {e}");
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// POST /api/a018-llm-chat/:id/model
pub async fn set_model(
    Path(id): Path<String>,
    Json(payload): Json<SetModelRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match a018_llm_chat::service::set_model(&id, payload.model_name).await {
        Ok(()) => Ok(Json(json!({"success": true}))),
        Err(error) => {
            tracing::warn!("a018 set_model({id}) rejected: {error}");
            Err((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": error.to_string()})),
            ))
        }
    }
}

/// POST /api/a018-llm-chat
pub async fn upsert(
    CurrentUser(claims): CurrentUser,
    Json(dto): Json<a018_llm_chat::service::LlmChatDto>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    if dto.id.is_some() {
        // Update
        match a018_llm_chat::service::update(dto).await {
            Ok(_) => Ok(Json(json!({"success": true}))),
            Err(e) => {
                tracing::error!("Failed to update LLM chat: {}", e);
                Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        // Create — владелец = текущий пользователь.
        match a018_llm_chat::service::create(dto, Some(claims.sub)).await {
            Ok(id) => Ok(Json(json!({"success": true, "id": id.to_string()}))),
            Err(e) => {
                tracing::error!("Failed to create LLM chat: {}", e);
                Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

/// GET /api/a018-llm-chat/:id/messages
pub async fn get_messages(
    Path(id): Path<String>,
) -> Result<Json<Vec<LlmChatMessage>>, axum::http::StatusCode> {
    match a018_llm_chat::service::get_messages(&id).await {
        Ok(v) => Ok(Json(v)),
        Err(e) => {
            tracing::error!("a018 get_messages({id}) failed: {e}");
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// GET /api/a018-llm-chat/message/:message_id/tool-trace
/// Полный журнал вызовов инструментов для сообщения ассистента.
pub async fn get_tool_trace(
    Path(message_id): Path<String>,
) -> Result<Json<Vec<ToolTraceEntry>>, axum::http::StatusCode> {
    match a018_llm_chat::service::get_tool_trace(&message_id).await {
        Ok(v) => Ok(Json(v)),
        Err(e) => {
            tracing::error!("a018 get_tool_trace({message_id}) failed: {e}");
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Deserialize)]
pub struct SetRatingRequest {
    /// 1..5, либо null чтобы снять оценку.
    pub rating: Option<i32>,
}

/// POST /api/a018-llm-chat/:id/rating
pub async fn set_rating(
    Path(id): Path<String>,
    Json(payload): Json<SetRatingRequest>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    match a018_llm_chat::service::set_rating(&id, payload.rating).await {
        Ok(()) => Ok(Json(json!({ "success": true }))),
        Err(e) => {
            tracing::warn!("set_rating failed for chat {}: {}", id, e);
            Err(axum::http::StatusCode::BAD_REQUEST)
        }
    }
}

#[derive(Deserialize)]
pub struct SetSharedRequest {
    pub is_shared: bool,
}

/// POST /api/a018-llm-chat/:id/shared
/// Переключить признак «Общий доступ». Разрешено владельцу чата или superadmin.
pub async fn set_shared(
    CurrentUser(claims): CurrentUser,
    Path(id): Path<String>,
    Json(payload): Json<SetSharedRequest>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    match a018_llm_chat::service::set_shared(&id, payload.is_shared, &claims.sub, claims.is_admin)
        .await
    {
        Ok(()) => Ok(Json(json!({ "success": true }))),
        Err(e) => {
            tracing::warn!("set_shared failed for chat {}: {}", id, e);
            Err(axum::http::StatusCode::FORBIDDEN)
        }
    }
}

#[derive(Serialize)]
pub struct SendJobResponse {
    pub job_id: String,
}

#[derive(Serialize)]
pub struct JobStatusResponse {
    pub status: String, // "pending" | "done" | "error"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<LlmChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Текущий этап выполнения (только для status == "pending").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<job_store::JobProgress>,
}

/// POST /api/a018-llm-chat/:id/messages
/// Immediately returns 202 Accepted with a job_id.
/// The LLM call runs in background; poll GET /jobs/:job_id for the result.
pub async fn send_message(
    Path(id): Path<String>,
    CurrentUser(claims): CurrentUser,
    Json(payload): Json<a018_llm_chat::service::SendMessageRequest>,
) -> Result<(StatusCode, Json<SendJobResponse>), StatusCode> {
    let database_activity = crate::system::maintenance::try_begin_database_activity()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    // Собеседник переезжает в фоновую задачу: инструменты, действующие от лица
    // пользователя (тикеты), должны знать автора, а сам HTTP-запрос к моменту их
    // выполнения уже завершится.
    let actor = crate::shared::llm::types::ToolCaller {
        user_id: claims.sub.clone(),
        username: claims.username.clone(),
        is_admin: claims.is_admin,
        primary_role: claims.primary_role.clone(),
    };
    let proposed_job_id = Uuid::new_v4().to_string();
    let request_id = payload
        .request_id
        .clone()
        .unwrap_or_else(|| proposed_job_id.clone());
    let (job_id, should_start) = job_store::register(&proposed_job_id, &id, &request_id)
        .await
        .map_err(|error| {
            tracing::error!("failed to register durable LLM job: {error}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if !should_start {
        return Ok((StatusCode::ACCEPTED, Json(SendJobResponse { job_id })));
    }

    let job_id_clone = job_id.clone();
    tokio::spawn(async move {
        let _database_activity = database_activity;
        tracing::info!("[llm_job] started job_id={} chat_id={}", job_id_clone, id);
        match a018_llm_chat::service::send_message(&id, payload, Some(&job_id_clone), Some(actor))
            .await
        {
            Ok(msg) => {
                tracing::info!("[llm_job] done job_id={}", job_id_clone);
                job_store::complete(&job_id_clone, msg).await;
            }
            Err(e) => {
                tracing::error!("[llm_job] error job_id={} err={}", job_id_clone, e);
                job_store::fail(&job_id_clone, e.to_string()).await;
            }
        }
    });

    Ok((StatusCode::ACCEPTED, Json(SendJobResponse { job_id })))
}

/// POST /api/a018-llm-chat/jobs/:job_id/cancel
pub async fn cancel_job(Path(job_id): Path<String>) -> Result<Json<serde_json::Value>, StatusCode> {
    let cancelled = job_store::cancel(&job_id).await.map_err(|e| {
        tracing::error!("a018 cancel_job({job_id}) failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if cancelled {
        Ok(Json(json!({ "ok": true })))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

/// GET /api/a018-llm-chat/jobs/:job_id
/// Returns current status of a background LLM job.
pub async fn poll_job(Path(job_id): Path<String>) -> Result<Json<JobStatusResponse>, StatusCode> {
    match job_store::take(&job_id).await {
        None => Err(StatusCode::NOT_FOUND),
        Some(LlmJobStatus::Pending(progress)) => Ok(Json(JobStatusResponse {
            status: "pending".to_string(),
            message: None,
            error: None,
            progress: Some(progress),
        })),
        Some(LlmJobStatus::Done(msg)) => Ok(Json(JobStatusResponse {
            status: "done".to_string(),
            message: Some(msg),
            error: None,
            progress: None,
        })),
        Some(LlmJobStatus::Error(e)) => Ok(Json(JobStatusResponse {
            status: "error".to_string(),
            message: None,
            error: Some(e),
            progress: None,
        })),
    }
}

/// GET /api/a018-llm-chat/jobs/:job_id/stream
/// SSE-стрим статуса job'а: события `progress` (смена этапа), `delta` (новый кусок
/// текста ответа), `done` (финальное сообщение) и `error`. Источник истины тот же,
/// что у poll_job (job_store); соединение закрывается после терминального события.
pub async fn stream_job(
    Path(job_id): Path<String>,
) -> axum::response::sse::Sse<
    impl futures_util::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>,
> {
    use axum::response::sse::{Event, KeepAlive, Sse};

    struct StreamState {
        job_id: String,
        sent_bytes: usize,
        last_stage: String,
        finished: bool,
    }

    let state = StreamState {
        job_id,
        sent_bytes: 0,
        last_stage: String::new(),
        finished: false,
    };

    let stream = futures_util::stream::unfold(state, |mut st| async move {
        if st.finished {
            return None;
        }
        loop {
            match job_store::take(&st.job_id).await {
                None => {
                    st.finished = true;
                    return Some((
                        Ok(Event::default().event("error").data("job not found")),
                        st,
                    ));
                }
                Some(LlmJobStatus::Pending(progress)) => {
                    // Сначала дельты текста, затем смена этапа
                    if let Some(partial) = &progress.partial_text {
                        if partial.len() > st.sent_bytes {
                            let delta = partial[st.sent_bytes..].to_string();
                            st.sent_bytes = partial.len();
                            let payload = serde_json::to_string(&json!({ "text": delta }))
                                .unwrap_or_default();
                            return Some((Ok(Event::default().event("delta").data(payload)), st));
                        }
                    }
                    if progress.stage != st.last_stage {
                        st.last_stage = progress.stage.clone();
                        let payload = serde_json::to_string(
                            &json!({ "step": progress.step, "stage": progress.stage }),
                        )
                        .unwrap_or_default();
                        return Some((Ok(Event::default().event("progress").data(payload)), st));
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                }
                Some(LlmJobStatus::Done(msg)) => {
                    st.finished = true;
                    let payload = serde_json::to_string(&msg).unwrap_or_default();
                    return Some((Ok(Event::default().event("done").data(payload)), st));
                }
                Some(LlmJobStatus::Error(e)) => {
                    st.finished = true;
                    return Some((Ok(Event::default().event("error").data(e)), st));
                }
            }
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// GET /api/a018-llm-chat/:id/context
/// Список пакетов контекста, привязанных к чату.
pub async fn get_chat_context(
    Path(id): Path<String>,
) -> Result<Json<Vec<ContextPackageSummary>>, StatusCode> {
    match a018_llm_chat::service::list_chat_context(&id).await {
        Ok(v) => Ok(Json(v)),
        Err(e) => {
            tracing::error!("a018 get_chat_context({id}) failed: {e}");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// GET /api/a018-llm-chat-context/:id
/// Получить один пакет контекста (для details-страницы просмотра контекста LLM).
pub async fn get_context_package(
    Path(id): Path<String>,
) -> Result<Json<ContextPackageSummary>, StatusCode> {
    match a018_llm_chat::service::get_context_by_id(&id).await {
        Ok(Some(s)) => Ok(Json(s)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("a018 get_context_package({id}) failed: {e}");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// POST /api/a018-llm-chat/:id/context
/// Собрать контекст текущей страницы по page_key и привязать к чату.
pub async fn add_chat_context(
    Path(id): Path<String>,
    CurrentUser(claims): CurrentUser,
    Json(req): Json<AddContextRequest>,
) -> Result<Json<ContextPackageSummary>, StatusCode> {
    // Снимок навигации кладём только если клиент его просит: аналитическому чату по
    // объекту он не нужен, а обращению в поддержку — наоборот, самое ценное.
    let session_user_id = req.with_session_snapshot.then_some(claims.sub.as_str());
    match a018_llm_chat::service::add_chat_context(
        &id,
        &req.page_key,
        req.label.as_deref(),
        session_user_id,
    )
    .await
    {
        Ok(summary) => Ok(Json(summary)),
        Err(e) => {
            tracing::error!("add_chat_context error: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Serialize)]
pub struct UploadResponse {
    pub id: String,
    pub filename: String,
    pub content_type: String,
    pub file_size: i64,
}

/// POST /api/a018-llm-chat/:id/upload
pub async fn upload_attachment(
    CurrentUser(claims): CurrentUser,
    Path(chat_id): Path<String>,
    mut multipart: Multipart,
) -> Result<Json<UploadResponse>, (StatusCode, Json<serde_json::Value>)> {
    a018_llm_chat::service::ensure_chat_access(&chat_id, &claims.sub, claims.is_admin)
        .await
        .map_err(|_| {
            (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "Нет доступа к чату" })),
            )
        })?;
    match a018_llm_chat::service::upload_attachment(&chat_id, &mut multipart, Some(claims.sub))
        .await
    {
        Ok(attachment) => Ok(Json(UploadResponse {
            id: attachment.id.to_string(),
            filename: attachment.filename,
            content_type: attachment.content_type,
            file_size: attachment.file_size,
        })),
        Err(e) => {
            tracing::error!("Failed to upload attachment: {}", e);
            let message = e.to_string();
            let status = if message.contains("10 MiB")
                || message.contains("length limit")
                || message.contains("body limit")
            {
                StatusCode::PAYLOAD_TOO_LARGE
            } else if message.contains("S3 storage is disabled")
                || message.contains("[s3].bucket")
                || message.contains("[s3].access_key_id")
                || message.contains("[s3].secret_access_key")
            {
                StatusCode::SERVICE_UNAVAILABLE
            } else if message.contains("multipart")
                || message.contains("supported")
                || message.contains("No file")
                || message.contains("No filename")
                || message.contains("Invalid chat ID")
            {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            Err((status, Json(json!({ "error": message }))))
        }
    }
}

/// GET /api/a018-llm-chat/:chat_id/attachments/:attachment_id
pub async fn get_attachment(
    CurrentUser(claims): CurrentUser,
    Path((chat_id, attachment_id)): Path<(String, String)>,
) -> Result<Response, StatusCode> {
    a018_llm_chat::service::ensure_chat_access(&chat_id, &claims.sub, claims.is_admin)
        .await
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let attachment = a018_llm_chat::service::get_attachment(&chat_id, &attachment_id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let bytes = a018_llm_chat::service::load_attachment_bytes(&attachment)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, attachment.content_type)
        .header(header::CACHE_CONTROL, "private, max-age=3600")
        .body(Body::from(bytes))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// DELETE /api/a018-llm-chat/:chat_id/attachments/:attachment_id
pub async fn delete_pending_attachment(
    CurrentUser(claims): CurrentUser,
    Path((chat_id, attachment_id)): Path<(String, String)>,
) -> Result<StatusCode, StatusCode> {
    a018_llm_chat::service::ensure_chat_access(&chat_id, &claims.sub, claims.is_admin)
        .await
        .map_err(|_| StatusCode::FORBIDDEN)?;
    a018_llm_chat::service::delete_pending_attachment(&chat_id, &attachment_id)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(StatusCode::NO_CONTENT)
}

// ─── Рабочий каталог чата ────────────────────────────────────────────────────

/// GET /api/a018-llm-chat/:id/workspace
/// Задачи чата и файлы активной задачи.
pub async fn get_workspace(
    CurrentUser(claims): CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<ChatWorkspaceView>, StatusCode> {
    a018_llm_chat::service::ensure_chat_access(&id, &claims.sub, claims.is_admin)
        .await
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let (activities, files, questions, plan_steps) =
        crate::shared::llm::chat_workspace::view_for_chat(&id)
            .await
            .map_err(|e| {
                tracing::error!("a018 get_workspace({id}) failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    Ok(Json(ChatWorkspaceView {
        activities,
        files,
        questions,
        plan_steps,
    }))
}

/// POST /api/a018-llm-chat/:id/workspace/answer
/// Ответ пользователя на уточняющий вопрос анкеты.
pub async fn answer_intake_question(
    CurrentUser(claims): CurrentUser,
    Path(id): Path<String>,
    Json(body): Json<AnswerQuestionRequest>,
) -> Result<StatusCode, StatusCode> {
    a018_llm_chat::service::ensure_chat_access(&id, &claims.sub, claims.is_admin)
        .await
        .map_err(|_| StatusCode::FORBIDDEN)?;
    crate::shared::llm::chat_workspace::answer_question(&id, &body.question_id, &body.answer)
        .await
        .map_err(|e| {
            tracing::warn!("a018 answer_intake_question({id}) failed: {e}");
            StatusCode::BAD_REQUEST
        })?;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/a018-llm-chat/:id/workspace/active
/// Переключить активную задачу вручную.
pub async fn set_active_activity(
    CurrentUser(claims): CurrentUser,
    Path(id): Path<String>,
    Json(body): Json<SetActiveActivityRequest>,
) -> Result<StatusCode, StatusCode> {
    a018_llm_chat::service::ensure_chat_access(&id, &claims.sub, claims.is_admin)
        .await
        .map_err(|_| StatusCode::FORBIDDEN)?;
    crate::shared::llm::chat_workspace::switch_activity(&id, &body.name)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/a018-llm-chat/:id/workspace/file/*path
pub async fn get_workspace_file(
    CurrentUser(claims): CurrentUser,
    Path((id, path)): Path<(String, String)>,
) -> Result<Json<ChatFileContent>, StatusCode> {
    a018_llm_chat::service::ensure_chat_access(&id, &claims.sub, claims.is_admin)
        .await
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let (content, is_live_document) = crate::shared::llm::chat_workspace::read_file(&id, &path)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok(Json(ChatFileContent {
        path,
        content,
        is_live_document,
    }))
}

/// PUT /api/a018-llm-chat/:id/workspace/file/*path
/// Правка живого документа (анкета, план, заметки). Журнал шагов append-only.
pub async fn save_workspace_file(
    CurrentUser(claims): CurrentUser,
    Path((id, path)): Path<(String, String)>,
    Json(body): Json<SaveChatFileRequest>,
) -> Result<StatusCode, StatusCode> {
    a018_llm_chat::service::ensure_chat_access(&id, &claims.sub, claims.is_admin)
        .await
        .map_err(|_| StatusCode::FORBIDDEN)?;
    crate::shared::llm::chat_workspace::write_file(&id, &path, &body.content)
        .await
        .map_err(|e| {
            tracing::warn!("a018 save_workspace_file({id}, {path}) rejected: {e}");
            StatusCode::BAD_REQUEST
        })?;
    Ok(StatusCode::NO_CONTENT)
}
