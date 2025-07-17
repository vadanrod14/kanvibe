use crate::app_state::AppState;
use crate::error::ApiError;
use crate::extractors::AuthUser;
use crate::models::api_response::ApiResponse;
use crate::models::user::{CreateUser, User, UserSession};
use axum::{extract::State, response::Json as ResponseJson};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub github_token: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: User,
}

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub display_name: Option<String>,
    pub github_token: Option<String>,
}

pub async fn login(
    State(state): State<AppState>,
    ResponseJson(payload): ResponseJson<LoginRequest>,
) -> Result<ResponseJson<ApiResponse<LoginResponse>>, ApiError> {
    let pool = &state.pool;
    
    // For now, we'll create a simple email-based login
    // In production, you'd want to verify GitHub token or use proper auth
    let user = User::find_by_email(pool, &payload.email)
        .await?
        .ok_or(ApiError::NotFound("User not found".into()))?;

    // Create session
    let session = UserSession::create(pool, user.id, 24 * 7).await?; // 7 days

    Ok(ResponseJson(ApiResponse::success(LoginResponse {
        token: session.token,
        user,
    })))
}

pub async fn register(
    State(state): State<AppState>,
    ResponseJson(payload): ResponseJson<RegisterRequest>,
) -> Result<ResponseJson<ApiResponse<LoginResponse>>, ApiError> {
    let pool = &state.pool;
    
    // Check if user already exists
    if let Some(_) = User::find_by_email(pool, &payload.email).await? {
        return Err(ApiError::BadRequest("Email already registered".into()));
    }

    // If GitHub token is provided, fetch GitHub user info
    let github_login = if let Some(github_token) = &payload.github_token {
        // In production, validate the GitHub token and fetch user info
        // For now, we'll just use a placeholder
        Some(payload.username.clone())
    } else {
        None
    };

    // Create user
    let user = User::create(
        pool,
        CreateUser {
            username: payload.username.clone(),
            email: payload.email.clone(),
            display_name: payload.display_name.clone(),
            github_login,
        },
    )
    .await?;

    // Create session
    let session = UserSession::create(pool, user.id, 24 * 7).await?; // 7 days

    Ok(ResponseJson(ApiResponse::success(LoginResponse {
        token: session.token,
        user,
    })))
}

pub async fn logout(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> Result<ResponseJson<ApiResponse<&'static str>>, ApiError> {
    let pool = &state.pool;
    
    // Delete the session
    sqlx::query!(
        "DELETE FROM user_sessions WHERE id = ?",
        auth_user.0.session.id
    )
    .execute(pool)
    .await?;

    Ok(ResponseJson(ApiResponse::success("Logged out successfully")))
}

pub async fn me(
    auth_user: AuthUser,
) -> Result<ResponseJson<ApiResponse<User>>, ApiError> {
    Ok(ResponseJson(ApiResponse::success(auth_user.0.user)))
}