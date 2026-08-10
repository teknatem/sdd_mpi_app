use contracts::system::tickets::{
    CreateTicketCommentRequest, CreateTicketRequest, TicketAttachmentUploadResponse,
    TicketCommentDto, TicketDetailsResponse, TicketDto, TicketListResponse, UpdateTicketRequest,
};
use gloo_net::http::Request as GlooRequest;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{FormData, Request, RequestInit, RequestMode, Response};

use crate::shared::api_utils::api_base;
use crate::shared::auth_download::{download_authenticated_file, open_authenticated_in_new_tab};
use crate::system::auth::storage;

fn auth_header() -> Result<String, String> {
    storage::get_access_token()
        .map(|token| format!("Bearer {}", token))
        .ok_or_else(|| "Not authenticated".to_string())
}

#[derive(Default, Clone)]
pub struct TicketListQuery {
    pub status: Option<String>,
    pub ticket_type: Option<String>,
    pub search: Option<String>,
    pub limit: u64,
    pub offset: u64,
}

pub async fn fetch_tickets(query: &TicketListQuery) -> Result<TicketListResponse, String> {
    let mut url = format!(
        "{}/api/system/tickets?limit={}&offset={}",
        api_base(),
        query.limit.max(1),
        query.offset
    );
    if let Some(status) = &query.status {
        if !status.is_empty() {
            url.push_str(&format!("&status={}", status));
        }
    }
    if let Some(ticket_type) = &query.ticket_type {
        if !ticket_type.is_empty() {
            url.push_str(&format!("&ticket_type={}", ticket_type));
        }
    }
    if let Some(search) = &query.search {
        if !search.trim().is_empty() {
            url.push_str(&format!("&search={}", urlencoding::encode(search.trim())));
        }
    }

    let response = GlooRequest::get(&url)
        .header("Authorization", &auth_header()?)
        .header("Cache-Control", "no-cache")
        .send()
        .await
        .map_err(|e| format!("Failed to send request: {}", e))?;

    if !response.ok() {
        return Err(format!("HTTP {}", response.status()));
    }

    response
        .json::<TicketListResponse>()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}

pub async fn fetch_ticket_details(id: &str) -> Result<TicketDetailsResponse, String> {
    let response = GlooRequest::get(&format!("{}/api/system/tickets/{}", api_base(), id))
        .header("Authorization", &auth_header()?)
        .header("Cache-Control", "no-cache")
        .send()
        .await
        .map_err(|e| format!("Failed to send request: {}", e))?;

    if !response.ok() {
        return Err(format!("HTTP {}", response.status()));
    }

    response
        .json::<TicketDetailsResponse>()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}

pub async fn create_ticket(req: &CreateTicketRequest) -> Result<TicketDto, String> {
    let response = GlooRequest::post(&format!("{}/api/system/tickets", api_base()))
        .header("Authorization", &auth_header()?)
        .json(req)
        .map_err(|e| format!("Failed to serialize request: {}", e))?
        .send()
        .await
        .map_err(|e| format!("Failed to send request: {}", e))?;

    if !response.ok() {
        return Err(format!("HTTP {}", response.status()));
    }

    response
        .json::<TicketDto>()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}

pub async fn update_ticket(id: &str, req: &UpdateTicketRequest) -> Result<TicketDto, String> {
    let response = GlooRequest::put(&format!("{}/api/system/tickets/{}", api_base(), id))
        .header("Authorization", &auth_header()?)
        .json(req)
        .map_err(|e| format!("Failed to serialize request: {}", e))?
        .send()
        .await
        .map_err(|e| format!("Failed to send request: {}", e))?;

    if !response.ok() {
        return Err(format!("HTTP {}", response.status()));
    }

    response
        .json::<TicketDto>()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}

pub async fn delete_ticket(id: &str) -> Result<(), String> {
    let response = GlooRequest::delete(&format!("{}/api/system/tickets/{}", api_base(), id))
        .header("Authorization", &auth_header()?)
        .send()
        .await
        .map_err(|e| format!("Failed to send request: {}", e))?;

    if !response.ok() {
        return Err(format!("HTTP {}", response.status()));
    }
    Ok(())
}

pub async fn add_comment(ticket_id: &str, body: String) -> Result<TicketCommentDto, String> {
    let req = CreateTicketCommentRequest { body };
    let response = GlooRequest::post(&format!(
        "{}/api/system/tickets/{}/comments",
        api_base(),
        ticket_id
    ))
    .header("Authorization", &auth_header()?)
    .json(&req)
    .map_err(|e| format!("Failed to serialize request: {}", e))?
    .send()
    .await
    .map_err(|e| format!("Failed to send request: {}", e))?;

    if !response.ok() {
        return Err(format!("HTTP {}", response.status()));
    }

    response
        .json::<TicketCommentDto>()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}

/// Загрузка вложения (multipart). `comment_id` — если вложение относится к комментарию.
pub async fn upload_attachment(
    ticket_id: &str,
    comment_id: Option<&str>,
    file: web_sys::File,
) -> Result<TicketAttachmentUploadResponse, String> {
    let form_data = FormData::new().map_err(|e| format!("{e:?}"))?;
    if let Some(comment_id) = comment_id {
        form_data
            .append_with_str("comment_id", comment_id)
            .map_err(|e| format!("{e:?}"))?;
    }
    let filename = file.name();
    form_data
        .append_with_blob_and_filename("file", &file, &filename)
        .map_err(|e| format!("{e:?}"))?;

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(RequestMode::Cors);
    opts.set_body(&form_data);

    let url = format!(
        "{}/api/system/tickets/{}/attachments",
        api_base(),
        ticket_id
    );
    let request = Request::new_with_str_and_init(&url, &opts).map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Authorization", &auth_header()?)
        .map_err(|e| format!("{e:?}"))?;

    let window = web_sys::window().ok_or_else(|| "No window object".to_string())?;
    let response_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("Upload request failed: {e:?}"))?;
    let response: Response = response_value
        .dyn_into()
        .map_err(|e| format!("Invalid upload response: {e:?}"))?;

    if !response.ok() {
        return Err(format!("Upload failed: HTTP {}", response.status()));
    }

    let text = JsFuture::from(response.text().map_err(|e| format!("{e:?}"))?)
        .await
        .map_err(|e| format!("{e:?}"))?
        .as_string()
        .ok_or_else(|| "Upload response is not text".to_string())?;
    serde_json::from_str::<TicketAttachmentUploadResponse>(&text)
        .map_err(|e| format!("Failed to parse upload response: {}", e))
}

pub async fn download_attachment(
    ticket_id: &str,
    attachment_id: &str,
    fallback_filename: &str,
) -> Result<(), String> {
    download_authenticated_file(
        &attachment_download_url(ticket_id, attachment_id),
        fallback_filename,
    )
    .await
}

pub fn attachment_download_url(ticket_id: &str, attachment_id: &str) -> String {
    format!(
        "{}/api/system/tickets/{}/attachments/{}/download",
        api_base(),
        ticket_id,
        attachment_id
    )
}

pub async fn open_attachment_in_new_tab(
    ticket_id: &str,
    attachment_id: &str,
) -> Result<(), String> {
    open_authenticated_in_new_tab(&attachment_download_url(ticket_id, attachment_id)).await
}

pub async fn delete_attachment(ticket_id: &str, attachment_id: &str) -> Result<(), String> {
    let response = GlooRequest::delete(&format!(
        "{}/api/system/tickets/{}/attachments/{}",
        api_base(),
        ticket_id,
        attachment_id
    ))
    .header("Authorization", &auth_header()?)
    .send()
    .await
    .map_err(|e| format!("Failed to send request: {}", e))?;

    if !response.ok() {
        return Err(format!("HTTP {}", response.status()));
    }
    Ok(())
}
