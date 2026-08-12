//! Минимальный S3-клиент поверх `reqwest` с собственной подписью SigV4.
//!
//! Подпись умеет query string: без неё нельзя обратиться ни к `?uploads`, ни к
//! `?uploadId=`, ни к `ListObjectsV2` — все они живут в параметрах запроса.
//! Именно это, а не размер объекта, годами блокировало многочастную загрузку.

use std::time::Duration;

use bytes::Bytes;
use chrono::Utc;
use hmac::{Hmac, Mac};
use once_cell::sync::Lazy;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE, HOST};
use reqwest::Method;
use sha2::{Digest, Sha256};

use crate::shared::config::S3Config;

type HmacSha256 = Hmac<Sha256>;

/// Один клиент на процесс: многочастная загрузка делает десятки запросов подряд,
/// и пересоздание `Client` на каждый (как было раньше) заново поднимало бы
/// TCP-соединение и TLS-сессию. Общего таймаута нет намеренно — скачивание
/// гигабайтного объекта идёт потоком и в любой фиксированный лимит не влезет.
static HTTP: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .pool_idle_timeout(Duration::from_secs(90))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
});

pub struct S3Object {
    pub bytes: Bytes,
    pub content_type: Option<String>,
}

/// Часть многочастной загрузки: номер и ETag, который вернул сервер.
#[derive(Debug, Clone)]
pub struct CompletedPart {
    pub part_number: u32,
    pub etag: String,
}

/// Канонический query string для SigV4: параметры отсортированы по
/// закодированному имени, имя и значение закодированы по RFC 3986, пары через
/// `&`. Пустое значение остаётся пустым (`uploads=`), а не выкидывается.
fn canonical_query(query: &[(&str, String)]) -> String {
    let mut pairs: Vec<(String, String)> = query
        .iter()
        .map(|(name, value)| {
            (
                urlencoding::encode(name).into_owned(),
                urlencoding::encode(value).into_owned(),
            )
        })
        .collect();
    pairs.sort();
    pairs
        .into_iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hmac_sha256(key: &[u8], data: &str) -> anyhow::Result<Vec<u8>> {
    let mut mac = HmacSha256::new_from_slice(key)
        .map_err(|err| anyhow::anyhow!("Failed to initialize HMAC: {}", err))?;
    mac.update(data.as_bytes());
    Ok(mac.finalize().into_bytes().to_vec())
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    hex_lower(&Sha256::digest(bytes))
}

fn encode_key(key: &str) -> String {
    key.split('/')
        .map(|part| urlencoding::encode(part).into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn endpoint_base(config: &S3Config) -> String {
    config.endpoint.trim_end_matches('/').to_string()
}

fn request_url(config: &S3Config, key: &str) -> String {
    format!(
        "{}/{}/{}",
        endpoint_base(config),
        urlencoding::encode(&config.bucket),
        encode_key(key)
    )
}

/// URL с query. Строка параметров — ровно та же, что ушла в подпись: любое
/// расхождение между подписанным и отправленным query даёт SignatureDoesNotMatch.
fn request_url_with_query(config: &S3Config, key: &str, query: &[(&str, String)]) -> String {
    let base = request_url(config, key);
    if query.is_empty() {
        return base;
    }
    format!("{base}?{}", canonical_query(query))
}

fn signed_headers(
    config: &S3Config,
    method: &Method,
    key: &str,
    query: &[(&str, String)],
    payload_hash: &str,
    content_type: Option<&str>,
) -> anyhow::Result<HeaderMap> {
    let endpoint = reqwest::Url::parse(&endpoint_base(config))?;
    let host = endpoint
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("S3 endpoint must include host"))?;

    let now = Utc::now();
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date_stamp = now.format("%Y%m%d").to_string();
    let canonical_uri = format!(
        "/{}/{}",
        urlencoding::encode(&config.bucket),
        encode_key(key)
    );

    let mut canonical_headers = format!(
        "host:{}\nx-amz-content-sha256:{}\nx-amz-date:{}\n",
        host, payload_hash, amz_date
    );
    let mut signed_headers = "host;x-amz-content-sha256;x-amz-date".to_string();

    if let Some(content_type) = content_type.filter(|value| !value.trim().is_empty()) {
        canonical_headers.push_str(&format!("content-type:{}\n", content_type));
        signed_headers.push_str(";content-type");
    }

    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        method.as_str(),
        canonical_uri,
        canonical_query(query),
        canonical_headers,
        signed_headers,
        payload_hash
    );
    let credential_scope = format!("{}/{}/s3/aws4_request", date_stamp, config.region);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        amz_date,
        credential_scope,
        sha256_hex(canonical_request.as_bytes())
    );

    let date_key = hmac_sha256(
        format!("AWS4{}", config.secret_access_key).as_bytes(),
        &date_stamp,
    )?;
    let region_key = hmac_sha256(&date_key, &config.region)?;
    let service_key = hmac_sha256(&region_key, "s3")?;
    let signing_key = hmac_sha256(&service_key, "aws4_request")?;
    let signature = hex_lower(&hmac_sha256(&signing_key, &string_to_sign)?);

    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
        config.access_key_id, credential_scope, signed_headers, signature
    );

    let mut headers = HeaderMap::new();
    headers.insert(HOST, HeaderValue::from_str(host)?);
    headers.insert("x-amz-date", HeaderValue::from_str(&amz_date)?);
    headers.insert("x-amz-content-sha256", HeaderValue::from_str(payload_hash)?);
    headers.insert(AUTHORIZATION, HeaderValue::from_str(&authorization)?);
    if let Some(content_type) = content_type.filter(|value| !value.trim().is_empty()) {
        headers.insert(CONTENT_TYPE, HeaderValue::from_str(content_type)?);
    }
    Ok(headers)
}

pub async fn put_object(
    config: &S3Config,
    key: &str,
    content_type: Option<&str>,
    bytes: Bytes,
) -> anyhow::Result<Option<String>> {
    let payload_hash = sha256_hex(&bytes);
    let headers = signed_headers(config, &Method::PUT, key, &[], &payload_hash, content_type)?;
    let response = HTTP
        .put(request_url(config, key))
        .headers(headers)
        .body(bytes)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("S3 PUT failed with {}: {}", status, body));
    }

    Ok(response
        .headers()
        .get("etag")
        .and_then(|value| value.to_str().ok())
        .map(ToString::to_string))
}

async fn get_object_response(config: &S3Config, key: &str) -> anyhow::Result<reqwest::Response> {
    let payload_hash = sha256_hex(&[]);
    let headers = signed_headers(config, &Method::GET, key, &[], &payload_hash, None)?;
    Ok(HTTP
        .get(request_url(config, key))
        .headers(headers)
        .send()
        .await?)
}

/// Ответ GET без вычитывания тела: вызывающий забирает его потоком
/// (`response.bytes_stream()`). Для объектов в гигабайты `bytes()` неприменим.
pub async fn get_object_stream(
    config: &S3Config,
    key: &str,
) -> anyhow::Result<(Option<u64>, reqwest::Response)> {
    let response = get_object_response(config, key).await?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("S3 GET failed with {}: {}", status, body));
    }
    Ok((response.content_length(), response))
}

async fn read_object_body(response: reqwest::Response) -> anyhow::Result<S3Object> {
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(ToString::to_string);
    let bytes = response.bytes().await?;
    Ok(S3Object {
        bytes,
        content_type,
    })
}

pub async fn get_object(config: &S3Config, key: &str) -> anyhow::Result<S3Object> {
    let response = get_object_response(config, key).await?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("S3 GET failed with {}: {}", status, body));
    }
    read_object_body(response).await
}

/// Как `get_object`, но возвращает `None` вместо ошибки, если объекта ещё не существует (404) —
/// нужно для чтения `catalog.json` до самой первой публикации какого-либо плагина.
pub(crate) async fn get_object_opt(
    config: &S3Config,
    key: &str,
) -> anyhow::Result<Option<S3Object>> {
    let response = get_object_response(config, key).await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("S3 GET failed with {}: {}", status, body));
    }
    Ok(Some(read_object_body(response).await?))
}

pub async fn delete_object(config: &S3Config, key: &str) -> anyhow::Result<()> {
    let payload_hash = sha256_hex(&[]);
    let headers = signed_headers(config, &Method::DELETE, key, &[], &payload_hash, None)?;
    let response = HTTP
        .delete(request_url(config, key))
        .headers(headers)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "S3 DELETE failed with {}: {}",
            status,
            body
        ));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Многочастная загрузка
// ---------------------------------------------------------------------------

/// Минимальный размер части в S3 — 5 МиБ (последняя часть может быть меньше),
/// максимальное число частей — 10 000. 64 МиБ дают запас: 640 ГБ одним объектом.
pub const MIN_PART_BYTES: usize = 5 * 1024 * 1024;
pub const DEFAULT_PART_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_PARTS: u64 = 10_000;

/// Размер части под ожидаемый объём. Оценка может быть занижена (например, для
/// сжимаемого потока), поэтому берётся с запасом на 9000 частей, а не на 10 000.
pub fn part_size_for(expected_bytes: u64) -> usize {
    let needed = expected_bytes.div_ceil(9000) as usize;
    needed.max(DEFAULT_PART_BYTES).max(MIN_PART_BYTES)
}

/// Вытаскивает содержимое первого тега `<name>…</name>`. Ответы multipart —
/// три коротких документа с плоской структурой, ради них XML-крейт не нужен.
fn xml_tag<'a>(body: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let start = body.find(&open)? + open.len();
    let end = body[start..].find(&close)? + start;
    Some(&body[start..end])
}

/// S3 умеет ответить 200 OK с телом `<Error>` — молча принять такой ответ за
/// успех означает собрать объект из частей, которых нет.
fn ensure_no_error_body(body: &str, context: &str) -> anyhow::Result<()> {
    if body.contains("<Error>") {
        let code = xml_tag(body, "Code").unwrap_or("Unknown");
        let message = xml_tag(body, "Message").unwrap_or(body);
        anyhow::bail!("{context}: S3 вернул ошибку {code}: {message}");
    }
    Ok(())
}

pub async fn create_multipart_upload(
    config: &S3Config,
    key: &str,
    content_type: Option<&str>,
) -> anyhow::Result<String> {
    let query = [("uploads", String::new())];
    let payload_hash = sha256_hex(&[]);
    let headers = signed_headers(
        config,
        &Method::POST,
        key,
        &query,
        &payload_hash,
        content_type,
    )?;
    let response = HTTP
        .post(request_url_with_query(config, key, &query))
        .headers(headers)
        .send()
        .await?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("S3 CreateMultipartUpload failed with {status}: {body}");
    }
    ensure_no_error_body(&body, "CreateMultipartUpload")?;
    xml_tag(&body, "UploadId")
        .map(ToString::to_string)
        .ok_or_else(|| anyhow::anyhow!("В ответе CreateMultipartUpload нет UploadId: {body}"))
}

pub async fn upload_part(
    config: &S3Config,
    key: &str,
    upload_id: &str,
    part_number: u32,
    bytes: Bytes,
) -> anyhow::Result<CompletedPart> {
    let query = [
        ("partNumber", part_number.to_string()),
        ("uploadId", upload_id.to_string()),
    ];
    let payload_hash = sha256_hex(&bytes);
    let headers = signed_headers(config, &Method::PUT, key, &query, &payload_hash, None)?;
    let response = HTTP
        .put(request_url_with_query(config, key, &query))
        .headers(headers)
        .body(bytes)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("S3 UploadPart {part_number} failed with {status}: {body}");
    }

    let etag = response
        .headers()
        .get("etag")
        .and_then(|value| value.to_str().ok())
        .map(ToString::to_string)
        .ok_or_else(|| anyhow::anyhow!("S3 не вернул ETag для части {part_number}"))?;
    Ok(CompletedPart { part_number, etag })
}

pub async fn complete_multipart_upload(
    config: &S3Config,
    key: &str,
    upload_id: &str,
    parts: &[CompletedPart],
) -> anyhow::Result<()> {
    if parts.is_empty() {
        anyhow::bail!("Многочастная загрузка без единой части");
    }
    let mut body = String::from("<CompleteMultipartUpload>");
    for part in parts {
        body.push_str(&format!(
            "<Part><PartNumber>{}</PartNumber><ETag>{}</ETag></Part>",
            part.part_number, part.etag
        ));
    }
    body.push_str("</CompleteMultipartUpload>");

    let query = [("uploadId", upload_id.to_string())];
    let payload = Bytes::from(body);
    let payload_hash = sha256_hex(&payload);
    let headers = signed_headers(
        config,
        &Method::POST,
        key,
        &query,
        &payload_hash,
        Some("application/xml"),
    )?;
    let response = HTTP
        .post(request_url_with_query(config, key, &query))
        .headers(headers)
        .body(payload)
        .send()
        .await?;

    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("S3 CompleteMultipartUpload failed with {status}: {text}");
    }
    ensure_no_error_body(&text, "CompleteMultipartUpload")
}

/// Отмена загрузки: без неё уже отправленные части остаются в бакете невидимым
/// мусором и продолжают тарифицироваться.
pub async fn abort_multipart_upload(
    config: &S3Config,
    key: &str,
    upload_id: &str,
) -> anyhow::Result<()> {
    let query = [("uploadId", upload_id.to_string())];
    let payload_hash = sha256_hex(&[]);
    let headers = signed_headers(config, &Method::DELETE, key, &query, &payload_hash, None)?;
    let response = HTTP
        .delete(request_url_with_query(config, key, &query))
        .headers(headers)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("S3 AbortMultipartUpload failed with {status}: {body}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_query_is_sorted_and_encoded() {
        let query = [
            ("uploadId", "a/b+c=".to_string()),
            ("partNumber", "7".to_string()),
        ];
        // Сортировка по имени: partNumber < uploadId. Значение кодируется
        // целиком, включая `/`, `+` и `=`.
        assert_eq!(
            canonical_query(&query),
            "partNumber=7&uploadId=a%2Fb%2Bc%3D"
        );
    }

    #[test]
    fn canonical_query_keeps_valueless_parameter() {
        assert_eq!(canonical_query(&[("uploads", String::new())]), "uploads=");
    }

    #[test]
    fn canonical_query_is_empty_without_parameters() {
        // Запросы без query подписываются ровно как раньше: пустая строка на
        // третьей позиции канонического запроса.
        assert_eq!(canonical_query(&[]), "");
    }

    #[test]
    fn part_size_never_drops_below_the_s3_minimum() {
        assert_eq!(part_size_for(0), DEFAULT_PART_BYTES);
        assert_eq!(part_size_for(1024), DEFAULT_PART_BYTES);
    }

    #[test]
    fn part_size_grows_to_stay_within_the_part_limit() {
        // 4 ТБ при 64 МиБ дали бы 65 536 частей — размер обязан вырасти.
        let huge = 4 * 1024 * 1024 * 1024 * 1024_u64;
        let size = part_size_for(huge);
        assert!(huge.div_ceil(size as u64) <= MAX_PARTS);
    }

    #[test]
    fn xml_tag_reads_upload_id() {
        let body = "<?xml version=\"1.0\"?><InitiateMultipartUploadResult>\
                    <Bucket>b</Bucket><Key>k</Key><UploadId>abc123</UploadId>\
                    </InitiateMultipartUploadResult>";
        assert_eq!(xml_tag(body, "UploadId"), Some("abc123"));
    }

    #[test]
    fn error_body_is_rejected_even_with_success_status() {
        let body = "<Error><Code>NoSuchUpload</Code><Message>gone</Message></Error>";
        let error = ensure_no_error_body(body, "CompleteMultipartUpload").unwrap_err();
        assert!(error.to_string().contains("NoSuchUpload"));
    }
}
