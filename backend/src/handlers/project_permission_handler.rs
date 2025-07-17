use crate::error::ApiError;
use crate::middleware::auth::extract_user_id;
use crate::models::api_response::ApiResponse;
use crate::models::project_enhanced::{ProjectEnhanced, CreateProjectEnhanced, UpdateProjectEnhanced};
use crate::models::project_permission::{ProjectPermissionRecord, GrantProjectPermission};
use actix_web::{web, HttpRequest, HttpResponse};
use sqlx::SqlitePool;
use uuid::Uuid;

pub async fn get_user_projects(
    pool: web::Data<SqlitePool>,
    req: HttpRequest,
) -> Result<HttpResponse, ApiError> {
    let user_id = extract_user_id(&req)?;
    let projects = ProjectEnhanced::find_all_for_user(&pool, user_id).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::success(projects)))
}

pub async fn get_project_with_permissions(
    pool: web::Data<SqlitePool>,
    project_id: web::Path<Uuid>,
    req: HttpRequest,
) -> Result<HttpResponse, ApiError> {
    let user_id = extract_user_id(&req)?;
    let project_id = project_id.into_inner();
    
    let project = ProjectEnhanced::find_by_id_with_permission(&pool, project_id, user_id)
        .await?
        .ok_or(ApiError::NotFound("Project not found or access denied".into()))?;
    
    Ok(HttpResponse::Ok().json(ApiResponse::success(project)))
}

pub async fn create_project_enhanced(
    pool: web::Data<SqlitePool>,
    payload: web::Json<CreateProjectEnhanced>,
    req: HttpRequest,
) -> Result<HttpResponse, ApiError> {
    let user_id = extract_user_id(&req)?;
    let project_id = Uuid::new_v4();
    
    // If team_id is provided, verify user is a member
    if let Some(team_id) = payload.team_id {
        let role = crate::models::team::Team::get_member_role(&pool, team_id, user_id).await?;
        if role.is_none() {
            return Err(ApiError::Forbidden("Not a member of the specified team".into()));
        }
    }
    
    let project = ProjectEnhanced::create(&pool, &payload, project_id, user_id).await?;
    
    // Return with permissions
    let project_with_perms = ProjectEnhanced::find_by_id_with_permission(&pool, project.id, user_id)
        .await?
        .ok_or(ApiError::InternalServerError("Failed to fetch created project".into()))?;
    
    Ok(HttpResponse::Ok().json(ApiResponse::success(project_with_perms)))
}

pub async fn update_project_enhanced(
    pool: web::Data<SqlitePool>,
    project_id: web::Path<Uuid>,
    payload: web::Json<UpdateProjectEnhanced>,
    req: HttpRequest,
) -> Result<HttpResponse, ApiError> {
    let user_id = extract_user_id(&req)?;
    let project_id = project_id.into_inner();
    
    let project = ProjectEnhanced::update_enhanced(&pool, project_id, &payload, user_id).await?;
    
    // Return with permissions
    let project_with_perms = ProjectEnhanced::find_by_id_with_permission(&pool, project.id, user_id)
        .await?
        .ok_or(ApiError::InternalServerError("Failed to fetch updated project".into()))?;
    
    Ok(HttpResponse::Ok().json(ApiResponse::success(project_with_perms)))
}

pub async fn get_project_permissions(
    pool: web::Data<SqlitePool>,
    project_id: web::Path<Uuid>,
    req: HttpRequest,
) -> Result<HttpResponse, ApiError> {
    let user_id = extract_user_id(&req)?;
    let project_id = project_id.into_inner();
    
    // Check if user has permission to view permissions
    let user_permission = ProjectPermissionRecord::get_user_permission(&pool, project_id, user_id).await?;
    match user_permission {
        Some(perm) if perm.can_manage() => {},
        _ => return Err(ApiError::Forbidden("Insufficient permissions".into())),
    }
    
    let permissions = ProjectPermissionRecord::get_project_permissions(&pool, project_id).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::success(permissions)))
}

pub async fn grant_project_permission(
    pool: web::Data<SqlitePool>,
    project_id: web::Path<Uuid>,
    payload: web::Json<GrantProjectPermission>,
    req: HttpRequest,
) -> Result<HttpResponse, ApiError> {
    let granting_user_id = extract_user_id(&req)?;
    let project_id = project_id.into_inner();
    
    // Check if granting user has permission to grant permissions
    let user_permission = ProjectPermissionRecord::get_user_permission(&pool, project_id, granting_user_id).await?;
    match user_permission {
        Some(perm) if perm.can_manage() => {},
        _ => return Err(ApiError::Forbidden("Insufficient permissions to grant access".into())),
    }
    
    // Grant permission
    ProjectPermissionRecord::grant(
        &pool,
        project_id,
        payload.user_id,
        payload.permission.clone(),
        granting_user_id,
    )
    .await?;
    
    Ok(HttpResponse::Ok().json(ApiResponse::success("Permission granted")))
}

pub async fn revoke_project_permission(
    pool: web::Data<SqlitePool>,
    path: web::Path<(Uuid, Uuid)>,
    req: HttpRequest,
) -> Result<HttpResponse, ApiError> {
    let user_id = extract_user_id(&req)?;
    let (project_id, target_user_id) = path.into_inner();
    
    // Check if user has permission to revoke permissions
    let user_permission = ProjectPermissionRecord::get_user_permission(&pool, project_id, user_id).await?;
    match user_permission {
        Some(perm) if perm.can_manage() => {},
        _ => return Err(ApiError::Forbidden("Insufficient permissions to revoke access".into())),
    }
    
    // Don't allow revoking owner's access
    let project = sqlx::query!(
        "SELECT owner_id FROM projects WHERE id = ?",
        project_id
    )
    .fetch_one(pool.as_ref())
    .await?;
    
    if let Some(owner_id) = project.owner_id {
        let owner_uuid: Uuid = owner_id;
        if owner_uuid == target_user_id {
            return Err(ApiError::BadRequest("Cannot revoke owner's access".into()));
        }
    }
    
    ProjectPermissionRecord::revoke(&pool, project_id, target_user_id).await?;
    
    Ok(HttpResponse::Ok().json(ApiResponse::success("Permission revoked")))
}