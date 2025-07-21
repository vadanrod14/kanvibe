use axum::{
    extract::{Request, State},
    middleware::Next,
    response::{Json as ResponseJson, Response},
    routing::{get, post},
    Json, Router,
};

use crate::{app_state::AppState, models::ApiResponse};

pub fn auth_router() -> Router<AppState> {
    Router::new()
        .route("/auth/github/device/start", post(device_start))
        .route("/auth/github/device/poll", post(device_poll))
        .route("/auth/github/check", get(github_check_token))
        .route("/auth/github/repos", get(github_repos))
}

#[derive(serde::Deserialize)]
struct DeviceStartRequest {}

#[derive(serde::Serialize)]
struct DeviceStartResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

#[derive(serde::Deserialize)]
struct DevicePollRequest {
    device_code: String,
}

/// POST /auth/github/device/start
async fn device_start() -> ResponseJson<ApiResponse<DeviceStartResponse>> {
    let params = [
        ("client_id", "Ov23li9bxz3kKfPOIsGm"),
        ("scope", "user:email,repo"),
    ];
    let client = reqwest::Client::new();
    let res = client
        .post("https://github.com/login/device/code")
        .header("Accept", "application/json")
        .form(&params)
        .send()
        .await;
    let res = match res {
        Ok(r) => r,
        Err(e) => {
            return ResponseJson(ApiResponse {
                success: false,
                data: None,
                message: Some(format!("Failed to contact GitHub: {e}")),
            });
        }
    };
    let json: serde_json::Value = match res.json().await {
        Ok(j) => j,
        Err(e) => {
            return ResponseJson(ApiResponse {
                success: false,
                data: None,
                message: Some(format!("Failed to parse GitHub response: {e}")),
            });
        }
    };
    if let (
        Some(device_code),
        Some(user_code),
        Some(verification_uri),
        Some(expires_in),
        Some(interval),
    ) = (
        json.get("device_code").and_then(|v| v.as_str()),
        json.get("user_code").and_then(|v| v.as_str()),
        json.get("verification_uri").and_then(|v| v.as_str()),
        json.get("expires_in").and_then(|v| v.as_u64()),
        json.get("interval").and_then(|v| v.as_u64()),
    ) {
        ResponseJson(ApiResponse {
            success: true,
            data: Some(DeviceStartResponse {
                device_code: device_code.to_string(),
                user_code: user_code.to_string(),
                verification_uri: verification_uri.to_string(),
                expires_in,
                interval,
            }),
            message: None,
        })
    } else {
        ResponseJson(ApiResponse {
            success: false,
            data: None,
            message: Some(format!("GitHub error: {}", json)),
        })
    }
}

/// POST /auth/github/device/poll
async fn device_poll(
    State(app_state): State<AppState>,
    Json(payload): Json<DevicePollRequest>,
) -> ResponseJson<ApiResponse<String>> {
    let params = [
        ("client_id", "Ov23li9bxz3kKfPOIsGm"),
        ("device_code", payload.device_code.as_str()),
        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
    ];
    let client = reqwest::Client::new();
    let res = client
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .form(&params)
        .send()
        .await;
    let res = match res {
        Ok(r) => r,
        Err(e) => {
            return ResponseJson(ApiResponse {
                success: false,
                data: None,
                message: Some(format!("Failed to contact GitHub: {e}")),
            });
        }
    };
    let json: serde_json::Value = match res.json().await {
        Ok(j) => j,
        Err(e) => {
            return ResponseJson(ApiResponse {
                success: false,
                data: None,
                message: Some(format!("Failed to parse GitHub response: {e}")),
            });
        }
    };
    if let Some(error) = json.get("error").and_then(|v| v.as_str()) {
        // Not authorized yet, or other error
        return ResponseJson(ApiResponse {
            success: false,
            data: None,
            message: Some(error.to_string()),
        });
    }
    let access_token = json.get("access_token").and_then(|v| v.as_str());
    if let Some(access_token) = access_token {
        // Fetch user info
        let user_res = client
            .get("https://api.github.com/user")
            .bearer_auth(access_token)
            .header("User-Agent", "vibe-kanban-app")
            .send()
            .await;
        let user_json: serde_json::Value = match user_res {
            Ok(res) => match res.json().await {
                Ok(json) => json,
                Err(e) => {
                    return ResponseJson(ApiResponse {
                        success: false,
                        data: None,
                        message: Some(format!("Failed to parse GitHub user response: {e}")),
                    });
                }
            },
            Err(e) => {
                return ResponseJson(ApiResponse {
                    success: false,
                    data: None,
                    message: Some(format!("Failed to fetch user info: {e}")),
                });
            }
        };
        let username = user_json
            .get("login")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        // Fetch user emails
        let emails_res = client
            .get("https://api.github.com/user/emails")
            .bearer_auth(access_token)
            .header("User-Agent", "vibe-kanban-app")
            .send()
            .await;
        let emails_json: serde_json::Value = match emails_res {
            Ok(res) => match res.json().await {
                Ok(json) => json,
                Err(e) => {
                    return ResponseJson(ApiResponse {
                        success: false,
                        data: None,
                        message: Some(format!("Failed to parse GitHub emails response: {e}")),
                    });
                }
            },
            Err(e) => {
                return ResponseJson(ApiResponse {
                    success: false,
                    data: None,
                    message: Some(format!("Failed to fetch user emails: {e}")),
                });
            }
        };
        let primary_email = emails_json
            .as_array()
            .and_then(|arr| {
                arr.iter()
                    .find(|email| {
                        email
                            .get("primary")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false)
                    })
                    .and_then(|email| email.get("email").and_then(|v| v.as_str()))
            })
            .map(|s| s.to_string());
        // Save to config
        {
            let mut config = app_state.get_config().write().await;
            config.github.username = username.clone();
            config.github.primary_email = primary_email.clone();
            config.github.token = Some(access_token.to_string());
            let config_path = crate::utils::config_path();
            if config.save(&config_path).is_err() {
                return ResponseJson(ApiResponse {
                    success: false,
                    data: None,
                    message: Some("Failed to save config".to_string()),
                });
            }
        }
        app_state.update_sentry_scope().await;
        // Identify user in PostHog
        let mut props = serde_json::Map::new();
        if let Some(ref username) = username {
            props.insert(
                "username".to_string(),
                serde_json::Value::String(username.clone()),
            );
        }
        if let Some(ref email) = primary_email {
            props.insert(
                "email".to_string(),
                serde_json::Value::String(email.clone()),
            );
        }
        {
            let props = serde_json::Value::Object(props);
            app_state
                .track_analytics_event("$identify", Some(props))
                .await;
        }

        ResponseJson(ApiResponse {
            success: true,
            data: Some("GitHub login successful".to_string()),
            message: None,
        })
    } else {
        ResponseJson(ApiResponse {
            success: false,
            data: None,
            message: Some("No access token yet".to_string()),
        })
    }
}

/// GET /auth/github/check
async fn github_check_token(State(app_state): State<AppState>) -> ResponseJson<ApiResponse<()>> {
    let config = app_state.get_config().read().await;
    let token = config.github.token.clone();
    drop(config);
    if let Some(token) = token {
        let client = reqwest::Client::new();
        let res = client
            .get("https://api.github.com/user")
            .bearer_auth(&token)
            .header("User-Agent", "vibe-kanban-app")
            .send()
            .await;
        match res {
            Ok(r) if r.status().is_success() => ResponseJson(ApiResponse {
                success: true,
                data: None,
                message: Some("GitHub token is valid".to_string()),
            }),
            _ => ResponseJson(ApiResponse {
                success: false,
                data: None,
                message: Some("github_token_invalid".to_string()),
            }),
        }
    } else {
        ResponseJson(ApiResponse {
            success: false,
            data: None,
            message: Some("github_token_invalid".to_string()),
        })
    }
}

/// GET /auth/github/repos
async fn github_repos(State(app_state): State<AppState>) -> ResponseJson<ApiResponse<serde_json::Value>> {
    let config = app_state.get_config().read().await;
    let token = config.github.token.clone();
    drop(config);
    
    if let Some(token) = token {
        let client = reqwest::Client::new();
        let res = client
            .get("https://api.github.com/user/repos?sort=updated&per_page=100")
            .bearer_auth(&token)
            .header("User-Agent", "vibe-kanban-app")
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await;
        
        match res {
            Ok(response) if response.status().is_success() => {
                match response.json::<serde_json::Value>().await {
                    Ok(repos) => {
                        // Transform to the format expected by frontend
                        let formatted_repos = repos.as_array().map(|repos_array| {
                            repos_array.iter().map(|repo| {
                                serde_json::json!({
                                    "id": repo.get("id"),
                                    "name": repo.get("name"),
                                    "full_name": repo.get("full_name"),
                                    "private": repo.get("private"),
                                    "html_url": repo.get("html_url"),
                                    "clone_url": repo.get("clone_url"),
                                    "ssh_url": repo.get("ssh_url"),
                                    "description": repo.get("description"),
                                    "language": repo.get("language"),
                                    "updated_at": repo.get("updated_at"),
                                    "owner": {
                                        "login": repo.get("owner").and_then(|o| o.get("login")),
                                        "avatar_url": repo.get("owner").and_then(|o| o.get("avatar_url"))
                                    }
                                })
                            }).collect::<Vec<_>>()
                        }).unwrap_or_default();
                        
                        ResponseJson(ApiResponse {
                            success: true,
                            data: Some(serde_json::Value::Array(formatted_repos)),
                            message: None,
                        })
                    }
                    Err(e) => ResponseJson(ApiResponse {
                        success: false,
                        data: None,
                        message: Some(format!("Failed to parse GitHub repositories response: {}", e)),
                    })
                }
            }
            Ok(response) => ResponseJson(ApiResponse {
                success: false,
                data: None,
                message: Some(format!("GitHub API error: {}", response.status())),
            }),
            Err(e) => ResponseJson(ApiResponse {
                success: false,
                data: None,
                message: Some(format!("Failed to fetch GitHub repositories: {}", e)),
            })
        }
    } else {
        ResponseJson(ApiResponse {
            success: false,
            data: None,
            message: Some("GitHub authentication required".to_string()),
        })
    }
}

/// Middleware to set Sentry user context for every request
pub async fn sentry_user_context_middleware(
    State(app_state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    app_state.update_sentry_scope().await;
    next.run(req).await
}
