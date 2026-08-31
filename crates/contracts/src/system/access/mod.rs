//! Access control policy types for the route registry and scope catalog.
//!
//! Separate from `shared/access` which holds runtime primitives (AccessMode, ScopeAccess).
//! These types describe the static policy of the application.

use serde::{Deserialize, Serialize};

// ============================================================================
// Route policy types
// ============================================================================

/// How an endpoint is protected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyMode {
    /// GET → Read, POST/PUT/DELETE/PATCH → All (standard `check_scope`)
    Auto,
    /// Always requires Read regardless of HTTP method (`check_scope_read`)
    ReadOnly,
    /// Requires `is_admin = true` (`require_admin`)
    AdminOnly,
    /// Requires valid JWT but no scope check — a policy violation if intentional
    AuthOnly,
    /// Authenticated by the shared `X-Api-Key` header instead of a JWT
    /// (`check_api_key`) — the external integration API.
    ///
    /// Отдельный режим, а не `AuthOnly` и не `Public`. `AuthOnly` означал бы
    /// «залогинен, но scope забыли» и считался бы нарушением; `Public` — «ключ
    /// не нужен» и завысил бы счётчик открытых маршрутов в аудите. Потребитель
    /// внешнего API анонимен по устройству (ключ один на всех), поэтому
    /// `scope_id` у таких маршрутов всегда `None`.
    ApiKey,
    /// No authentication required — must be in an explicit whitelist
    Public,
}

impl PolicyMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::ReadOnly => "read_only",
            Self::AdminOnly => "admin_only",
            Self::AuthOnly => "auth_only",
            Self::ApiKey => "api_key",
            Self::Public => "public",
        }
    }

    pub fn is_violation(&self) -> bool {
        matches!(self, Self::AuthOnly)
    }
}

/// One entry in the static route registry.
///
/// Every endpoint in the application must have exactly one `RoutePolicy`.
/// Use `scope_id = None` only for `AdminOnly` and `Public` routes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoutePolicy {
    /// HTTP method: "GET", "POST", "PUT", "DELETE", "PATCH", "*"
    pub method: &'static str,
    /// URL path pattern, e.g. "/api/u504/import/start"
    pub path: &'static str,
    /// Scope identifier — None for admin-only or public routes
    pub scope_id: Option<&'static str>,
    /// Protection mode
    pub mode: PolicyMode,
}

// ============================================================================
// Scope descriptor types
// ============================================================================

/// Category of a scope for UI grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeType {
    Aggregate,
    Projection,
    Usecase,
    System,
}

impl ScopeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Aggregate => "aggregate",
            Self::Projection => "projection",
            Self::Usecase => "usecase",
            Self::System => "system",
        }
    }
}

/// Rich descriptor for a scope — used in the permission matrix UI and audit page.
///
/// Lives in compile-time statics (`SCOPE_CATALOG`), serialized for API responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScopeDescriptor {
    /// Stable scope identifier, e.g. "u504_import_from_wildberries"
    pub scope_id: &'static str,
    /// Whether this scope belongs to an aggregate, projection, usecase, or system area
    pub scope_type: ScopeType,
    /// Human-readable name shown in the UI, e.g. "Импорт из Wildberries"
    pub label: &'static str,
    /// One-sentence business description
    pub description: &'static str,
    /// Icon name (lucide), e.g. "download-cloud"
    pub icon: &'static str,
    /// UI grouping category, e.g. "imports", "analytics", "references", "system"
    pub category: &'static str,
    /// What the user can do with `read` access
    pub read_label: &'static str,
    /// What the user can do with `all` access
    pub all_label: &'static str,
}

// ============================================================================
// Serializable DTO versions for API responses
// ============================================================================

/// Serializable version of `ScopeDescriptor` for `GET /api/system/scopes`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeDescriptorDto {
    pub scope_id: String,
    pub scope_type: ScopeType,
    pub label: String,
    pub description: String,
    pub icon: String,
    pub category: String,
    pub read_label: String,
    pub all_label: String,
}

impl From<&ScopeDescriptor> for ScopeDescriptorDto {
    fn from(s: &ScopeDescriptor) -> Self {
        Self {
            scope_id: s.scope_id.to_string(),
            scope_type: s.scope_type,
            label: s.label.to_string(),
            description: s.description.to_string(),
            icon: s.icon.to_string(),
            category: s.category.to_string(),
            read_label: s.read_label.to_string(),
            all_label: s.all_label.to_string(),
        }
    }
}
