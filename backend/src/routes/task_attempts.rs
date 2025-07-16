use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json as ResponseJson,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    app_state::AppState,
    executor::{ExecutorConfig, NormalizedConversation, NormalizedEntry, NormalizedEntryType},
    models::{
        config::Config,
        execution_process::{
            ExecutionProcess, ExecutionProcessStatus, ExecutionProcessSummary, ExecutionProcessType,
        },
        executor_session::ExecutorSession,
        project::Project,
        task::Task,
        task_attempt::{
            BranchStatus, CreateFollowUpAttempt, CreateTaskAttempt, TaskAttempt, TaskAttemptState,
            TaskAttemptStatus, WorktreeDiff,
        },
        task_attempt_activity::{
            CreateTaskAttemptActivity, TaskAttemptActivity, TaskAttemptActivityWithPrompt,
        },
        ApiResponse,
    },
    services::agent_market::AgentMarketClient,
};

#[derive(Debug, Deserialize, Serialize)]
pub struct RebaseTaskAttemptRequest {
    pub new_base_branch: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateGitHubPRRequest {
    pub title: String,
    pub body: Option<String>,
    pub base_branch: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FollowUpResponse {
    pub message: String,
    pub actual_attempt_id: Uuid,
    pub created_new_attempt: bool,
}

pub async fn get_task_attempts(
    Path((project_id, task_id)): Path<(Uuid, Uuid)>,
    State(app_state): State<AppState>,
) -> Result<ResponseJson<ApiResponse<Vec<TaskAttempt>>>, StatusCode> {
    // Verify task exists in project first
    match Task::exists(&app_state.db_pool, task_id, project_id).await {
        Ok(false) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to check task existence: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        Ok(true) => {}
    }

    match TaskAttempt::find_by_task_id(&app_state.db_pool, task_id).await {
        Ok(attempts) => Ok(ResponseJson(ApiResponse {
            success: true,
            data: Some(attempts),
            message: None,
        })),
        Err(e) => {
            tracing::error!("Failed to fetch task attempts for task {}: {}", task_id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_task_attempt_activities(
    Path((project_id, task_id, attempt_id)): Path<(Uuid, Uuid, Uuid)>,
    State(app_state): State<AppState>,
) -> Result<ResponseJson<ApiResponse<Vec<TaskAttemptActivityWithPrompt>>>, StatusCode> {
    // Verify task attempt exists and belongs to the correct task
    match TaskAttempt::exists_for_task(&app_state.db_pool, attempt_id, task_id, project_id).await {
        Ok(false) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to check task attempt existence: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        Ok(true) => {}
    }

    // Get activities with prompts for the task attempt
    match TaskAttemptActivity::find_with_prompts_by_task_attempt_id(&app_state.db_pool, attempt_id)
        .await
    {
        Ok(activities) => Ok(ResponseJson(ApiResponse {
            success: true,
            data: Some(activities),
            message: None,
        })),
        Err(e) => {
            tracing::error!(
                "Failed to fetch task attempt activities for attempt {}: {}",
                attempt_id,
                e
            );
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn create_task_attempt(
    Path((project_id, task_id)): Path<(Uuid, Uuid)>,
    State(app_state): State<AppState>,
    Json(payload): Json<CreateTaskAttempt>,
) -> Result<ResponseJson<ApiResponse<TaskAttempt>>, StatusCode> {
    // Verify task exists in project first
    match Task::exists(&app_state.db_pool, task_id, project_id).await {
        Ok(false) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to check task existence: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        Ok(true) => {}
    }

    // Check Agent Market API key before creating task attempt
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

    let executor_string = payload.executor.as_ref().map(|exec| exec.to_string());

    match TaskAttempt::create(&app_state.db_pool, &payload, task_id).await {
        Ok(attempt) => {
            app_state
                .track_analytics_event(
                    "task_attempt_started",
                    Some(serde_json::json!({
                        "task_id": task_id.to_string(),
                        "executor_type": executor_string.as_deref().unwrap_or("default"),
                        "attempt_id": attempt.id.to_string(),
                    })),
                )
                .await;

            // Validate Agent Market API key and create instance synchronously
            match create_agent_market_instance_sync(
                &app_state.db_pool,
                &app_state,
                attempt.id,
                task_id,
                project_id,
            )
            .await
            {
                Ok(_) => {
                    tracing::info!(
                        "Successfully created Agent Market instance for task attempt {}",
                        attempt.id
                    );
                    
                    // Create a setup execution process to show Agent Market initialization in logs
                    let process_id = Uuid::new_v4();
                    let create_process = crate::models::execution_process::CreateExecutionProcess {
                        task_attempt_id: attempt.id,
                        process_type: ExecutionProcessType::SetupScript,
                        executor_type: None, // Use default setup script executor
                        command: "setup script".to_string(),
                        args: None,
                        working_directory: project_id.to_string(),
                    };
                    
                    match ExecutionProcess::create(&app_state.db_pool, &create_process, process_id).await {
                        Ok(process) => {
                            // Add initialization messages to stdout
                            let timestamp1 = chrono::Utc::now();
                            let timestamp2 = timestamp1 + chrono::Duration::milliseconds(100);
                            let timestamp3 = timestamp2 + chrono::Duration::milliseconds(100);
                            
                            let init_messages = vec![
                                format!(
                                    r#"{{"timestamp":"{}","type":"user_message","content":"Initializing Agent Market instance..."}}"#,
                                    timestamp1.to_rfc3339()
                                ),
                                format!(
                                    r#"{{"timestamp":"{}","type":"assistant_message","content":"✅ Successfully created Agent Market instance"}}"#,
                                    timestamp2.to_rfc3339()
                                ),
                                format!(
                                    r#"{{"timestamp":"{}","type":"assistant_message","content":"Agent Market is now ready to execute your task. The agent will begin working shortly..."}}"#,
                                    timestamp3.to_rfc3339()
                                ),
                            ];
                            
                            let full_stdout = init_messages.join("\n");
                            
                            if let Err(e) = ExecutionProcess::append_stdout(&app_state.db_pool, process.id, &full_stdout).await {
                                tracing::error!("Failed to append stdout for Agent Market init: {}", e);
                            }
                            
                            // Create an activity for the Agent Market initialization while it's running
                            let activity_id = Uuid::new_v4();
                            let create_activity = CreateTaskAttemptActivity {
                                execution_process_id: process_id,
                                status: Some(TaskAttemptStatus::SetupRunning),
                                note: Some("Initializing Agent Market instance".to_string()),
                            };
                            
                            if let Err(e) = TaskAttemptActivity::create(
                                &app_state.db_pool,
                                &create_activity,
                                activity_id,
                                TaskAttemptStatus::SetupRunning,
                            )
                            .await
                            {
                                tracing::error!("Failed to create activity for Agent Market init: {}", e);
                            }
                            
                            // Small delay to ensure logs are visible
                            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                            
                            // Now mark the process as completed
                            if let Err(e) = ExecutionProcess::update_completion(
                                &app_state.db_pool,
                                process.id,
                                ExecutionProcessStatus::Completed,
                                Some(0),
                            ).await {
                                tracing::error!("Failed to update Agent Market init process status: {}", e);
                            }
                            
                            // Update activity to reflect completion
                            let completion_activity_id = Uuid::new_v4();
                            let completion_activity = CreateTaskAttemptActivity {
                                execution_process_id: process_id,
                                status: Some(TaskAttemptStatus::SetupComplete),
                                note: Some("Agent Market instance created successfully".to_string()),
                            };
                            
                            if let Err(e) = TaskAttemptActivity::create(
                                &app_state.db_pool,
                                &completion_activity,
                                completion_activity_id,
                                TaskAttemptStatus::SetupComplete,
                            )
                            .await
                            {
                                tracing::error!("Failed to create completion activity for Agent Market init: {}", e);
                            }
                            
                            // Mark setup as completed for this attempt
                            if let Err(e) = TaskAttempt::mark_setup_completed(&app_state.db_pool, attempt.id).await {
                                tracing::error!("Failed to mark setup completed: {}", e);
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to create execution process for Agent Market init: {}", e);
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to create Agent Market instance for task attempt {}: {}",
                        attempt.id,
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

            Ok(ResponseJson(ApiResponse {
                success: true,
                data: Some(attempt),
                message: Some("Task attempt created successfully".to_string()),
            }))
        }
        Err(e) => {
            tracing::error!("Failed to create task attempt: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn create_task_attempt_activity(
    Path((project_id, task_id, attempt_id)): Path<(Uuid, Uuid, Uuid)>,
    State(app_state): State<AppState>,
    Json(payload): Json<CreateTaskAttemptActivity>,
) -> Result<ResponseJson<ApiResponse<TaskAttemptActivity>>, StatusCode> {
    // Verify task attempt exists and belongs to the correct task
    match TaskAttempt::exists_for_task(&app_state.db_pool, attempt_id, task_id, project_id).await {
        Ok(false) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to check task attempt existence: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        Ok(true) => {}
    }

    let id = Uuid::new_v4();

    // Check that execution_process_id is provided in payload
    if payload.execution_process_id == Uuid::nil() {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Verify the execution process exists and belongs to this task attempt
    match ExecutionProcess::find_by_id(&app_state.db_pool, payload.execution_process_id).await {
        Ok(Some(process)) => {
            if process.task_attempt_id != attempt_id {
                return Err(StatusCode::BAD_REQUEST);
            }
        }
        Ok(None) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to verify execution process: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    // Default to SetupRunning status if not provided
    let status = payload
        .status
        .clone()
        .unwrap_or(TaskAttemptStatus::SetupRunning);

    match TaskAttemptActivity::create(&app_state.db_pool, &payload, id, status).await {
        Ok(activity) => Ok(ResponseJson(ApiResponse {
            success: true,
            data: Some(activity),
            message: Some("Task attempt activity created successfully".to_string()),
        })),
        Err(e) => {
            tracing::error!("Failed to create task attempt activity: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_task_attempt_diff(
    Path((project_id, task_id, attempt_id)): Path<(Uuid, Uuid, Uuid)>,
    State(app_state): State<AppState>,
) -> Result<ResponseJson<ApiResponse<WorktreeDiff>>, StatusCode> {
    // Verify task attempt exists and belongs to the correct task
    match TaskAttempt::exists_for_task(&app_state.db_pool, attempt_id, task_id, project_id).await {
        Ok(false) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to check task attempt existence: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        Ok(true) => {}
    }

    Ok(ResponseJson(ApiResponse {
        success: true,
        data: Some(crate::models::task_attempt::WorktreeDiff {
            files: vec![], // Agent Market handles diff generation
        }),
        message: Some("Diff generation is handled by Agent Market".to_string()),
    }))
}

#[axum::debug_handler]
pub async fn merge_task_attempt(
    Path((project_id, task_id, attempt_id)): Path<(Uuid, Uuid, Uuid)>,
    State(app_state): State<AppState>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    // Verify task attempt exists and belongs to the correct task
    match TaskAttempt::exists_for_task(&app_state.db_pool, attempt_id, task_id, project_id).await {
        Ok(false) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to check task attempt existence: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        Ok(true) => {}
    }

    Ok(ResponseJson(ApiResponse {
        success: false,
        data: None,
        message: Some("Merging is handled automatically by Agent Market".to_string()),
    }))
}

pub async fn create_github_pr(
    Path((project_id, task_id, attempt_id)): Path<(Uuid, Uuid, Uuid)>,
    State(app_state): State<AppState>,
    Json(request): Json<CreateGitHubPRRequest>,
) -> Result<ResponseJson<ApiResponse<String>>, StatusCode> {
    // Verify task attempt exists and belongs to the correct task
    match TaskAttempt::exists_for_task(&app_state.db_pool, attempt_id, task_id, project_id).await {
        Ok(false) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to check task attempt existence: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        Ok(true) => {}
    }

    // Load the user's GitHub configuration
    let config = match Config::load(&crate::utils::config_path()) {
        Ok(config) => config,
        Err(e) => {
            tracing::error!("Failed to load config: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let _github_token = match config.github.token {
        Some(token) => token,
        None => {
            return Ok(ResponseJson(ApiResponse {
                success: false,
                data: None,
                message: Some(
                    "GitHub authentication not configured. Please sign in with GitHub.".to_string(),
                ),
            }));
        }
    };

    // Get the task attempt to access the stored base branch
    let attempt = match TaskAttempt::find_by_id(&app_state.db_pool, attempt_id).await {
        Ok(Some(attempt)) => attempt,
        Ok(None) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to fetch task attempt {}: {}", attempt_id, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let _base_branch = request.base_branch.unwrap_or_else(|| {
        // Use the stored base branch from the task attempt as the default
        // Fall back to config default or "main" only if stored base branch is somehow invalid
        if !attempt.base_branch.trim().is_empty() {
            attempt.base_branch.clone()
        } else {
            config
                .github
                .default_pr_base
                .unwrap_or_else(|| "main".to_string())
        }
    });

    Ok(ResponseJson(ApiResponse {
        success: false,
        data: None,
        message: Some("PR creation is handled automatically by Agent Market".to_string()),
    }))
}

#[derive(serde::Deserialize)]
pub struct OpenEditorRequest {
    editor_type: Option<String>,
}

pub async fn open_task_attempt_in_editor(
    Path((project_id, task_id, attempt_id)): Path<(Uuid, Uuid, Uuid)>,
    State(app_state): State<AppState>,
    Json(payload): Json<Option<OpenEditorRequest>>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    // Verify task attempt exists and belongs to the correct task
    match TaskAttempt::exists_for_task(&app_state.db_pool, attempt_id, task_id, project_id).await {
        Ok(false) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to check task attempt existence: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        Ok(true) => {}
    }

    // Get the task attempt to access the worktree path
    let _attempt = match TaskAttempt::find_by_id(&app_state.db_pool, attempt_id).await {
        Ok(Some(attempt)) => attempt,
        Ok(None) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to fetch task attempt {}: {}", attempt_id, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Get editor command from config or override
    let editor_command = {
        let config_guard = app_state.get_config().read().await;
        if let Some(ref request) = payload {
            if let Some(ref editor_type) = request.editor_type {
                // Create a temporary editor config with the override
                use crate::models::config::{EditorConfig, EditorType};
                let override_editor_type = match editor_type.as_str() {
                    "vscode" => EditorType::VSCode,
                    "cursor" => EditorType::Cursor,
                    "windsurf" => EditorType::Windsurf,
                    "intellij" => EditorType::IntelliJ,
                    "zed" => EditorType::Zed,
                    "custom" => EditorType::Custom,
                    _ => config_guard.editor.editor_type.clone(),
                };
                let temp_config = EditorConfig {
                    editor_type: override_editor_type,
                    custom_command: config_guard.editor.custom_command.clone(),
                };
                temp_config.get_command()
            } else {
                config_guard.editor.get_command()
            }
        } else {
            config_guard.editor.get_command()
        }
    };

    // Open editor in the worktree directory
    let mut cmd = std::process::Command::new(&editor_command[0]);
    for arg in &editor_command[1..] {
        cmd.arg(arg);
    }
    return Ok(ResponseJson(ApiResponse {
        success: false,
        data: None,
        message: Some("Editor opening is not supported with Agent Market integration".to_string()),
    }));

    #[allow(unreachable_code)]
    match cmd.spawn() {
        Ok(_) => {
            tracing::info!(
                "Opened editor ({}) for task attempt {}",
                editor_command.join(" "),
                attempt_id
            );
            Ok(ResponseJson(ApiResponse {
                success: true,
                data: None,
                message: Some("Editor opened successfully".to_string()),
            }))
        }
        Err(e) => {
            tracing::error!(
                "Failed to open editor ({}) for attempt {}: {}",
                editor_command.join(" "),
                attempt_id,
                e
            );
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_task_attempt_branch_status(
    Path((project_id, task_id, attempt_id)): Path<(Uuid, Uuid, Uuid)>,
    State(app_state): State<AppState>,
) -> Result<ResponseJson<ApiResponse<BranchStatus>>, StatusCode> {
    // Verify task attempt exists and belongs to the correct task
    match TaskAttempt::exists_for_task(&app_state.db_pool, attempt_id, task_id, project_id).await {
        Ok(false) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to check task attempt existence: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        Ok(true) => {}
    }

    Ok(ResponseJson(ApiResponse {
        success: true,
        data: Some(BranchStatus {
            is_behind: false,
            commits_behind: 0,
            commits_ahead: 0,
            up_to_date: true,
            merged: false,
            has_uncommitted_changes: false,
            base_branch_name: "main".to_string(),
        }),
        message: Some("Branch status is handled by Agent Market".to_string()),
    }))
}

#[axum::debug_handler]
pub async fn rebase_task_attempt(
    Path((project_id, task_id, attempt_id)): Path<(Uuid, Uuid, Uuid)>,
    State(app_state): State<AppState>,
    request_body: Option<Json<RebaseTaskAttemptRequest>>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    // Verify task attempt exists and belongs to the correct task
    match TaskAttempt::exists_for_task(&app_state.db_pool, attempt_id, task_id, project_id).await {
        Ok(false) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to check task attempt existence: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        Ok(true) => {}
    }

    // Extract new base branch from request body if provided
    let _new_base_branch = request_body.and_then(|body| body.new_base_branch.clone());

    Ok(ResponseJson(ApiResponse {
        success: false,
        data: None,
        message: Some("Rebasing is handled automatically by Agent Market".to_string()),
    }))
}

pub async fn get_task_attempt_execution_processes(
    Path((project_id, task_id, attempt_id)): Path<(Uuid, Uuid, Uuid)>,
    State(app_state): State<AppState>,
) -> Result<ResponseJson<ApiResponse<Vec<ExecutionProcessSummary>>>, StatusCode> {
    // Verify task attempt exists and belongs to the correct task
    match TaskAttempt::exists_for_task(&app_state.db_pool, attempt_id, task_id, project_id).await {
        Ok(false) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to check task attempt existence: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        Ok(true) => {}
    }

    match ExecutionProcess::find_summaries_by_task_attempt_id(&app_state.db_pool, attempt_id).await
    {
        Ok(processes) => Ok(ResponseJson(ApiResponse {
            success: true,
            data: Some(processes),
            message: None,
        })),
        Err(e) => {
            tracing::error!(
                "Failed to fetch execution processes for attempt {}: {}",
                attempt_id,
                e
            );
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_execution_process(
    Path((project_id, process_id)): Path<(Uuid, Uuid)>,
    State(app_state): State<AppState>,
) -> Result<ResponseJson<ApiResponse<ExecutionProcess>>, StatusCode> {
    match ExecutionProcess::find_by_id(&app_state.db_pool, process_id).await {
        Ok(Some(process)) => {
            // Verify the process belongs to a task attempt in the correct project
            match TaskAttempt::find_by_id(&app_state.db_pool, process.task_attempt_id).await {
                Ok(Some(attempt)) => {
                    match Task::find_by_id(&app_state.db_pool, attempt.task_id).await {
                        Ok(Some(task)) if task.project_id == project_id => {
                            Ok(ResponseJson(ApiResponse {
                                success: true,
                                data: Some(process),
                                message: None,
                            }))
                        }
                        Ok(Some(_)) => Err(StatusCode::NOT_FOUND), // Wrong project
                        Ok(None) => Err(StatusCode::NOT_FOUND),
                        Err(e) => {
                            tracing::error!("Failed to fetch task: {}", e);
                            Err(StatusCode::INTERNAL_SERVER_ERROR)
                        }
                    }
                }
                Ok(None) => Err(StatusCode::NOT_FOUND),
                Err(e) => {
                    tracing::error!("Failed to fetch task attempt: {}", e);
                    Err(StatusCode::INTERNAL_SERVER_ERROR)
                }
            }
        }
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to fetch execution process {}: {}", process_id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[axum::debug_handler]
pub async fn stop_all_execution_processes(
    Path((project_id, task_id, attempt_id)): Path<(Uuid, Uuid, Uuid)>,
    State(app_state): State<AppState>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    // Verify task attempt exists and belongs to the correct task
    match TaskAttempt::exists_for_task(&app_state.db_pool, attempt_id, task_id, project_id).await {
        Ok(false) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to check task attempt existence: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        Ok(true) => {}
    }

    // Get all execution processes for the task attempt
    let processes =
        match ExecutionProcess::find_by_task_attempt_id(&app_state.db_pool, attempt_id).await {
            Ok(processes) => processes,
            Err(e) => {
                tracing::error!(
                    "Failed to fetch execution processes for attempt {}: {}",
                    attempt_id,
                    e
                );
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        };

    let mut stopped_count = 0;
    let mut errors = Vec::new();

    // Stop all running processes
    for process in processes {
        match app_state.stop_running_execution_by_id(process.id).await {
            Ok(true) => {
                stopped_count += 1;

                // Update the execution process status in the database
                if let Err(e) = ExecutionProcess::update_completion(
                    &app_state.db_pool,
                    process.id,
                    crate::models::execution_process::ExecutionProcessStatus::Killed,
                    None,
                )
                .await
                {
                    tracing::error!("Failed to update execution process status: {}", e);
                    errors.push(format!("Failed to update process {} status", process.id));
                } else {
                    // Create activity record for stopped processes (skip dev servers)
                    if !matches!(
                        process.process_type,
                        crate::models::execution_process::ExecutionProcessType::DevServer
                    ) {
                        let activity_id = Uuid::new_v4();
                        let create_activity = CreateTaskAttemptActivity {
                            execution_process_id: process.id,
                            status: Some(TaskAttemptStatus::ExecutorFailed),
                            note: Some(format!(
                                "Execution process {:?} ({}) stopped by user",
                                process.process_type, process.id
                            )),
                        };

                        if let Err(e) = TaskAttemptActivity::create(
                            &app_state.db_pool,
                            &create_activity,
                            activity_id,
                            TaskAttemptStatus::ExecutorFailed,
                        )
                        .await
                        {
                            tracing::error!("Failed to create stopped activity: {}", e);
                            errors.push(format!(
                                "Failed to create activity for process {}",
                                process.id
                            ));
                        }
                    }
                }
            }
            Ok(false) => {
                // Process was not running, which is fine
            }
            Err(e) => {
                tracing::error!("Failed to stop execution process {}: {}", process.id, e);
                errors.push(format!("Failed to stop process {}: {}", process.id, e));
            }
        }
    }

    if !errors.is_empty() {
        return Ok(ResponseJson(ApiResponse {
            success: false,
            data: None,
            message: Some(format!(
                "Stopped {} processes, but encountered errors: {}",
                stopped_count,
                errors.join(", ")
            )),
        }));
    }

    if stopped_count == 0 {
        return Ok(ResponseJson(ApiResponse {
            success: true,
            data: None,
            message: Some("No running processes found to stop".to_string()),
        }));
    }

    Ok(ResponseJson(ApiResponse {
        success: true,
        data: None,
        message: Some(format!(
            "Successfully stopped {} execution processes",
            stopped_count
        )),
    }))
}

#[axum::debug_handler]
pub async fn stop_execution_process(
    Path((project_id, task_id, attempt_id, process_id)): Path<(Uuid, Uuid, Uuid, Uuid)>,
    State(app_state): State<AppState>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    // Verify task attempt exists and belongs to the correct task
    match TaskAttempt::exists_for_task(&app_state.db_pool, attempt_id, task_id, project_id).await {
        Ok(false) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to check task attempt existence: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        Ok(true) => {}
    }

    // Verify execution process exists and belongs to the task attempt
    let process = match ExecutionProcess::find_by_id(&app_state.db_pool, process_id).await {
        Ok(Some(process)) if process.task_attempt_id == attempt_id => process,
        Ok(Some(_)) => return Err(StatusCode::NOT_FOUND), // Process exists but wrong attempt
        Ok(None) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to fetch execution process {}: {}", process_id, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Stop the specific execution process
    let stopped = match app_state.stop_running_execution_by_id(process_id).await {
        Ok(stopped) => stopped,
        Err(e) => {
            tracing::error!("Failed to stop execution process {}: {}", process_id, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    if !stopped {
        return Ok(ResponseJson(ApiResponse {
            success: true,
            data: None,
            message: Some("Execution process was not running".to_string()),
        }));
    }

    // Update the execution process status in the database
    if let Err(e) = ExecutionProcess::update_completion(
        &app_state.db_pool,
        process_id,
        crate::models::execution_process::ExecutionProcessStatus::Killed,
        None,
    )
    .await
    {
        tracing::error!("Failed to update execution process status: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    // Create activity record for stopped processes (skip dev servers)
    if !matches!(
        process.process_type,
        crate::models::execution_process::ExecutionProcessType::DevServer
    ) {
        let activity_id = Uuid::new_v4();
        let create_activity = CreateTaskAttemptActivity {
            execution_process_id: process_id,
            status: Some(TaskAttemptStatus::ExecutorFailed),
            note: Some(format!(
                "Execution process {:?} ({}) stopped by user",
                process.process_type, process_id
            )),
        };

        if let Err(e) = TaskAttemptActivity::create(
            &app_state.db_pool,
            &create_activity,
            activity_id,
            TaskAttemptStatus::ExecutorFailed,
        )
        .await
        {
            tracing::error!("Failed to create stopped activity: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    Ok(ResponseJson(ApiResponse {
        success: true,
        data: None,
        message: Some(format!(
            "Execution process {} stopped successfully",
            process_id
        )),
    }))
}


#[axum::debug_handler]
pub async fn delete_task_attempt_file(
    Path((project_id, task_id, attempt_id)): Path<(Uuid, Uuid, Uuid)>,
    State(app_state): State<AppState>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    // Verify task attempt exists and belongs to the correct task
    match TaskAttempt::exists_for_task(&app_state.db_pool, attempt_id, task_id, project_id).await {
        Ok(false) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to check task attempt existence: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        Ok(true) => {}
    }

    Ok(ResponseJson(ApiResponse {
        success: false,
        data: None,
        message: Some("File operations are handled automatically by Agent Market".to_string()),
    }))
}

pub async fn create_followup_attempt(
    Path((project_id, task_id, attempt_id)): Path<(Uuid, Uuid, Uuid)>,
    State(app_state): State<AppState>,
    Json(payload): Json<CreateFollowUpAttempt>,
) -> Result<ResponseJson<ApiResponse<FollowUpResponse>>, StatusCode> {
    // Verify task attempt exists
    if !TaskAttempt::exists_for_task(&app_state.db_pool, attempt_id, task_id, project_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to check task attempt existence: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    {
        return Err(StatusCode::NOT_FOUND);
    }

    // Check Agent Market API key before starting follow-up execution
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

    // Start follow-up execution synchronously to catch errors
    match TaskAttempt::start_followup_execution(
        &app_state.db_pool,
        &app_state,
        attempt_id,
        task_id,
        project_id,
        &payload.prompt,
    )
    .await
    {
        Ok(actual_attempt_id) => {
            let created_new_attempt = actual_attempt_id != attempt_id;
            let message = if created_new_attempt {
                format!(
                    "Follow-up execution started on new attempt {} (original worktree was deleted)",
                    actual_attempt_id
                )
            } else {
                "Follow-up execution started successfully".to_string()
            };

            Ok(ResponseJson(ApiResponse {
                success: true,
                data: Some(FollowUpResponse {
                    message: message.clone(),
                    actual_attempt_id,
                    created_new_attempt,
                }),
                message: Some(message),
            }))
        }
        Err(e) => {
            tracing::error!(
                "Failed to start follow-up execution for task attempt {}: {}",
                attempt_id,
                e
            );
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn start_dev_server(
    Path((project_id, task_id, attempt_id)): Path<(Uuid, Uuid, Uuid)>,
    State(app_state): State<AppState>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    // Verify task attempt exists and belongs to the correct task
    match TaskAttempt::exists_for_task(&app_state.db_pool, attempt_id, task_id, project_id).await {
        Ok(false) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to check task attempt existence: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        Ok(true) => {}
    }

    // Stop any existing dev servers for this project
    let existing_dev_servers =
        match ExecutionProcess::find_running_dev_servers_by_project(&app_state.db_pool, project_id)
            .await
        {
            Ok(servers) => servers,
            Err(e) => {
                tracing::error!(
                    "Failed to find running dev servers for project {}: {}",
                    project_id,
                    e
                );
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        };

    for dev_server in existing_dev_servers {
        tracing::info!(
            "Stopping existing dev server {} for project {}",
            dev_server.id,
            project_id
        );

        // Stop the running process
        if let Err(e) = app_state.stop_running_execution_by_id(dev_server.id).await {
            tracing::error!("Failed to stop dev server {}: {}", dev_server.id, e);
        } else {
            // Update the execution process status in the database
            if let Err(e) = ExecutionProcess::update_completion(
                &app_state.db_pool,
                dev_server.id,
                crate::models::execution_process::ExecutionProcessStatus::Killed,
                None,
            )
            .await
            {
                tracing::error!(
                    "Failed to update dev server {} status: {}",
                    dev_server.id,
                    e
                );
            }
        }
    }

    // Start dev server execution
    match TaskAttempt::start_dev_server(
        &app_state.db_pool,
        &app_state,
        attempt_id,
        task_id,
        project_id,
    )
    .await
    {
        Ok(_) => Ok(ResponseJson(ApiResponse {
            success: true,
            data: None,
            message: Some("Dev server started successfully".to_string()),
        })),
        Err(e) => {
            tracing::error!(
                "Failed to start dev server for task attempt {}: {}",
                attempt_id,
                e
            );
            Ok(ResponseJson(ApiResponse {
                success: false,
                data: None,
                message: Some(e.to_string()),
            }))
        }
    }
}

pub async fn get_task_attempt_execution_state(
    Path((project_id, task_id, attempt_id)): Path<(Uuid, Uuid, Uuid)>,
    State(app_state): State<AppState>,
) -> Result<ResponseJson<ApiResponse<TaskAttemptState>>, StatusCode> {
    // Verify task attempt exists and belongs to the correct task
    match TaskAttempt::exists_for_task(&app_state.db_pool, attempt_id, task_id, project_id).await {
        Ok(false) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to check task attempt existence: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        Ok(true) => {}
    }

    // Get the execution state
    match TaskAttempt::get_execution_state(&app_state.db_pool, attempt_id, task_id, project_id)
        .await
    {
        Ok(state) => Ok(ResponseJson(ApiResponse {
            success: true,
            data: Some(state),
            message: None,
        })),
        Err(e) => {
            tracing::error!(
                "Failed to get execution state for task attempt {}: {}",
                attempt_id,
                e
            );
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_execution_process_normalized_logs(
    Path((project_id, process_id)): Path<(Uuid, Uuid)>,
    State(app_state): State<AppState>,
) -> Result<ResponseJson<ApiResponse<NormalizedConversation>>, StatusCode> {
    // Get the execution process and verify it belongs to the correct project
    let process = match ExecutionProcess::find_by_id(&app_state.db_pool, process_id).await {
        Ok(Some(process)) => process,
        Ok(None) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to fetch execution process {}: {}", process_id, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Verify the process belongs to a task attempt in the correct project
    let attempt = match TaskAttempt::find_by_id(&app_state.db_pool, process.task_attempt_id).await {
        Ok(Some(attempt)) => attempt,
        Ok(None) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to fetch task attempt: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let _task = match Task::find_by_id(&app_state.db_pool, attempt.task_id).await {
        Ok(Some(task)) if task.project_id == project_id => task,
        Ok(Some(_)) => return Err(StatusCode::NOT_FOUND), // Wrong project
        Ok(None) => return Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to fetch task: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Get executor session data for this execution process
    let executor_session =
        match ExecutorSession::find_by_execution_process_id(&app_state.db_pool, process_id).await {
            Ok(session) => session,
            Err(e) => {
                tracing::error!(
                    "Failed to fetch executor session for process {}: {}",
                    process_id,
                    e
                );
                None
            }
        };

    // Handle the case where no logs are available
    let has_stdout =
        process.stdout.is_some() && !process.stdout.as_ref().unwrap().trim().is_empty();
    let has_stderr =
        process.stderr.is_some() && !process.stderr.as_ref().unwrap().trim().is_empty();

    // If the process is still running and has no stdout/stderr, return empty logs
    if process.status == ExecutionProcessStatus::Running && !has_stdout && !has_stderr {
        return Ok(ResponseJson(ApiResponse {
            success: true,
            data: Some(NormalizedConversation {
                entries: vec![],
                session_id: None,
                executor_type: process
                    .executor_type
                    .clone()
                    .unwrap_or("unknown".to_string()),
                prompt: executor_session.as_ref().and_then(|s| s.prompt.clone()),
                summary: executor_session.as_ref().and_then(|s| s.summary.clone()),
            }),
            message: None,
        }));
    }

    // If process is completed but has no logs, return appropriate error
    if process.status != ExecutionProcessStatus::Running && !has_stdout && !has_stderr {
        return Ok(ResponseJson(ApiResponse {
            success: false,
            data: None,
            message: Some("No logs available for this execution process".to_string()),
        }));
    }

    // Parse stdout as JSONL using executor normalization
    let mut stdout_entries = Vec::new();
    if let Some(stdout) = &process.stdout {
        if !stdout.trim().is_empty() {
            // Determine executor type and create appropriate executor for normalization
            let executor_type = process.executor_type.as_deref().unwrap_or("unknown");

            let executor_config = if process.process_type == ExecutionProcessType::SetupScript {
                // For setup scripts, use the setup script executor
                ExecutorConfig::SetupScript {
                    script: executor_session
                        .as_ref()
                        .and_then(|s| s.prompt.clone())
                        .unwrap_or_else(|| "setup script".to_string()),
                }
            } else {
                match executor_type {
                    "amp" => ExecutorConfig::Amp,
                    "claude" => ExecutorConfig::Claude,
                    "echo" => ExecutorConfig::Echo,
                    "gemini" => ExecutorConfig::Gemini,
                    "opencode" => ExecutorConfig::Opencode,
                    _ => {
                        tracing::warn!(
                            "Unsupported executor type: {}, cannot normalize logs properly",
                            executor_type
                        );
                        return Ok(ResponseJson(ApiResponse {
                            success: false,
                            data: None,
                            message: Some(format!("Unsupported executor type: {}", executor_type)),
                        }));
                    }
                }
            };

            let executor = executor_config.create_executor();

            // Use the working directory path for normalization
            // Try to canonicalize if the directory exists, otherwise use the stored path as-is
            let working_dir_path = match std::fs::canonicalize(&process.working_directory) {
                Ok(canonical_path) => {
                    tracing::debug!(
                        "Using canonical path for normalization: {}",
                        canonical_path.display()
                    );
                    canonical_path.to_string_lossy().to_string()
                }
                Err(_) => {
                    tracing::debug!(
                            "Working directory {} no longer exists, using stored path for normalization",
                            process.working_directory
                        );
                    process.working_directory.clone()
                }
            };

            // Normalize stdout logs with error handling
            match executor.normalize_logs(stdout, &working_dir_path) {
                Ok(normalized) => {
                    stdout_entries = normalized.entries;
                    tracing::debug!(
                        "Successfully normalized {} stdout entries for process {}",
                        stdout_entries.len(),
                        process_id
                    );
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to normalize stdout for process {}: {}",
                        process_id,
                        e
                    );
                    return Ok(ResponseJson(ApiResponse {
                        success: false,
                        data: None,
                        message: Some(format!("Failed to normalize logs: {}", e)),
                    }));
                }
            }
        }
    }

    // Parse stderr chunks separated by boundary markers
    let mut stderr_entries = Vec::new();
    if let Some(stderr) = &process.stderr {
        let trimmed = stderr.trim();
        if !trimmed.is_empty() {
            // Split stderr by chunk boundaries and create separate error messages
            let chunks: Vec<&str> = trimmed.split("---STDERR_CHUNK_BOUNDARY---").collect();

            for chunk in chunks {
                let chunk_trimmed = chunk.trim();
                if !chunk_trimmed.is_empty() {
                    // Filter out any remaining boundary markers from the chunk content
                    let filtered_content = chunk_trimmed.replace("---STDERR_CHUNK_BOUNDARY---", "");
                    if !filtered_content.trim().is_empty() {
                        stderr_entries.push(NormalizedEntry {
                            timestamp: Some(chrono::Utc::now().to_rfc3339()),
                            entry_type: NormalizedEntryType::ErrorMessage,
                            content: filtered_content.trim().to_string(),
                            metadata: None,
                        });
                    }
                }
            }

            tracing::debug!(
                "Processed stderr content into {} error messages for process {}",
                stderr_entries.len(),
                process_id
            );
        }
    }

    // Merge stdout and stderr entries chronologically
    let mut all_entries = Vec::new();
    all_entries.extend(stdout_entries);
    all_entries.extend(stderr_entries);

    // Sort by timestamp (entries without timestamps go to the end)
    all_entries.sort_by(|a, b| match (&a.timestamp, &b.timestamp) {
        (Some(a_ts), Some(b_ts)) => a_ts.cmp(b_ts),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });

    // Create final normalized conversation
    let executor_type = if process.process_type == ExecutionProcessType::SetupScript {
        "setup_script".to_string()
    } else {
        process
            .executor_type
            .clone()
            .unwrap_or("unknown".to_string())
    };

    let normalized_conversation = NormalizedConversation {
        entries: all_entries,
        session_id: None,
        executor_type,
        prompt: executor_session.as_ref().and_then(|s| s.prompt.clone()),
        summary: executor_session.as_ref().and_then(|s| s.summary.clone()),
    };

    Ok(ResponseJson(ApiResponse {
        success: true,
        data: Some(normalized_conversation),
        message: None,
    }))
}

/// Create an Agent Market instance for the task attempt (synchronous version that returns errors)
async fn create_agent_market_instance_sync(
    pool: &sqlx::SqlitePool,
    _app_state: &AppState,
    attempt_id: Uuid,
    task_id: Uuid,
    project_id: Uuid,
) -> Result<(), anyhow::Error> {
    tracing::info!(
        "Creating Agent Market instance for task attempt {} (task: {}, project: {})",
        attempt_id,
        task_id,
        project_id
    );

    // Get the task and project
    let task = Task::find_by_id(pool, task_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Task not found"))?;
    
    let project = Project::find_by_id(pool, project_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Project not found"))?;

    // Load configuration to get API key
    let config = Config::load(&crate::utils::config_path())?;
    
    // Check if Agent Market API key is configured
    let api_key = config.agent_market_api_key
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Agent Market API key is required to run tasks. Please configure your API key in settings."))?;

    // Create Agent Market client and instance
    let client = AgentMarketClient::new();
    
    // Use a default max reward for now - this could be configurable
    let max_reward = 1000; // $10.00 in cents
    
    // Return error instead of swallowing it
    client.create_instance(api_key, &task, &project, max_reward).await?;
    
    tracing::info!(
        "Successfully created Agent Market instance for task attempt {}",
        attempt_id
    );
    
    Ok(())
}

/// Create an Agent Market instance for the task attempt
async fn create_agent_market_instance(
    pool: &sqlx::SqlitePool,
    _app_state: &AppState,
    attempt_id: Uuid,
    task_id: Uuid,
    project_id: Uuid,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing::info!(
        "Creating Agent Market instance for task attempt {} (task: {}, project: {})",
        attempt_id,
        task_id,
        project_id
    );

    // Get the task and project
    let task = Task::find_by_id(pool, task_id)
        .await?
        .ok_or("Task not found")?;
    
    let project = Project::find_by_id(pool, project_id)
        .await?
        .ok_or("Project not found")?;

    // Load configuration to get API key
    let config = Config::load(&crate::utils::config_path())?;
    
    // Check if Agent Market API key is configured
    let api_key = config.agent_market_api_key
        .as_ref()
        .ok_or("Agent Market API key is required to run tasks. Please configure your API key in settings.")?;

    // Create Agent Market client and instance
    let client = AgentMarketClient::new();
    
    // Use a default max reward for now - this could be configurable
    let max_reward = 1000; // $10.00 in cents
    
    match client.create_instance(api_key, &task, &project, max_reward).await {
        Ok(response) => {
            tracing::info!(
                "Successfully created Agent Market instance for task attempt {}: {:?}",
                attempt_id,
                response
            );
            
            // TODO: Store the instance ID in the database if needed for tracking
            // let instance_id = response.get("id").and_then(|v| v.as_str());
            
            Ok(())
        }
        Err(e) => {
            tracing::warn!(
                "Failed to create Agent Market instance for task attempt {}: {}. Task attempt will continue without Agent Market integration.",
                attempt_id,
                e
            );
            // Don't fail the task attempt creation if Agent Market integration fails
            Ok(())
        }
    }
}

pub fn task_attempts_router() -> Router<AppState> {
    use axum::routing::post;

    Router::new()
        .route(
            "/projects/:project_id/tasks/:task_id/attempts",
            get(get_task_attempts).post(create_task_attempt),
        )
        .route(
            "/projects/:project_id/tasks/:task_id/attempts/:attempt_id/activities",
            get(get_task_attempt_activities).post(create_task_attempt_activity),
        )

        .route(
            "/projects/:project_id/tasks/:task_id/attempts/:attempt_id/diff",
            get(get_task_attempt_diff),
        )
        .route(
            "/projects/:project_id/tasks/:task_id/attempts/:attempt_id/merge",
            post(merge_task_attempt),
        )
        .route(
            "/projects/:project_id/tasks/:task_id/attempts/:attempt_id/branch-status",
            get(get_task_attempt_branch_status),
        )
        .route(
            "/projects/:project_id/tasks/:task_id/attempts/:attempt_id/rebase",
            post(rebase_task_attempt),
        )
        .route(
            "/projects/:project_id/tasks/:task_id/attempts/:attempt_id/open-editor",
            post(open_task_attempt_in_editor),
        )
        .route(
            "/projects/:project_id/tasks/:task_id/attempts/:attempt_id/delete-file",
            post(delete_task_attempt_file),
        )
        .route(
            "/projects/:project_id/tasks/:task_id/attempts/:attempt_id/create-pr",
            post(create_github_pr),
        )
        .route(
            "/projects/:project_id/tasks/:task_id/attempts/:attempt_id/execution-processes",
            get(get_task_attempt_execution_processes),
        )
        .route(
            "/projects/:project_id/tasks/:task_id/attempts/:attempt_id/stop",
            post(stop_all_execution_processes),
        )
        .route(
            "/projects/:project_id/tasks/:task_id/attempts/:attempt_id/execution-processes/:process_id/stop",
            post(stop_execution_process),
        )
        .route(
            "/projects/:project_id/execution-processes/:process_id",
            get(get_execution_process),
        )
        .route(
            "/projects/:project_id/execution-processes/:process_id/normalized-logs",
            get(get_execution_process_normalized_logs),
        )
        .route(
            "/projects/:project_id/tasks/:task_id/attempts/:attempt_id/follow-up",
            post(create_followup_attempt),
        )
        .route(
            "/projects/:project_id/tasks/:task_id/attempts/:attempt_id/start-dev-server",
            post(start_dev_server),
        )
        .route(
            "/projects/:project_id/tasks/:task_id/attempts/:attempt_id",
            get(get_task_attempt_execution_state),
        )
}
