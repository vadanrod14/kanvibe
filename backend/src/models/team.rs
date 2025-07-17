use crate::error::ApiError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteQueryResult, FromRow, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct Team {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateTeam {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateTeam {
    pub name: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum TeamRole {
    #[serde(rename = "owner")]
    Owner,
    #[serde(rename = "admin")]
    Admin,
    #[serde(rename = "member")]
    Member,
    #[serde(rename = "viewer")]
    Viewer,
}

impl TeamRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            TeamRole::Owner => "owner",
            TeamRole::Admin => "admin",
            TeamRole::Member => "member",
            TeamRole::Viewer => "viewer",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "owner" => Some(TeamRole::Owner),
            "admin" => Some(TeamRole::Admin),
            "member" => Some(TeamRole::Member),
            "viewer" => Some(TeamRole::Viewer),
            _ => None,
        }
    }

    pub fn can_edit(&self) -> bool {
        matches!(self, TeamRole::Owner | TeamRole::Admin | TeamRole::Member)
    }

    pub fn can_manage_members(&self) -> bool {
        matches!(self, TeamRole::Owner | TeamRole::Admin)
    }

    pub fn can_delete(&self) -> bool {
        matches!(self, TeamRole::Owner)
    }
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct TeamMember {
    pub id: Uuid,
    pub team_id: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TeamMemberWithUser {
    pub id: Uuid,
    pub team_id: Uuid,
    pub user_id: Uuid,
    pub role: TeamRole,
    pub joined_at: DateTime<Utc>,
    pub username: String,
    pub email: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct TeamInvitation {
    pub id: Uuid,
    pub team_id: Uuid,
    pub email: String,
    pub invited_by: Uuid,
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl Team {
    pub async fn create(db: &SqlitePool, input: CreateTeam, created_by: Uuid) -> Result<Team, ApiError> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        
        let result = sqlx::query!(
            r#"
            INSERT INTO teams (id, name, description, created_by, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            id,
            input.name,
            input.description,
            created_by,
            now,
            now
        )
        .execute(db)
        .await?;

        if result.rows_affected() == 0 {
            return Err(ApiError::InternalServerError("Failed to create team".into()));
        }

        // Add creator as team owner
        let member_id = Uuid::new_v4();
        sqlx::query!(
            r#"
            INSERT INTO team_members (id, team_id, user_id, role, joined_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            member_id,
            id,
            created_by,
            TeamRole::Owner.as_str(),
            now
        )
        .execute(db)
        .await?;

        Ok(Team {
            id,
            name: input.name,
            description: input.description,
            created_by,
            created_at: now,
            updated_at: now,
        })
    }

    pub async fn find_by_id(db: &SqlitePool, id: Uuid) -> Result<Option<Team>, ApiError> {
        let team = sqlx::query_as!(
            Team,
            r#"
            SELECT id as "id: Uuid", name, description, created_by as "created_by: Uuid",
                   created_at as "created_at: DateTime<Utc>", 
                   updated_at as "updated_at: DateTime<Utc>"
            FROM teams
            WHERE id = ?1
            "#,
            id
        )
        .fetch_optional(db)
        .await?;

        Ok(team)
    }

    pub async fn find_by_user(db: &SqlitePool, user_id: Uuid) -> Result<Vec<Team>, ApiError> {
        let teams = sqlx::query_as!(
            Team,
            r#"
            SELECT t.id as "id: Uuid", t.name, t.description, 
                   t.created_by as "created_by: Uuid",
                   t.created_at as "created_at: DateTime<Utc>", 
                   t.updated_at as "updated_at: DateTime<Utc>"
            FROM teams t
            JOIN team_members tm ON t.id = tm.team_id
            WHERE tm.user_id = ?1
            ORDER BY t.name
            "#,
            user_id
        )
        .fetch_all(db)
        .await?;

        Ok(teams)
    }

    pub async fn update(db: &SqlitePool, id: Uuid, input: UpdateTeam) -> Result<Team, ApiError> {
        let team = Team::find_by_id(db, id).await?
            .ok_or(ApiError::NotFound("Team not found".into()))?;

        let name = input.name.unwrap_or(team.name.clone());
        let description = input.description.or(team.description.clone());
        let now = Utc::now();

        sqlx::query!(
            r#"
            UPDATE teams
            SET name = ?2, description = ?3, updated_at = ?4
            WHERE id = ?1
            "#,
            id,
            name,
            description,
            now
        )
        .execute(db)
        .await?;

        Team::find_by_id(db, id).await?
            .ok_or(ApiError::InternalServerError("Failed to fetch updated team".into()))
    }

    pub async fn delete(db: &SqlitePool, id: Uuid) -> Result<SqliteQueryResult, ApiError> {
        let result = sqlx::query!(
            r#"
            DELETE FROM teams
            WHERE id = ?1
            "#,
            id
        )
        .execute(db)
        .await?;

        Ok(result)
    }

    pub async fn get_member_role(db: &SqlitePool, team_id: Uuid, user_id: Uuid) -> Result<Option<TeamRole>, ApiError> {
        let role = sqlx::query_scalar!(
            r#"
            SELECT role
            FROM team_members
            WHERE team_id = ?1 AND user_id = ?2
            "#,
            team_id,
            user_id
        )
        .fetch_optional(db)
        .await?;

        Ok(role.and_then(|r| TeamRole::from_str(&r)))
    }

    pub async fn add_member(db: &SqlitePool, team_id: Uuid, user_id: Uuid, role: TeamRole) -> Result<(), ApiError> {
        let id = Uuid::new_v4();
        let now = Utc::now();

        sqlx::query!(
            r#"
            INSERT INTO team_members (id, team_id, user_id, role, joined_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            id,
            team_id,
            user_id,
            role.as_str(),
            now
        )
        .execute(db)
        .await?;

        Ok(())
    }

    pub async fn update_member_role(db: &SqlitePool, team_id: Uuid, user_id: Uuid, role: TeamRole) -> Result<(), ApiError> {
        sqlx::query!(
            r#"
            UPDATE team_members
            SET role = ?3
            WHERE team_id = ?1 AND user_id = ?2
            "#,
            team_id,
            user_id,
            role.as_str()
        )
        .execute(db)
        .await?;

        Ok(())
    }

    pub async fn remove_member(db: &SqlitePool, team_id: Uuid, user_id: Uuid) -> Result<(), ApiError> {
        sqlx::query!(
            r#"
            DELETE FROM team_members
            WHERE team_id = ?1 AND user_id = ?2
            "#,
            team_id,
            user_id
        )
        .execute(db)
        .await?;

        Ok(())
    }

    pub async fn get_members(db: &SqlitePool, team_id: Uuid) -> Result<Vec<TeamMemberWithUser>, ApiError> {
        let members = sqlx::query_as!(
            TeamMemberWithUser,
            r#"
            SELECT tm.id as "id: Uuid", tm.team_id as "team_id: Uuid", 
                   tm.user_id as "user_id: Uuid", tm.role as "role: String",
                   tm.joined_at as "joined_at: DateTime<Utc>",
                   u.username, u.email, u.display_name
            FROM team_members tm
            JOIN users u ON tm.user_id = u.id
            WHERE tm.team_id = ?1
            ORDER BY tm.joined_at
            "#,
            team_id
        )
        .fetch_all(db)
        .await?
        .into_iter()
        .filter_map(|mut m| {
            TeamRole::from_str(&m.role).map(|role| {
                TeamMemberWithUser {
                    id: m.id,
                    team_id: m.team_id,
                    user_id: m.user_id,
                    role,
                    joined_at: m.joined_at,
                    username: m.username,
                    email: m.email,
                    display_name: m.display_name,
                }
            })
        })
        .collect();

        Ok(members)
    }
}

impl TeamInvitation {
    pub async fn create(db: &SqlitePool, team_id: Uuid, email: String, invited_by: Uuid) -> Result<TeamInvitation, ApiError> {
        let id = Uuid::new_v4();
        let token = Uuid::new_v4().to_string();
        let now = Utc::now();
        let expires_at = now + chrono::Duration::days(7);

        sqlx::query!(
            r#"
            INSERT INTO team_invitations (id, team_id, email, invited_by, token, expires_at, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            id,
            team_id,
            email,
            invited_by,
            token,
            expires_at,
            now
        )
        .execute(db)
        .await?;

        Ok(TeamInvitation {
            id,
            team_id,
            email,
            invited_by,
            token,
            expires_at,
            accepted_at: None,
            created_at: now,
        })
    }

    pub async fn find_by_token(db: &SqlitePool, token: &str) -> Result<Option<TeamInvitation>, ApiError> {
        let invitation = sqlx::query_as!(
            TeamInvitation,
            r#"
            SELECT id as "id: Uuid", team_id as "team_id: Uuid", email,
                   invited_by as "invited_by: Uuid", token,
                   expires_at as "expires_at: DateTime<Utc>",
                   accepted_at as "accepted_at: DateTime<Utc>",
                   created_at as "created_at: DateTime<Utc>"
            FROM team_invitations
            WHERE token = ?1 AND expires_at > datetime('now') AND accepted_at IS NULL
            "#,
            token
        )
        .fetch_optional(db)
        .await?;

        Ok(invitation)
    }

    pub async fn accept(db: &SqlitePool, token: &str) -> Result<(), ApiError> {
        let now = Utc::now();
        
        sqlx::query!(
            r#"
            UPDATE team_invitations
            SET accepted_at = ?2
            WHERE token = ?1
            "#,
            token,
            now
        )
        .execute(db)
        .await?;

        Ok(())
    }
}