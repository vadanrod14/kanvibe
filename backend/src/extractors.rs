use crate::error::ApiError;
use crate::middleware::auth::AuthenticatedUser;
use axum::{
    async_trait,
    extract::{FromRequestParts, Request},
    http::request::Parts,
};
use uuid::Uuid;

pub struct AuthUser(pub AuthenticatedUser);

#[async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let user = parts
            .extensions
            .get::<AuthenticatedUser>()
            .cloned()
            .ok_or_else(|| ApiError::Unauthorized("User not authenticated".into()))?;

        Ok(AuthUser(user))
    }
}

impl AuthUser {
    pub fn id(&self) -> Uuid {
        self.0.user.id
    }

    pub fn user(&self) -> &crate::models::user::User {
        &self.0.user
    }
}