use crate::error::ApiError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum ProjectPermission {
    #[serde(rename = "view")]
    View,
    #[serde(rename = "edit")]
    Edit,
    #[serde(rename = "admin")]
    Admin,
}

impl ProjectPermission {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProjectPermission::View => "view",
            ProjectPermission::Edit => "edit",
            ProjectPermission::Admin => "admin",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "view" => Some(ProjectPermission::View),
            "edit" => Some(ProjectPermission::Edit),
            "admin" => Some(ProjectPermission::Admin),
            _ => None,
        }
    }

    pub fn can_view(&self) -> bool {
        true // All permission levels can view
    }

    pub fn can_edit(&self) -> bool {
        matches!(self, ProjectPermission::Edit | ProjectPermission::Admin)
    }

    pub fn can_manage(&self) -> bool {
        matches!(self, ProjectPermission::Admin)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum ProjectVisibility {
    #[serde(rename = "private")]
    Private,
    #[serde(rename = "team")]
    Team,
    #[serde(rename = "public")]
    Public,
}

impl ProjectVisibility {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProjectVisibility::Private => "private",
            ProjectVisibility::Team => "team",
            ProjectVisibility::Public => "public",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "private" => Some(ProjectVisibility::Private),
            "team" => Some(ProjectVisibility::Team),
            "public" => Some(ProjectVisibility::Public),
            _ => None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct ProjectPermissionRecord {
    pub id: Uuid,
    pub project_id: Uuid,
    pub user_id: Uuid,
    pub permission: String,
    pub granted_by: Uuid,
    pub granted_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectPermissionWithUser {
    pub id: Uuid,
    pub project_id: Uuid,
    pub user_id: Uuid,
    pub permission: ProjectPermission,
    pub granted_by: Uuid,
    pub granted_at: DateTime<Utc>,
    pub username: String,
    pub email: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GrantProjectPermission {
    pub user_id: Uuid,
    pub permission: ProjectPermission,
}

impl ProjectPermissionRecord {
    pub async fn grant(
        db: &SqlitePool,
        project_id: Uuid,
        user_id: Uuid,
        permission: ProjectPermission,
        granted_by: Uuid,
    ) -> Result<(), ApiError> {
        let id = Uuid::new_v4();
        let now = Utc::now();

        sqlx::query!(
            r#"
            INSERT INTO project_permissions (id, project_id, user_id, permission, granted_by, granted_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(project_id, user_id) DO UPDATE SET
                permission = excluded.permission,
                granted_by = excluded.granted_by,
                granted_at = excluded.granted_at
            "#,
            id,
            project_id,
            user_id,
            permission.as_str(),
            granted_by,
            now
        )
        .execute(db)
        .await?;

        Ok(())
    }

    pub async fn revoke(db: &SqlitePool, project_id: Uuid, user_id: Uuid) -> Result<(), ApiError> {
        sqlx::query!(
            r#"
            DELETE FROM project_permissions
            WHERE project_id = ?1 AND user_id = ?2
            "#,
            project_id,
            user_id
        )
        .execute(db)
        .await?;

        Ok(())
    }

    pub async fn get_user_permission(
        db: &SqlitePool,
        project_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<ProjectPermission>, ApiError> {
        // First check if user is the owner
        let is_owner = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) as count
            FROM projects
            WHERE id = ?1 AND owner_id = ?2
            "#,
            project_id,
            user_id
        )
        .fetch_one(db)
        .await?;

        if is_owner > 0 {
            return Ok(Some(ProjectPermission::Admin));
        }

        // Check if user has team access
        let team_access = sqlx::query_scalar!(
            r#"
            SELECT tm.role
            FROM projects p
            JOIN team_members tm ON p.team_id = tm.team_id
            WHERE p.id = ?1 AND tm.user_id = ?2 AND p.visibility = 'team'
            "#,
            project_id,
            user_id
        )
        .fetch_optional(db)
        .await?;

        if let Some(role) = team_access {
            // Map team role to project permission
            return Ok(match role.as_str() {
                "owner" | "admin" => Some(ProjectPermission::Admin),
                "member" => Some(ProjectPermission::Edit),
                "viewer" => Some(ProjectPermission::View),
                _ => None,
            });
        }

        // Check explicit permissions
        let permission = sqlx::query_scalar!(
            r#"
            SELECT permission
            FROM project_permissions
            WHERE project_id = ?1 AND user_id = ?2
            "#,
            project_id,
            user_id
        )
        .fetch_optional(db)
        .await?;

        Ok(permission.and_then(|p| ProjectPermission::from_str(&p)))
    }

    pub async fn get_project_permissions(
        db: &SqlitePool,
        project_id: Uuid,
    ) -> Result<Vec<ProjectPermissionWithUser>, ApiError> {
        let permissions = sqlx::query!(
            r#"
            SELECT pp.id as "id: Uuid", pp.project_id as "project_id: Uuid",
                   pp.user_id as "user_id: Uuid", pp.permission,
                   pp.granted_by as "granted_by: Uuid",
                   pp.granted_at as "granted_at: DateTime<Utc>",
                   u.username, u.email, u.display_name
            FROM project_permissions pp
            JOIN users u ON pp.user_id = u.id
            WHERE pp.project_id = ?1
            ORDER BY pp.granted_at DESC
            "#,
            project_id
        )
        .fetch_all(db)
        .await?
        .into_iter()
        .filter_map(|r| {
            ProjectPermission::from_str(&r.permission).map(|permission| ProjectPermissionWithUser {
                id: r.id,
                project_id: r.project_id,
                user_id: r.user_id,
                permission,
                granted_by: r.granted_by,
                granted_at: r.granted_at,
                username: r.username,
                email: r.email,
                display_name: r.display_name,
            })
        })
        .collect();

        Ok(permissions)
    }

    pub async fn get_user_accessible_projects(
        db: &SqlitePool,
        user_id: Uuid,
    ) -> Result<Vec<Uuid>, ApiError> {
        let project_ids = sqlx::query_scalar!(
            r#"
            SELECT DISTINCT p.id as "id: Uuid"
            FROM projects p
            WHERE p.owner_id = ?1
               OR p.visibility = 'public'
               OR (p.visibility = 'team' AND EXISTS (
                   SELECT 1 FROM team_members tm
                   WHERE tm.team_id = p.team_id AND tm.user_id = ?1
               ))
               OR EXISTS (
                   SELECT 1 FROM project_permissions pp
                   WHERE pp.project_id = p.id AND pp.user_id = ?1
               )
            "#,
            user_id
        )
        .fetch_all(db)
        .await?;

        Ok(project_ids)
    }
}