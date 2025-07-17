use crate::error::ApiError;
use crate::middleware::auth::extract_user_id;
use crate::models::api_response::ApiResponse;
use crate::models::team::{CreateTeam, Team, TeamInvitation, TeamRole, UpdateTeam};
use crate::models::user::User;
use actix_web::{web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct AddMemberRequest {
    pub email: String,
    pub role: TeamRole,
}

#[derive(Debug, Deserialize)]
pub struct UpdateMemberRoleRequest {
    pub role: TeamRole,
}

#[derive(Debug, Deserialize)]
pub struct AcceptInvitationRequest {
    pub token: String,
}

pub async fn create_team(
    pool: web::Data<SqlitePool>,
    payload: web::Json<CreateTeam>,
    req: HttpRequest,
) -> Result<HttpResponse, ApiError> {
    let user_id = extract_user_id(&req)?;
    let team = Team::create(&pool, payload.into_inner(), user_id).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::success(team)))
}

pub async fn get_teams(
    pool: web::Data<SqlitePool>,
    req: HttpRequest,
) -> Result<HttpResponse, ApiError> {
    let user_id = extract_user_id(&req)?;
    let teams = Team::find_by_user(&pool, user_id).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::success(teams)))
}

pub async fn get_team(
    pool: web::Data<SqlitePool>,
    team_id: web::Path<Uuid>,
    req: HttpRequest,
) -> Result<HttpResponse, ApiError> {
    let user_id = extract_user_id(&req)?;
    let team_id = team_id.into_inner();

    // Check if user is a member
    let member_role = Team::get_member_role(&pool, team_id, user_id).await?;
    if member_role.is_none() {
        return Err(ApiError::Forbidden("Not a team member".into()));
    }

    let team = Team::find_by_id(&pool, team_id)
        .await?
        .ok_or(ApiError::NotFound("Team not found".into()))?;

    Ok(HttpResponse::Ok().json(ApiResponse::success(team)))
}

pub async fn update_team(
    pool: web::Data<SqlitePool>,
    team_id: web::Path<Uuid>,
    payload: web::Json<UpdateTeam>,
    req: HttpRequest,
) -> Result<HttpResponse, ApiError> {
    let user_id = extract_user_id(&req)?;
    let team_id = team_id.into_inner();

    // Check if user has permission to update
    let member_role = Team::get_member_role(&pool, team_id, user_id).await?;
    match member_role {
        Some(role) if role.can_manage_members() => {},
        _ => return Err(ApiError::Forbidden("Insufficient permissions".into())),
    }

    let team = Team::update(&pool, team_id, payload.into_inner()).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::success(team)))
}

pub async fn delete_team(
    pool: web::Data<SqlitePool>,
    team_id: web::Path<Uuid>,
    req: HttpRequest,
) -> Result<HttpResponse, ApiError> {
    let user_id = extract_user_id(&req)?;
    let team_id = team_id.into_inner();

    // Check if user has permission to delete
    let member_role = Team::get_member_role(&pool, team_id, user_id).await?;
    match member_role {
        Some(role) if role.can_delete() => {},
        _ => return Err(ApiError::Forbidden("Only team owner can delete team".into())),
    }

    Team::delete(&pool, team_id).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::success("Team deleted")))
}

pub async fn get_team_members(
    pool: web::Data<SqlitePool>,
    team_id: web::Path<Uuid>,
    req: HttpRequest,
) -> Result<HttpResponse, ApiError> {
    let user_id = extract_user_id(&req)?;
    let team_id = team_id.into_inner();

    // Check if user is a member
    let member_role = Team::get_member_role(&pool, team_id, user_id).await?;
    if member_role.is_none() {
        return Err(ApiError::Forbidden("Not a team member".into()));
    }

    let members = Team::get_members(&pool, team_id).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::success(members)))
}

pub async fn add_team_member(
    pool: web::Data<SqlitePool>,
    team_id: web::Path<Uuid>,
    payload: web::Json<AddMemberRequest>,
    req: HttpRequest,
) -> Result<HttpResponse, ApiError> {
    let user_id = extract_user_id(&req)?;
    let team_id = team_id.into_inner();

    // Check if user has permission to add members
    let member_role = Team::get_member_role(&pool, team_id, user_id).await?;
    match member_role {
        Some(role) if role.can_manage_members() => {},
        _ => return Err(ApiError::Forbidden("Insufficient permissions".into())),
    }

    // Check if user exists
    let new_member = User::find_by_email(&pool, &payload.email).await?;
    
    if let Some(new_member) = new_member {
        // Add user directly
        Team::add_member(&pool, team_id, new_member.id, payload.role.clone()).await?;
        Ok(HttpResponse::Ok().json(ApiResponse::success("Member added")))
    } else {
        // Create invitation
        let invitation = TeamInvitation::create(&pool, team_id, payload.email.clone(), user_id).await?;
        Ok(HttpResponse::Ok().json(ApiResponse::success(invitation)))
    }
}

pub async fn update_team_member_role(
    pool: web::Data<SqlitePool>,
    path: web::Path<(Uuid, Uuid)>,
    payload: web::Json<UpdateMemberRoleRequest>,
    req: HttpRequest,
) -> Result<HttpResponse, ApiError> {
    let user_id = extract_user_id(&req)?;
    let (team_id, member_id) = path.into_inner();

    // Check if user has permission to update roles
    let member_role = Team::get_member_role(&pool, team_id, user_id).await?;
    match member_role {
        Some(role) if role.can_manage_members() => {},
        _ => return Err(ApiError::Forbidden("Insufficient permissions".into())),
    }

    // Prevent demoting the owner
    let target_role = Team::get_member_role(&pool, team_id, member_id).await?;
    if matches!(target_role, Some(TeamRole::Owner)) && !matches!(payload.role, TeamRole::Owner) {
        return Err(ApiError::BadRequest("Cannot change owner role".into()));
    }

    Team::update_member_role(&pool, team_id, member_id, payload.role.clone()).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::success("Role updated")))
}

pub async fn remove_team_member(
    pool: web::Data<SqlitePool>,
    path: web::Path<(Uuid, Uuid)>,
    req: HttpRequest,
) -> Result<HttpResponse, ApiError> {
    let user_id = extract_user_id(&req)?;
    let (team_id, member_id) = path.into_inner();

    // Check if user has permission to remove members
    let member_role = Team::get_member_role(&pool, team_id, user_id).await?;
    match member_role {
        Some(role) if role.can_manage_members() => {},
        _ => return Err(ApiError::Forbidden("Insufficient permissions".into())),
    }

    // Prevent removing the owner
    let target_role = Team::get_member_role(&pool, team_id, member_id).await?;
    if matches!(target_role, Some(TeamRole::Owner)) {
        return Err(ApiError::BadRequest("Cannot remove team owner".into()));
    }

    Team::remove_member(&pool, team_id, member_id).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::success("Member removed")))
}

pub async fn accept_team_invitation(
    pool: web::Data<SqlitePool>,
    payload: web::Json<AcceptInvitationRequest>,
    req: HttpRequest,
) -> Result<HttpResponse, ApiError> {
    let user_id = extract_user_id(&req)?;
    
    // Find invitation
    let invitation = TeamInvitation::find_by_token(&pool, &payload.token)
        .await?
        .ok_or(ApiError::NotFound("Invalid or expired invitation".into()))?;

    // Verify email matches
    let user = User::find_by_id(&pool, user_id)
        .await?
        .ok_or(ApiError::NotFound("User not found".into()))?;
    
    if user.email != invitation.email {
        return Err(ApiError::BadRequest("Invitation email does not match".into()));
    }

    // Accept invitation
    TeamInvitation::accept(&pool, &payload.token).await?;
    
    // Add user to team
    Team::add_member(&pool, invitation.team_id, user_id, TeamRole::Member).await?;

    Ok(HttpResponse::Ok().json(ApiResponse::success("Invitation accepted")))
}