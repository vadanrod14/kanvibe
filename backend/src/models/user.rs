use crate::error::ApiError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteQueryResult, FromRow, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub display_name: Option<String>,
    pub github_login: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateUser {
    pub username: String,
    pub email: String,
    pub display_name: Option<String>,
    pub github_login: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateUser {
    pub username: Option<String>,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub github_login: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct UserSession {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl User {
    pub async fn create(db: &SqlitePool, input: CreateUser) -> Result<User, ApiError> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        
        let result = sqlx::query!(
            r#"
            INSERT INTO users (id, username, email, display_name, github_login, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            id,
            input.username,
            input.email,
            input.display_name,
            input.github_login,
            now,
            now
        )
        .execute(db)
        .await?;

        if result.rows_affected() == 0 {
            return Err(ApiError::InternalServerError("Failed to create user".into()));
        }

        Ok(User {
            id,
            username: input.username,
            email: input.email,
            display_name: input.display_name,
            github_login: input.github_login,
            created_at: now,
            updated_at: now,
        })
    }

    pub async fn find_by_id(db: &SqlitePool, id: Uuid) -> Result<Option<User>, ApiError> {
        let user = sqlx::query_as!(
            User,
            r#"
            SELECT id as "id: Uuid", username, email, display_name, github_login,
                   created_at as "created_at: DateTime<Utc>", 
                   updated_at as "updated_at: DateTime<Utc>"
            FROM users
            WHERE id = ?1
            "#,
            id
        )
        .fetch_optional(db)
        .await?;

        Ok(user)
    }

    pub async fn find_by_email(db: &SqlitePool, email: &str) -> Result<Option<User>, ApiError> {
        let user = sqlx::query_as!(
            User,
            r#"
            SELECT id as "id: Uuid", username, email, display_name, github_login,
                   created_at as "created_at: DateTime<Utc>", 
                   updated_at as "updated_at: DateTime<Utc>"
            FROM users
            WHERE email = ?1
            "#,
            email
        )
        .fetch_optional(db)
        .await?;

        Ok(user)
    }

    pub async fn find_by_github_login(db: &SqlitePool, github_login: &str) -> Result<Option<User>, ApiError> {
        let user = sqlx::query_as!(
            User,
            r#"
            SELECT id as "id: Uuid", username, email, display_name, github_login,
                   created_at as "created_at: DateTime<Utc>", 
                   updated_at as "updated_at: DateTime<Utc>"
            FROM users
            WHERE github_login = ?1
            "#,
            github_login
        )
        .fetch_optional(db)
        .await?;

        Ok(user)
    }

    pub async fn update(db: &SqlitePool, id: Uuid, input: UpdateUser) -> Result<User, ApiError> {
        let user = User::find_by_id(db, id).await?
            .ok_or(ApiError::NotFound("User not found".into()))?;

        let username = input.username.unwrap_or(user.username.clone());
        let email = input.email.unwrap_or(user.email.clone());
        let display_name = input.display_name.or(user.display_name.clone());
        let github_login = input.github_login.or(user.github_login.clone());
        let now = Utc::now();

        sqlx::query!(
            r#"
            UPDATE users
            SET username = ?2, email = ?3, display_name = ?4, github_login = ?5, updated_at = ?6
            WHERE id = ?1
            "#,
            id,
            username,
            email,
            display_name,
            github_login,
            now
        )
        .execute(db)
        .await?;

        User::find_by_id(db, id).await?
            .ok_or(ApiError::InternalServerError("Failed to fetch updated user".into()))
    }

    pub async fn delete(db: &SqlitePool, id: Uuid) -> Result<SqliteQueryResult, ApiError> {
        let result = sqlx::query!(
            r#"
            DELETE FROM users
            WHERE id = ?1
            "#,
            id
        )
        .execute(db)
        .await?;

        Ok(result)
    }
}

impl UserSession {
    pub async fn create(db: &SqlitePool, user_id: Uuid, hours_valid: i64) -> Result<UserSession, ApiError> {
        let id = Uuid::new_v4();
        let token = Uuid::new_v4().to_string();
        let now = Utc::now();
        let expires_at = now + chrono::Duration::hours(hours_valid);
        
        sqlx::query!(
            r#"
            INSERT INTO user_sessions (id, user_id, token, expires_at, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            id,
            user_id,
            token,
            expires_at,
            now
        )
        .execute(db)
        .await?;

        Ok(UserSession {
            id,
            user_id,
            token,
            expires_at,
            created_at: now,
        })
    }

    pub async fn find_by_token(db: &SqlitePool, token: &str) -> Result<Option<UserSession>, ApiError> {
        let session = sqlx::query_as!(
            UserSession,
            r#"
            SELECT id as "id: Uuid", user_id as "user_id: Uuid", token,
                   expires_at as "expires_at: DateTime<Utc>",
                   created_at as "created_at: DateTime<Utc>"
            FROM user_sessions
            WHERE token = ?1 AND expires_at > datetime('now')
            "#,
            token
        )
        .fetch_optional(db)
        .await?;

        Ok(session)
    }

    pub async fn delete_expired(db: &SqlitePool) -> Result<SqliteQueryResult, ApiError> {
        let result = sqlx::query!(
            r#"
            DELETE FROM user_sessions
            WHERE expires_at < datetime('now')
            "#
        )
        .execute(db)
        .await?;

        Ok(result)
    }
}