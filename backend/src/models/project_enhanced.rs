use crate::models::project_permission::{ProjectPermission, ProjectVisibility};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProjectEnhanced {
    pub id: Uuid,
    pub name: String,
    pub git_repo_path: String,
    pub setup_script: Option<String>,
    pub dev_script: Option<String>,
    pub owner_id: Option<Uuid>,
    pub team_id: Option<Uuid>,
    pub visibility: String,
    #[ts(type = "Date")]
    pub created_at: DateTime<Utc>,
    #[ts(type = "Date")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProjectWithPermissions {
    #[serde(flatten)]
    pub project: ProjectEnhanced,
    pub user_permission: Option<ProjectPermission>,
    pub owner_username: Option<String>,
    pub team_name: Option<String>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct CreateProjectEnhanced {
    pub name: String,
    pub git_repo_path: String,
    pub use_existing_repo: bool,
    pub setup_script: Option<String>,
    pub dev_script: Option<String>,
    pub team_id: Option<Uuid>,
    pub visibility: ProjectVisibility,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct UpdateProjectEnhanced {
    pub name: Option<String>,
    pub git_repo_path: Option<String>,
    pub setup_script: Option<String>,
    pub dev_script: Option<String>,
    pub team_id: Option<Uuid>,
    pub visibility: Option<ProjectVisibility>,
}

impl ProjectEnhanced {
    pub async fn find_all_for_user(
        pool: &SqlitePool,
        user_id: Uuid,
    ) -> Result<Vec<ProjectWithPermissions>, sqlx::Error> {
        let projects = sqlx::query!(
            r#"
            SELECT DISTINCT
                p.id as "id: Uuid",
                p.name,
                p.git_repo_path,
                p.setup_script,
                p.dev_script,
                p.owner_id as "owner_id: Uuid",
                p.team_id as "team_id: Uuid",
                p.visibility,
                p.created_at as "created_at: DateTime<Utc>",
                p.updated_at as "updated_at: DateTime<Utc>",
                u.username as owner_username,
                t.name as team_name
            FROM projects p
            LEFT JOIN users u ON p.owner_id = u.id
            LEFT JOIN teams t ON p.team_id = t.id
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
            ORDER BY p.updated_at DESC
            "#,
            user_id
        )
        .fetch_all(pool)
        .await?;

        let mut result = Vec::new();
        for row in projects {
            let visibility = ProjectVisibility::from_str(&row.visibility).unwrap_or(ProjectVisibility::Private);
            
            // Determine user permission
            let user_permission = if row.owner_id == Some(user_id) {
                Some(ProjectPermission::Admin)
            } else if row.visibility == "public" {
                Some(ProjectPermission::View)
            } else {
                // Check team permissions or explicit permissions
                crate::models::project_permission::ProjectPermissionRecord::get_user_permission(
                    pool,
                    row.id,
                    user_id,
                )
                .await
                .ok()
                .flatten()
            };

            result.push(ProjectWithPermissions {
                project: ProjectEnhanced {
                    id: row.id,
                    name: row.name,
                    git_repo_path: row.git_repo_path,
                    setup_script: row.setup_script,
                    dev_script: row.dev_script,
                    owner_id: row.owner_id,
                    team_id: row.team_id,
                    visibility: row.visibility,
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                },
                user_permission,
                owner_username: row.owner_username,
                team_name: row.team_name,
            });
        }

        Ok(result)
    }

    pub async fn find_by_id_with_permission(
        pool: &SqlitePool,
        id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<ProjectWithPermissions>, sqlx::Error> {
        let row = sqlx::query!(
            r#"
            SELECT
                p.id as "id: Uuid",
                p.name,
                p.git_repo_path,
                p.setup_script,
                p.dev_script,
                p.owner_id as "owner_id: Uuid",
                p.team_id as "team_id: Uuid",
                p.visibility,
                p.created_at as "created_at: DateTime<Utc>",
                p.updated_at as "updated_at: DateTime<Utc>",
                u.username as owner_username,
                t.name as team_name
            FROM projects p
            LEFT JOIN users u ON p.owner_id = u.id
            LEFT JOIN teams t ON p.team_id = t.id
            WHERE p.id = ?1
            "#,
            id
        )
        .fetch_optional(pool)
        .await?;

        match row {
            Some(row) => {
                let user_permission = crate::models::project_permission::ProjectPermissionRecord::get_user_permission(
                    pool,
                    id,
                    user_id,
                )
                .await
                .ok()
                .flatten();

                // Check if user has access
                if user_permission.is_none() && row.visibility != "public" {
                    return Ok(None);
                }

                Ok(Some(ProjectWithPermissions {
                    project: ProjectEnhanced {
                        id: row.id,
                        name: row.name,
                        git_repo_path: row.git_repo_path,
                        setup_script: row.setup_script,
                        dev_script: row.dev_script,
                        owner_id: row.owner_id,
                        team_id: row.team_id,
                        visibility: row.visibility,
                        created_at: row.created_at,
                        updated_at: row.updated_at,
                    },
                    user_permission,
                    owner_username: row.owner_username,
                    team_name: row.team_name,
                }))
            }
            None => Ok(None),
        }
    }

    pub async fn create(
        pool: &SqlitePool,
        data: &CreateProjectEnhanced,
        project_id: Uuid,
        owner_id: Uuid,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as!(
            ProjectEnhanced,
            r#"INSERT INTO projects (id, name, git_repo_path, setup_script, dev_script, owner_id, team_id, visibility) 
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8) 
            RETURNING id as "id!: Uuid", name, git_repo_path, setup_script, dev_script, 
                      owner_id as "owner_id: Uuid", team_id as "team_id: Uuid", visibility,
                      created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>""#,
            project_id,
            data.name,
            data.git_repo_path,
            data.setup_script,
            data.dev_script,
            owner_id,
            data.team_id,
            data.visibility.as_str()
        )
        .fetch_one(pool)
        .await
    }

    pub async fn update_enhanced(
        pool: &SqlitePool,
        id: Uuid,
        data: &UpdateProjectEnhanced,
        user_id: Uuid,
    ) -> Result<Self, sqlx::Error> {
        // First check if user has permission to update
        let permission = crate::models::project_permission::ProjectPermissionRecord::get_user_permission(
            pool,
            id,
            user_id,
        )
        .await
        .map_err(|_| sqlx::Error::RowNotFound)?;

        match permission {
            Some(perm) if perm.can_manage() => {
                // User has admin permission, can update everything
                let current = sqlx::query_as!(
                    ProjectEnhanced,
                    r#"SELECT id as "id!: Uuid", name, git_repo_path, setup_script, dev_script,
                              owner_id as "owner_id: Uuid", team_id as "team_id: Uuid", visibility,
                              created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>"
                       FROM projects WHERE id = $1"#,
                    id
                )
                .fetch_one(pool)
                .await?;

                let name = data.name.as_ref().unwrap_or(&current.name);
                let git_repo_path = data.git_repo_path.as_ref().unwrap_or(&current.git_repo_path);
                let setup_script = data.setup_script.clone().or(current.setup_script);
                let dev_script = data.dev_script.clone().or(current.dev_script);
                let team_id = data.team_id.or(current.team_id);
                let visibility = data.visibility.as_ref().map(|v| v.as_str()).unwrap_or(&current.visibility);

                sqlx::query_as!(
                    ProjectEnhanced,
                    r#"UPDATE projects 
                       SET name = $2, git_repo_path = $3, setup_script = $4, dev_script = $5, 
                           team_id = $6, visibility = $7, updated_at = datetime('now')
                       WHERE id = $1 
                       RETURNING id as "id!: Uuid", name, git_repo_path, setup_script, dev_script,
                                 owner_id as "owner_id: Uuid", team_id as "team_id: Uuid", visibility,
                                 created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>""#,
                    id,
                    name,
                    git_repo_path,
                    setup_script,
                    dev_script,
                    team_id,
                    visibility
                )
                .fetch_one(pool)
                .await
            }
            Some(perm) if perm.can_edit() => {
                // User has edit permission, can only update non-ownership fields
                let current = sqlx::query_as!(
                    ProjectEnhanced,
                    r#"SELECT id as "id!: Uuid", name, git_repo_path, setup_script, dev_script,
                              owner_id as "owner_id: Uuid", team_id as "team_id: Uuid", visibility,
                              created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>"
                       FROM projects WHERE id = $1"#,
                    id
                )
                .fetch_one(pool)
                .await?;

                let name = data.name.as_ref().unwrap_or(&current.name);
                let git_repo_path = data.git_repo_path.as_ref().unwrap_or(&current.git_repo_path);
                let setup_script = data.setup_script.clone().or(current.setup_script);
                let dev_script = data.dev_script.clone().or(current.dev_script);

                sqlx::query_as!(
                    ProjectEnhanced,
                    r#"UPDATE projects 
                       SET name = $2, git_repo_path = $3, setup_script = $4, dev_script = $5, updated_at = datetime('now')
                       WHERE id = $1 
                       RETURNING id as "id!: Uuid", name, git_repo_path, setup_script, dev_script,
                                 owner_id as "owner_id: Uuid", team_id as "team_id: Uuid", visibility,
                                 created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>""#,
                    id,
                    name,
                    git_repo_path,
                    setup_script,
                    dev_script
                )
                .fetch_one(pool)
                .await
            }
            _ => Err(sqlx::Error::RowNotFound), // No permission
        }
    }
}