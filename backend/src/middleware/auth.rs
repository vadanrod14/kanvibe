use crate::app_state::AppState;
use crate::models::user::{User, UserSession};
use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use sqlx::SqlitePool;
use uuid::Uuid;

#[derive(Clone)]
pub struct AuthenticatedUser {
    pub user: User,
    pub session: UserSession,
}

pub async fn auth_middleware(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let pool = &state.pool;

    // Find session by token
    let session = UserSession::find_by_token(pool, token)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // Find user by session
    let user = User::find_by_id(pool, session.user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // Insert authenticated user into request extensions
    request.extensions_mut().insert(AuthenticatedUser { user, session });

    Ok(next.run(request).await)
}