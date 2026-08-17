use crate::shared::error::ApiError;
use axum::{
    extract::{Path, Query},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::domain::a039_mail_message;
use contracts::domain::a039_mail_message::aggregate::MailMessage;

#[derive(Deserialize)]
pub struct MailMessageListParams {
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub sort_by: Option<String>,
    pub sort_desc: Option<bool>,
}

#[derive(Serialize)]
pub struct MailMessagePaginatedResponse {
    pub items: Vec<MailMessage>,
    pub total: u64,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

pub async fn list_all() -> Result<Json<Vec<MailMessage>>, ApiError> {
    match a039_mail_message::service::list_all().await {
        Ok(v) => Ok(Json(v)),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

pub async fn list_paginated(
    Query(params): Query<MailMessageListParams>,
) -> Result<Json<MailMessagePaginatedResponse>, ApiError> {
    let limit = params.limit.unwrap_or(100).clamp(10, 10000);
    let offset = params.offset.unwrap_or(0);
    let sort_by = params.sort_by.as_deref().unwrap_or("created_at");
    let sort_desc = params.sort_desc.unwrap_or(true);

    match a039_mail_message::service::list_paginated(limit, offset, sort_by, sort_desc).await {
        Ok((items, total)) => {
            let page_size = limit as usize;
            let page = (offset as usize) / page_size;
            let total_pages = ((total as usize) + page_size - 1) / page_size;
            Ok(Json(MailMessagePaginatedResponse {
                items,
                total,
                page,
                page_size,
                total_pages,
            }))
        }
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

pub async fn get_by_id(Path(id): Path<String>) -> Result<Json<MailMessage>, ApiError> {
    match a039_mail_message::service::get_by_id(&id).await {
        Ok(Some(v)) => Ok(Json(v)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND.into()),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}

pub async fn delete(Path(id): Path<String>) -> Result<(), ApiError> {
    match a039_mail_message::service::delete(&id).await {
        Ok(()) => Ok(()),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into()),
    }
}
