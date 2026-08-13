//! Приём писем через IMAP (синхронный `imap` crate).
//!
//! Все функции блокирующие — вызываются из `mod.rs` через `spawn_blocking`.

use super::{EmailFull, EmailHeader};
use crate::shared::config::get_mail_config;
use imap::types::Flag;
use mail_parser::MessageParser;
use std::sync::Mutex;
use std::time::{Duration, Instant};

type ImapSession = imap::Session<imap::Connection>;

/// Как долго держим простаивающую сессию в пуле, прежде чем переоткрыть.
/// Короче типичного серверного idle-timeout, чтобы reuse почти всегда попадал в живое соединение.
const IDLE_TTL: Duration = Duration::from_secs(120);

struct Pooled {
    session: ImapSession,
    last_used: Instant,
}

/// Пул на одну переиспользуемую IMAP-сессию (ящик один; операции сериализуются
/// через `spawn_blocking`, так что больше одной живой сессии не нужно).
static POOL: Mutex<Option<Pooled>> = Mutex::new(None);

/// Открыть аутентифицированную IMAP-сессию поверх TLS.
fn open_session() -> anyhow::Result<ImapSession> {
    let cfg = get_mail_config();
    cfg.validate_ready()?;

    let client = imap::ClientBuilder::new(&cfg.imap_host, cfg.imap_port)
        .connect()
        .map_err(|e| {
            anyhow::anyhow!(
                "IMAP connect to {}:{} failed: {e}",
                cfg.imap_host,
                cfg.imap_port
            )
        })?;

    let session = client
        .login(&cfg.username, &cfg.password)
        .map_err(|(e, _)| anyhow::anyhow!("IMAP login failed: {e}"))?;

    Ok(session)
}

/// Взять сессию из пула (если свежа и жива — проверяем NOOP) или открыть новую.
fn take_or_open() -> anyhow::Result<ImapSession> {
    // Забираем кандидата из пула, освобождая лок до (потенциально медленного) NOOP/connect.
    let pooled = POOL.lock().ok().and_then(|mut g| g.take());
    if let Some(mut p) = pooled {
        if p.last_used.elapsed() < IDLE_TTL && p.session.noop().is_ok() {
            return Ok(p.session);
        }
        let _ = p.session.logout(); // протух или мёртв — закрываем и открываем заново
    }
    open_session()
}

/// Вернуть сессию в пул для повторного использования.
fn give_back(session: ImapSession) {
    if let Ok(mut g) = POOL.lock() {
        *g = Some(Pooled {
            session,
            last_used: Instant::now(),
        });
    }
}

/// Выполнить операцию на переиспользуемой сессии. При ошибке сессия не
/// возвращается в пул (logout), чтобы не оставить его в сломанном состоянии.
fn with_session<T>(f: impl FnOnce(&mut ImapSession) -> anyhow::Result<T>) -> anyhow::Result<T> {
    let mut session = take_or_open()?;
    match f(&mut session) {
        Ok(v) => {
            give_back(session);
            Ok(v)
        }
        Err(e) => {
            let _ = session.logout();
            Err(e)
        }
    }
}

/// Последние `limit` писем из INBOX (новые сверху).
pub fn list_inbox(limit: usize) -> anyhow::Result<Vec<EmailHeader>> {
    let limit = limit.clamp(1, 100);
    with_session(|session| {
        let mailbox = session
            .select("INBOX")
            .map_err(|e| anyhow::anyhow!("IMAP select INBOX failed: {e}"))?;

        let exists = mailbox.exists;
        if exists == 0 {
            return Ok(vec![]);
        }

        let start = exists.saturating_sub(limit as u32 - 1).max(1);
        let seq = format!("{start}:{exists}");

        let fetches = session
            .fetch(&seq, "(UID FLAGS RFC822.HEADER)")
            .map_err(|e| anyhow::anyhow!("IMAP fetch failed: {e}"))?;

        let mut out: Vec<EmailHeader> = Vec::new();
        for f in fetches.iter() {
            let uid = f.uid.unwrap_or(0);
            let seen = f.flags().iter().any(|fl| *fl == Flag::Seen);
            let (from, subject, date) = match f.header() {
                Some(bytes) => parse_header_fields(bytes),
                None => (String::new(), String::new(), String::new()),
            };
            out.push(EmailHeader {
                uid,
                from,
                subject,
                date,
                seen,
            });
        }

        // Новые сверху (по UID убыв.).
        out.sort_by(|a, b| b.uid.cmp(&a.uid));
        Ok(out)
    })
}

/// Полное письмо по UID (заголовки + текстовое тело).
pub fn read_email(uid: u32) -> anyhow::Result<EmailFull> {
    with_session(|session| {
        session
            .select("INBOX")
            .map_err(|e| anyhow::anyhow!("IMAP select INBOX failed: {e}"))?;

        let fetches = session
            .uid_fetch(uid.to_string(), "(UID RFC822)")
            .map_err(|e| anyhow::anyhow!("IMAP uid_fetch failed: {e}"))?;

        let f = fetches
            .iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Письмо с UID {uid} не найдено"))?;

        let raw = f
            .body()
            .ok_or_else(|| anyhow::anyhow!("Пустое тело письма UID {uid}"))?;

        let parsed = MessageParser::default()
            .parse(raw)
            .ok_or_else(|| anyhow::anyhow!("Не удалось распарсить письмо UID {uid}"))?;

        let from = addr_to_string(parsed.from());
        let to = addr_to_string(parsed.to());
        let subject = parsed.subject().unwrap_or("").to_string();
        let date = parsed.date().map(|d| d.to_rfc3339()).unwrap_or_default();
        let message_id = parsed.message_id().map(|s| s.to_string());
        let body = parsed
            .body_text(0)
            .map(|c| c.into_owned())
            .or_else(|| parsed.body_html(0).map(|c| strip_html(&c)))
            .unwrap_or_default();

        Ok(EmailFull {
            uid,
            from,
            to,
            subject,
            date,
            body,
            message_id,
        })
    })
}

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Извлечь From/Subject/Date из сырых RFC822-заголовков (с MIME-декодированием).
fn parse_header_fields(bytes: &[u8]) -> (String, String, String) {
    match MessageParser::default().parse(bytes) {
        Some(msg) => {
            let from = addr_to_string(msg.from());
            let subject = msg.subject().unwrap_or("").to_string();
            let date = msg.date().map(|d| d.to_rfc3339()).unwrap_or_default();
            (from, subject, date)
        }
        None => (String::new(), String::new(), String::new()),
    }
}

/// Отформатировать адрес(а) в "Имя <email>" (первый адрес).
fn addr_to_string(a: Option<&mail_parser::Address>) -> String {
    match a.and_then(|addr| addr.first()) {
        Some(m) => {
            let email = m.address().unwrap_or("");
            match m.name() {
                Some(n) if !n.trim().is_empty() => format!("{} <{}>", n.trim(), email),
                _ => email.to_string(),
            }
        }
        None => String::new(),
    }
}

/// Грубое удаление HTML-тегов на случай письма без text/plain части.
fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}
