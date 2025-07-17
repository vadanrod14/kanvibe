use crate::app_state::AppState;
use crate::handlers::team_handler::*;
use actix_web_httpauth::middleware::HttpAuthentication;
use axum::{
    routing::{delete, get, post, put},
    Router,
};

pub fn teams_router() -> Router<AppState> {
    Router::new()
        .route("/teams", post(create_team).get(get_teams))
        .route("/teams/:team_id", get(get_team).put(update_team).delete(delete_team))
        .route("/teams/:team_id/members", get(get_team_members).post(add_team_member))
        .route("/teams/:team_id/members/:member_id", put(update_team_member_role).delete(remove_team_member))
        .route("/teams/invitations/accept", post(accept_team_invitation))
}