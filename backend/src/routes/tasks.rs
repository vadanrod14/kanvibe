use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json as ResponseJson,
    routing::get,
    Json, Router,
};
use uuid::Uuid;

use crate::{
    app_state::AppState,
    models::{
        config::Config,
        project::Project,
        task::{CreateTask, CreateTaskAndStart, Task, TaskWithAttemptStatus, UpdateTask},
        task_attempt::{CreateTaskAttempt, TaskAttempt},
        ApiResponse,
    },
    services::AgentMarketClient,
};

pub async fn get_project_tasks(
    Path(project_id): Path<Uuid>,
    State(app_state): State<AppState>,
) -> Result<ResponseJson<ApiResponse<Vec<TaskWithAttemptStatus>>>, StatusCode> {
    match Task::find_by_project_id_with_attempt_status(&app_state.db_pool, project_id).await {
        Ok(tasks) => Ok(ResponseJson(ApiResponse {
            success: true,
            data: Some(tasks),
            message: None,
        })),
        Err(e) => {
            tracing::error!("Failed to fetch tasks for project {}: {}", project_id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_task(
    Path((project_id, task_id)): Path<(Uuid, Uuid)>,
    State(app_state): State<AppState>,
) -> Result<ResponseJson<ApiResponse<Task>>, StatusCode> {
    match Task::find_by_id_and_project_id(&app_state.db_pool, task_id, project_id).await {
        Ok(Some(task)) => Ok(ResponseJson(ApiResponse {
            success: true,
            data: Some(task),
            message: None,
        })),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!(
                "Failed to fetch task {} in project {}: {}",
                task_id,
                project_id,
                e
            );
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn create_task(
    Path(project_id): Path<Uuid>,
    State(app_state): State<AppState>,
    Json(mut payload): Json<CreateTask>,
) -> Result<ResponseJson<ApiResponse<Task>>, StatusCode> {
    let id = Uuid::new_v4();

    // Ensure the project_id in the payload matches the path parameter
    payload.project_id = project_id;

    // Verify project exists first
    match Project::exists(&app_state.db_pool, project_id).await {
        Ok(false) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to check project existence: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        Ok(true) => {}
    }

    tracing::debug!(
        "Creating task '{}' in project {}",
        payload.title,
        project_id
    );

    match Task::create(&app_state.db_pool, &payload, id).await {
        Ok(task) => {
            // Track task creation event
            app_state
                .track_analytics_event(
                    "task_created",
                    Some(serde_json::json!({
                        "task_id": task.id.to_string(),
                        "project_id": project_id.to_string(),
                        "has_description": task.description.is_some(),
                    })),
                )
                .await;

            Ok(ResponseJson(ApiResponse {
                success: true,
                data: Some(task),
                message: Some("Task created successfully".to_string()),
            }))
        }
        Err(e) => {
            tracing::error!("Failed to create task: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn create_task_and_start(
    Path(project_id): Path<Uuid>,
    State(app_state): State<AppState>,
    Json(mut payload): Json<CreateTaskAndStart>,
) -> Result<ResponseJson<ApiResponse<Task>>, StatusCode> {
    let task_id = Uuid::new_v4();

    // Ensure the project_id in the payload matches the path parameter
    payload.project_id = project_id;

    // Verify project exists first
    match Project::exists(&app_state.db_pool, project_id).await {
        Ok(false) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to check project existence: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        Ok(true) => {}
    }

    tracing::debug!(
        "Creating and starting task '{}' in project {}",
        payload.title,
        project_id
    );

    // Check Agent Market API key before creating task and attempt
    let config = match Config::load(&crate::utils::config_path()) {
        Ok(config) => config,
        Err(e) => {
            tracing::error!("Failed to load config: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    if config.agent_market_api_key.is_none() {
        return Ok(ResponseJson(ApiResponse {
            success: false,
            data: None,
            message: Some("Agent Market API key is required to run tasks. Please configure your API key in the application settings.".to_string()),
        }));
    }

    // Create the task first
    let create_task_payload = CreateTask {
        project_id: payload.project_id,
        title: payload.title.clone(),
        description: payload.description.clone(),
        max_reward: payload.max_reward,
    };
    let task = match Task::create(&app_state.db_pool, &create_task_payload, task_id).await {
        Ok(task) => task,
        Err(e) => {
            tracing::error!("Failed to create task: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Create task attempt - using default executor since Agent Market will handle execution
    let attempt_payload = CreateTaskAttempt {
        executor: Some("agent_market".to_string()),
        base_branch: None, // Not supported in task creation endpoint, only in task attempts
    };

    match TaskAttempt::create(&app_state.db_pool, &attempt_payload, task_id).await {
        Ok(attempt) => {
            app_state
                .track_analytics_event(
                    "task_created",
                    Some(serde_json::json!({
                        "task_id": task.id.to_string(),
                        "project_id": project_id.to_string(),
                        "has_description": task.description.is_some(),
                    })),
                )
                .await;

            app_state
                .track_analytics_event(
                    "task_attempt_started",
                    Some(serde_json::json!({
                        "task_id": task.id.to_string(),
                        "executor_type": "agent_market",
                        "attempt_id": attempt.id.to_string(),
                    })),
                )
                .await;

            // Validate Agent Market API key and create instance synchronously
            let config = app_state.get_config().read().await;
            if let Some(api_key) = config.agent_market_api_key.clone() {
                drop(config); // Release the lock early
                
                // Get the project for Agent Market instance creation
                match Project::find_by_id(&app_state.db_pool, project_id).await {
                    Ok(Some(project)) => {
                        let agent_market_client = AgentMarketClient::new();
                        match agent_market_client
                            .create_instance(&api_key, &task, &project, payload.max_reward)
                            .await
                        {
                            Ok(instance_response) => {
                                tracing::info!(
                                    "Successfully created Agent Market instance for task {}: {:?}",
                                    task_id,
                                    instance_response
                                );
                            }
                            Err(e) => {
                                tracing::error!(
                                    "Failed to create Agent Market instance for task {}: {}",
                                    task_id,
                                    e
                                );
                                
                                // Return error to frontend instead of continuing
                                return Ok(ResponseJson(ApiResponse {
                                    success: false,
                                    data: None,
                                    message: Some(format!("Failed to start Agent Market instance: {}", e)),
                                }));
                            }
                        }
                    }
                    Ok(None) => {
                        tracing::error!(
                            "Project {} not found for Agent Market instance",
                            project_id
                        );
                        return Ok(ResponseJson(ApiResponse {
                            success: false,
                            data: None,
                            message: Some("Project not found".to_string()),
                        }));
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to fetch project {} for Agent Market instance: {}",
                            project_id,
                            e
                        );
                        return Ok(ResponseJson(ApiResponse {
                            success: false,
                            data: None,
                            message: Some("Failed to fetch project information".to_string()),
                        }));
                    }
                }
            } else {
                tracing::error!("Agent Market API key not configured");
                return Ok(ResponseJson(ApiResponse {
                    success: false,
                    data: None,
                    message: Some("Agent Market API key not configured".to_string()),
                }));
            }

            Ok(ResponseJson(ApiResponse {
                success: true,
                data: Some(task),
                message: Some("Task created and started successfully".to_string()),
            }))
        }
        Err(e) => {
            tracing::error!("Failed to create task attempt: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn update_task(
    Path((project_id, task_id)): Path<(Uuid, Uuid)>,
    State(app_state): State<AppState>,
    Json(payload): Json<UpdateTask>,
) -> Result<ResponseJson<ApiResponse<Task>>, StatusCode> {
    // Check if task exists in the specified project
    let existing_task =
        match Task::find_by_id_and_project_id(&app_state.db_pool, task_id, project_id).await {
            Ok(Some(task)) => task,
            Ok(None) => return Err(StatusCode::NOT_FOUND),
            Err(e) => {
                tracing::error!("Failed to check task existence: {}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        };

    // Use existing values if not provided in update
    let title = payload.title.unwrap_or(existing_task.title);
    let description = payload.description.or(existing_task.description);
    let status = payload.status.unwrap_or(existing_task.status);

    match Task::update(
        &app_state.db_pool,
        task_id,
        project_id,
        title,
        description,
        status,
    )
    .await
    {
        Ok(task) => Ok(ResponseJson(ApiResponse {
            success: true,
            data: Some(task),
            message: Some("Task updated successfully".to_string()),
        })),
        Err(e) => {
            tracing::error!("Failed to update task: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn delete_task(
    Path((project_id, task_id)): Path<(Uuid, Uuid)>,
    State(app_state): State<AppState>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    // Verify task exists in the specified project
    match Task::find_by_id_and_project_id(&app_state.db_pool, task_id, project_id).await {
        Ok(Some(_)) => {} // Task exists, proceed
        Ok(None) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to check task existence: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    // Clean up all executor sessions for this task before deletion
    match TaskAttempt::find_by_task_id(&app_state.db_pool, task_id).await {
        Ok(task_attempts) => {
            for attempt in task_attempts {
                if let Err(e) =
                    crate::models::executor_session::ExecutorSession::delete_by_task_attempt_id(
                        &app_state.db_pool,
                        attempt.id,
                    )
                    .await
                {
                    tracing::error!(
                        "Failed to cleanup executor sessions for task attempt {}: {}",
                        attempt.id,
                        e
                    );
                    // Continue with deletion even if session cleanup fails
                } else {
                    tracing::debug!(
                        "Cleaned up executor sessions for task attempt {}",
                        attempt.id
                    );
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to get task attempts for session cleanup: {}", e);
            // Continue with deletion even if we can't get task attempts
        }
    }

    match Task::delete(&app_state.db_pool, task_id, project_id).await {
        Ok(rows_affected) => {
            if rows_affected == 0 {
                Err(StatusCode::NOT_FOUND)
            } else {
                Ok(ResponseJson(ApiResponse {
                    success: true,
                    data: None,
                    message: Some("Task deleted successfully".to_string()),
                }))
            }
        }
        Err(e) => {
            tracing::error!("Failed to delete task: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub fn tasks_router() -> Router<AppState> {
    use axum::routing::post;

    Router::new()
        .route(
            "/projects/:project_id/tasks",
            get(get_project_tasks).post(create_task),
        )
        .route(
            "/projects/:project_id/tasks/create-and-start",
            post(create_task_and_start),
        )
        .route(
            "/projects/:project_id/tasks/:task_id",
            get(get_task).put(update_task).delete(delete_task),
        )
}
