use contracts::system::auth::{
    LoginRequest, LoginResponse, RefreshRequest, RefreshResponse, UserInfo,
};
use contracts::system::maintenance::MaintenanceUnavailableDto;
use gloo_net::http::Request;

use crate::shared::api_utils::api_base;

/// Login with username and password
///
/// Сообщение об ошибке возвращается готовым к показу человеку: экран входа —
/// единственное место, где пользователь вообще что-то видит, и «HTTP 503» ему
/// ничего не объясняет.
pub async fn login(username: String, password: String) -> Result<LoginResponse, String> {
    let request = LoginRequest { username, password };

    let response = Request::post(&format!("{}/api/system/auth/login", api_base()))
        .json(&request)
        .map_err(|e| format!("Не удалось сформировать запрос: {}", e))?
        .send()
        .await
        .map_err(|e| format!("Сервер недоступен: {}", e))?;

    if !response.ok() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(login_error_message(status, &body));
    }

    response
        .json::<LoginResponse>()
        .await
        .map_err(|e| format!("Не удалось разобрать ответ сервера: {}", e))
}

/// Человеческий текст отказа во входе.
fn login_error_message(status: u16, body: &str) -> String {
    // Режим обслуживания приходит с телом: показываем причину, которую указал
    // администратор, — она и объясняет, почему вход не работает.
    if let Ok(maintenance) = serde_json::from_str::<MaintenanceUnavailableDto>(body) {
        return match maintenance.custom_reason() {
            Some(reason) => {
                format!("Идут технические работы: {reason}. Вход доступен только администраторам.")
            }
            None => "Идут технические работы. Вход доступен только администраторам.".to_string(),
        };
    }

    match status {
        401 => "Неверный логин или пароль".to_string(),
        403 => "Доступ запрещён".to_string(),
        503 => "Сервис временно недоступен, попробуйте позже".to_string(),
        other => format!("Не удалось войти (код ответа {other})"),
    }
}

/// Refresh access token using refresh token
pub async fn refresh_token(refresh_token: String) -> Result<RefreshResponse, String> {
    let request = RefreshRequest { refresh_token };

    let response = Request::post(&format!("{}/api/system/auth/refresh", api_base()))
        .json(&request)
        .map_err(|e| format!("Failed to serialize request: {}", e))?
        .send()
        .await
        .map_err(|e| format!("Failed to send request: {}", e))?;

    if !response.ok() {
        return Err(format!("Refresh failed: {}", response.status()));
    }

    response
        .json::<RefreshResponse>()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}

/// Logout (revoke refresh token)
pub async fn logout(refresh_token: String) -> Result<(), String> {
    let request = RefreshRequest { refresh_token };

    let response = Request::post(&format!("{}/api/system/auth/logout", api_base()))
        .json(&request)
        .map_err(|e| format!("Failed to serialize request: {}", e))?
        .send()
        .await
        .map_err(|e| format!("Failed to send request: {}", e))?;

    if !response.ok() {
        return Err(format!("Logout failed: {}", response.status()));
    }

    Ok(())
}

/// Get current user info
pub async fn get_current_user(access_token: &str) -> Result<UserInfo, String> {
    let response = Request::get(&format!("{}/api/system/auth/me", api_base()))
        .header("Authorization", &format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| format!("Failed to send request: {}", e))?;

    if !response.ok() {
        return Err(format!("Get current user failed: {}", response.status()));
    }

    response
        .json::<UserInfo>()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}

/// Fetch with authentication (helper function)
pub async fn fetch_with_auth<T>(url: &str, access_token: &str) -> Result<T, String>
where
    T: for<'de> serde::Deserialize<'de>,
{
    let response = Request::get(&format!("{}{}", api_base(), url))
        .header("Authorization", &format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| format!("Failed to send request: {}", e))?;

    if !response.ok() {
        return Err(format!("Request failed: {}", response.status()));
    }

    response
        .json::<T>()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}
