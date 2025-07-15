use git2::Repository;
use uuid::Uuid;

use crate::{
    app_state::AppState,
    models::{
        execution_process::{ExecutionProcess, ExecutionProcessStatus, ExecutionProcessType},
        task::{Task, TaskStatus},
        task_attempt::{TaskAttempt, TaskAttemptStatus},
        task_attempt_activity::{CreateTaskAttemptActivity, TaskAttemptActivity},
    },
    services::{NotificationConfig, NotificationService, ProcessService},
};

/// Delegation context structure
#[derive(Debug, serde::Deserialize)]
struct DelegationContext {
    delegate_to: String,
    operation_params: DelegationOperationParams,
}

#[derive(Debug, serde::Deserialize)]
struct DelegationOperationParams {
    task_id: uuid::Uuid,
    project_id: uuid::Uuid,
    attempt_id: uuid::Uuid,
    additional: Option<serde_json::Value>,
}

/// Parse delegation context from process args JSON
fn parse_delegation_context(args_json: &str) -> Option<DelegationContext> {
    // Parse the args JSON array
    if let Ok(args_array) = serde_json::from_str::<serde_json::Value>(args_json) {
        if let Some(args) = args_array.as_array() {
            // Look for --delegation-context flag
            for (i, arg) in args.iter().enumerate() {
                if let Some(arg_str) = arg.as_str() {
                    if arg_str == "--delegation-context" && i + 1 < args.len() {
                        // Next argument should be the delegation context JSON
                        if let Some(context_str) = args[i + 1].as_str() {
                            if let Ok(context) =
                                serde_json::from_str::<DelegationContext>(context_str)
                            {
                                return Some(context);
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Handle delegation after setup completion
async fn handle_setup_delegation(app_state: &AppState, delegation_context: DelegationContext) {
    let params = &delegation_context.operation_params;
    let task_id = params.task_id;
    let project_id = params.project_id;
    let attempt_id = params.attempt_id;

    tracing::info!(
        "Delegating to {} after setup completion for attempt {}",
        delegation_context.delegate_to,
        attempt_id
    );

    let result = match delegation_context.delegate_to.as_str() {
        "dev_server" => {
            ProcessService::start_dev_server_direct(
                &app_state.db_pool,
                app_state,
                attempt_id,
                task_id,
                project_id,
            )
            .await
        }
        "coding_agent" => {
            ProcessService::start_coding_agent(
                &app_state.db_pool,
                app_state,
                attempt_id,
                task_id,
                project_id,
            )
            .await
        }
        "followup" => {
            let prompt = params
                .additional
                .as_ref()
                .and_then(|a| a.get("prompt"))
                .and_then(|p| p.as_str())
                .unwrap_or("");

            ProcessService::start_followup_execution_direct(
                &app_state.db_pool,
                app_state,
                attempt_id,
                task_id,
                project_id,
                prompt,
            )
            .await
            .map(|_| ())
        }
        _ => {
            tracing::error!(
                "Unknown delegation target: {}",
                delegation_context.delegate_to
            );
            return;
        }
    };

    if let Err(e) = result {
        tracing::error!(
            "Failed to delegate to {} after setup completion: {}",
            delegation_context.delegate_to,
            e
        );
    } else {
        tracing::info!(
            "Successfully delegated to {} after setup completion",
            delegation_context.delegate_to
        );
    }
}

/// Commit any unstaged changes in the worktree after execution completion
#[allow(dead_code)]
async fn commit_execution_changes(
    worktree_path: &str,
    attempt_id: Uuid,
    summary: Option<&str>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Run git operations in a blocking task since git2 is synchronous
    let worktree_path = worktree_path.to_string();
    let summary = summary.map(|s| s.to_string());
    tokio::task::spawn_blocking(move || {
        let worktree_repo = Repository::open(&worktree_path)?;

        // Check if there are any changes to commit
        let status = worktree_repo.statuses(None)?;
        let has_changes = status.iter().any(|entry| {
            let flags = entry.status();
            flags.contains(git2::Status::INDEX_NEW)
                || flags.contains(git2::Status::INDEX_MODIFIED)
                || flags.contains(git2::Status::INDEX_DELETED)
                || flags.contains(git2::Status::WT_NEW)
                || flags.contains(git2::Status::WT_MODIFIED)
                || flags.contains(git2::Status::WT_DELETED)
        });

        if !has_changes {
            return Ok::<(), Box<dyn std::error::Error + Send + Sync>>(());
        }

        // Get the current signature for commits
        let signature = worktree_repo.signature()?;

        // Get the current HEAD commit
        let head = worktree_repo.head()?;
        let parent_commit = head.peel_to_commit()?;

        // Stage all changes
        let mut worktree_index = worktree_repo.index()?;
        worktree_index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
        worktree_index.write()?;

        let tree_id = worktree_index.write_tree()?;
        let tree = worktree_repo.find_tree(tree_id)?;

        // Create commit for the changes
        let commit_message = if let Some(ref summary_msg) = summary {
            summary_msg.clone()
        } else {
            format!("Task attempt {} - Final changes", attempt_id)
        };
        worktree_repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            &commit_message,
            &tree,
            &[&parent_commit],
        )?;

        Ok(())
    })
    .await??;

    Ok(())
}

/// Check if worktree has uncommitted changes and warn if so
#[allow(dead_code)]
fn check_uncommitted_changes(worktree_path: &str) {
    if let Ok(repo) = Repository::open(worktree_path) {
        if let Ok(statuses) = repo.statuses(None) {
            // Simplified check: any status entry indicates changes
            if !statuses.is_empty() {
                tracing::warn!(
                    "Deleting worktree {} with uncommitted changes",
                    worktree_path
                );
            }
        }
    }
}

/// Delete a single git worktree and its filesystem directory using WorktreeManager
pub async fn execution_monitor(app_state: AppState) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
    let _cleanup_interval = tokio::time::interval(tokio::time::Duration::from_secs(1800)); // 30 minutes

    loop {
        tokio::select! {
            _ = interval.tick() => {
                // Check for completed processes FIRST to avoid race conditions
                let completed_executions = app_state.get_running_executions_for_monitor().await;

                // Handle completed executions
                for (execution_process_id, task_attempt_id, success, exit_code) in completed_executions {
                    let status_text = if success {
                        "completed successfully"
                    } else {
                        "failed"
                    };
                    let exit_text = if let Some(code) = exit_code {
                        format!(" with exit code {}", code)
                    } else {
                        String::new()
                    };

                    tracing::info!(
                        "Execution {} {}{}",
                        execution_process_id,
                        status_text,
                        exit_text
                    );

                    // Update the execution process record
                    let execution_status = if success {
                        ExecutionProcessStatus::Completed
                    } else {
                        ExecutionProcessStatus::Failed
                    };

                    if let Err(e) = ExecutionProcess::update_completion(
                        &app_state.db_pool,
                        execution_process_id,
                        execution_status,
                        exit_code,
                    )
                    .await
                    {
                        tracing::error!(
                            "Failed to update execution process {} completion: {}",
                            execution_process_id,
                            e
                        );
                    }

                    // Get the execution process to determine next steps
                    if let Ok(Some(execution_process)) =
                        ExecutionProcess::find_by_id(&app_state.db_pool, execution_process_id).await
                    {
                        match execution_process.process_type {
                            ExecutionProcessType::SetupScript => {
                                handle_setup_completion(
                                    &app_state,
                                    task_attempt_id,
                                    execution_process_id,
                                    execution_process,
                                    success,
                                    exit_code,
                                )
                                .await;
                            }
                            ExecutionProcessType::CodingAgent => {
                                handle_coding_agent_completion(
                                    &app_state,
                                    task_attempt_id,
                                    execution_process_id,
                                    execution_process,
                                    success,
                                    exit_code,
                                )
                                .await;
                            }
                            ExecutionProcessType::DevServer => {
                                handle_dev_server_completion(
                                    &app_state,
                                    task_attempt_id,
                                    execution_process_id,
                                    execution_process,
                                    success,
                                    exit_code,
                                )
                                .await;
                            }
                        }
                    } else {
                        tracing::error!(
                            "Failed to find execution process {} for completion handling",
                            execution_process_id
                        );
                    }
                }

                // Check for orphaned execution processes AFTER handling completions
                // Add a small delay to ensure completed processes are properly handled first
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

                let running_processes = match ExecutionProcess::find_running(&app_state.db_pool).await {
                    Ok(processes) => processes,
                    Err(e) => {
                        tracing::error!("Failed to query running execution processes: {}", e);
                        continue;
                    }
                };

                for process in running_processes {
                    // Check if this process is not actually running in the app state
                    if !app_state.has_running_execution(process.task_attempt_id).await {
                        // Additional check: if the process was recently updated, skip it to prevent race conditions
                        let now = chrono::Utc::now();
                        let time_since_update = now - process.updated_at;
                        if time_since_update.num_seconds() < 10 {
                            // Process was updated within last 10 seconds, likely just completed
                            tracing::debug!(
                                "Skipping recently updated orphaned process {} (updated {} seconds ago)",
                                process.id,
                                time_since_update.num_seconds()
                            );
                            continue;
                        }

                        // This is truly an orphaned execution process - mark it as failed
                        tracing::info!(
                            "Found orphaned execution process {} for task attempt {}",
                            process.id,
                            process.task_attempt_id
                        );
                        // This is truly an orphaned execution process - mark it as failed
                        tracing::info!(
                            "Found orphaned execution process {} for task attempt {}",
                            process.id,
                            process.task_attempt_id
                        );

                        // Update the execution process status first
                        if let Err(e) = ExecutionProcess::update_completion(
                            &app_state.db_pool,
                            process.id,
                            ExecutionProcessStatus::Failed,
                            None, // No exit code for orphaned processes
                        )
                        .await
                        {
                            tracing::error!(
                                "Failed to update orphaned execution process {} status: {}",
                                process.id,
                                e
                            );
                            continue;
                        }

                        // Create task attempt activity for non-dev server processes
                        if process.process_type != ExecutionProcessType::DevServer {
                            let activity_id = Uuid::new_v4();
                            let create_activity = CreateTaskAttemptActivity {
                                execution_process_id: process.id,
                                status: Some(TaskAttemptStatus::ExecutorFailed),
                                note: Some("Execution lost (server restart or crash)".to_string()),
                            };

                            if let Err(e) = TaskAttemptActivity::create(
                                &app_state.db_pool,
                                &create_activity,
                                activity_id,
                                TaskAttemptStatus::ExecutorFailed,
                            )
                            .await
                            {
                                tracing::error!(
                                    "Failed to create failed activity for orphaned process: {}",
                                    e
                                );
                                continue;
                            }
                        }

                        tracing::info!("Marked orphaned execution process {} as failed", process.id);

                        // Update task status to InReview for coding agent and setup script failures
                        if matches!(
                            process.process_type,
                            ExecutionProcessType::CodingAgent | ExecutionProcessType::SetupScript
                        ) {
                            if let Ok(Some(task_attempt)) =
                                TaskAttempt::find_by_id(&app_state.db_pool, process.task_attempt_id).await
                            {
                                if let Ok(Some(task)) =
                                    Task::find_by_id(&app_state.db_pool, task_attempt.task_id).await
                                {
                                    if let Err(e) = Task::update_status(
                                        &app_state.db_pool,
                                        task.id,
                                        task.project_id,
                                        TaskStatus::InReview,
                                    )
                                    .await
                                    {
                                        tracing::error!("Failed to update task status to InReview for orphaned attempt: {}", e);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Handle setup script completion
async fn handle_setup_completion(
    app_state: &AppState,
    task_attempt_id: Uuid,
    execution_process_id: Uuid,
    execution_process: ExecutionProcess,
    success: bool,
    exit_code: Option<i64>,
) {
    let exit_text = if let Some(code) = exit_code {
        format!(" with exit code {}", code)
    } else {
        String::new()
    };

    if success {
        // Mark setup as completed in database
        if let Err(e) = TaskAttempt::mark_setup_completed(&app_state.db_pool, task_attempt_id).await
        {
            tracing::error!(
                "Failed to mark setup as completed for attempt {}: {}",
                task_attempt_id,
                e
            );
        }

        // Setup completed successfully, create activity
        let activity_id = Uuid::new_v4();
        let create_activity = CreateTaskAttemptActivity {
            execution_process_id,
            status: Some(TaskAttemptStatus::SetupComplete),
            note: Some(format!("Setup script completed successfully{}", exit_text)),
        };

        if let Err(e) = TaskAttemptActivity::create(
            &app_state.db_pool,
            &create_activity,
            activity_id,
            TaskAttemptStatus::SetupComplete,
        )
        .await
        {
            tracing::error!("Failed to create setup complete activity: {}", e);
            return;
        }

        // Check for delegation context in process args
        let delegation_result = if let Some(args_json) = &execution_process.args {
            parse_delegation_context(args_json)
        } else {
            None
        };

        if let Some(delegation_context) = delegation_result {
            // Delegate to the original operation
            handle_setup_delegation(app_state, delegation_context).await;
        } else {
            // Fallback to original behavior - start coding agent
            if let Ok(Some(task_attempt)) =
                TaskAttempt::find_by_id(&app_state.db_pool, task_attempt_id).await
            {
                if let Ok(Some(task)) =
                    Task::find_by_id(&app_state.db_pool, task_attempt.task_id).await
                {
                    // Start the coding agent
                    if let Err(e) = ProcessService::start_coding_agent(
                        &app_state.db_pool,
                        app_state,
                        task_attempt_id,
                        task.id,
                        task.project_id,
                    )
                    .await
                    {
                        tracing::error!(
                            "Failed to start coding agent after setup completion: {}",
                            e
                        );
                    }
                }
            }
        }
    } else {
        // Setup failed, create activity and update task status
        let activity_id = Uuid::new_v4();
        let create_activity = CreateTaskAttemptActivity {
            execution_process_id,
            status: Some(TaskAttemptStatus::SetupFailed),
            note: Some(format!("Setup script failed{}", exit_text)),
        };

        if let Err(e) = TaskAttemptActivity::create(
            &app_state.db_pool,
            &create_activity,
            activity_id,
            TaskAttemptStatus::SetupFailed,
        )
        .await
        {
            tracing::error!("Failed to create setup failed activity: {}", e);
        }

        // Update task status to InReview since setup failed
        if let Ok(Some(task_attempt)) =
            TaskAttempt::find_by_id(&app_state.db_pool, task_attempt_id).await
        {
            if let Ok(Some(task)) = Task::find_by_id(&app_state.db_pool, task_attempt.task_id).await
            {
                if let Err(e) = Task::update_status(
                    &app_state.db_pool,
                    task.id,
                    task.project_id,
                    TaskStatus::InReview,
                )
                .await
                {
                    tracing::error!(
                        "Failed to update task status to InReview after setup failure: {}",
                        e
                    );
                }
            }
        }
    }
}

/// Handle coding agent completion
async fn handle_coding_agent_completion(
    app_state: &AppState,
    task_attempt_id: Uuid,
    execution_process_id: Uuid,
    execution_process: ExecutionProcess,
    success: bool,
    exit_code: Option<i64>,
) {
    let exit_text = if let Some(code) = exit_code {
        format!(" with exit code {}", code)
    } else {
        String::new()
    };

    // Extract and store assistant message from execution logs
    let _summary = if let Some(stdout) = &execution_process.stdout {
        if let Some(assistant_message) = crate::executor::parse_assistant_message_from_logs(stdout)
        {
            if let Err(e) = crate::models::executor_session::ExecutorSession::update_summary(
                &app_state.db_pool,
                execution_process_id,
                &assistant_message,
            )
            .await
            {
                tracing::error!(
                    "Failed to update summary for execution process {}: {}",
                    execution_process_id,
                    e
                );
                None
            } else {
                tracing::info!(
                    "Successfully stored summary for execution process {}",
                    execution_process_id
                );
                Some(assistant_message)
            }
        } else {
            None
        }
    } else {
        None
    };

    // Send notifications if enabled
    let sound_enabled = app_state.get_sound_alerts_enabled().await;
    let push_enabled = app_state.get_push_notifications_enabled().await;

    if sound_enabled || push_enabled {
        let sound_file = app_state.get_sound_file().await;
        let notification_config = NotificationConfig {
            sound_enabled,
            push_enabled,
        };

        let notification_service = NotificationService::new(notification_config);

        // Get task attempt and task details for richer notification
        let (notification_title, notification_message) = if let Ok(Some(task_attempt)) =
            TaskAttempt::find_by_id(&app_state.db_pool, task_attempt_id).await
        {
            if let Ok(Some(task)) = Task::find_by_id(&app_state.db_pool, task_attempt.task_id).await
            {
                let title = format!("Task Complete: {}", task.title);
                let message = if success {
                    format!(
                        "✅ '{}' completed successfully\nBranch: {}\nExecutor: {}",
                        task.title,
                        task_attempt.branch,
                        task_attempt.executor.as_deref().unwrap_or("default")
                    )
                } else {
                    format!(
                        "❌ '{}' execution failed\nBranch: {}\nExecutor: {}",
                        task.title,
                        task_attempt.branch,
                        task_attempt.executor.as_deref().unwrap_or("default")
                    )
                };
                (title, message)
            } else {
                // Fallback if task not found
                let title = "Task Complete";
                let message = if success {
                    "Task execution completed successfully"
                } else {
                    "Task execution failed"
                };
                (title.to_string(), message.to_string())
            }
        } else {
            // Fallback if task attempt not found
            let title = "Task Complete";
            let message = if success {
                "Task execution completed successfully"
            } else {
                "Task execution failed"
            };
            (title.to_string(), message.to_string())
        };

        notification_service
            .notify(&notification_title, &notification_message, &sound_file)
            .await;
    }

    // Get task attempt for status updates
    if let Ok(Some(task_attempt)) =
        TaskAttempt::find_by_id(&app_state.db_pool, task_attempt_id).await
    {
        // Create task attempt activity with appropriate completion status
        let activity_id = Uuid::new_v4();
        let status = if success {
            TaskAttemptStatus::ExecutorComplete
        } else {
            TaskAttemptStatus::ExecutorFailed
        };
        let create_activity = CreateTaskAttemptActivity {
            execution_process_id,
            status: Some(status.clone()),
            note: Some(format!("Coding agent execution completed{}", exit_text)),
        };

        if let Err(e) =
            TaskAttemptActivity::create(&app_state.db_pool, &create_activity, activity_id, status)
                .await
        {
            tracing::error!("Failed to create executor completion activity: {}", e);
        } else {
            tracing::info!(
                "Task attempt {} set to paused after coding agent completion",
                task_attempt_id
            );

            // Get task to access task_id and project_id for status update
            if let Ok(Some(task)) = Task::find_by_id(&app_state.db_pool, task_attempt.task_id).await
            {
                app_state
                    .track_analytics_event(
                        "task_attempt_finished",
                        Some(serde_json::json!({
                            "task_id": task.id.to_string(),
                            "project_id": task.project_id.to_string(),
                            "attempt_id": task_attempt_id.to_string(),
                            "execution_success": success,
                            "exit_code": exit_code,
                        })),
                    )
                    .await;

                // Update task status to InReview
                if let Err(e) = Task::update_status(
                    &app_state.db_pool,
                    task.id,
                    task.project_id,
                    TaskStatus::InReview,
                )
                .await
                {
                    tracing::error!(
                        "Failed to update task status to InReview for completed attempt: {}",
                        e
                    );
                }
            }
        }
    } else {
        tracing::error!(
            "Failed to find task attempt {} for coding agent completion",
            task_attempt_id
        );
    }
}

/// Handle dev server completion (future functionality)
async fn handle_dev_server_completion(
    app_state: &AppState,
    task_attempt_id: Uuid,
    execution_process_id: Uuid,
    _execution_process: ExecutionProcess,
    success: bool,
    exit_code: Option<i64>,
) {
    let exit_text = if let Some(code) = exit_code {
        format!(" with exit code {}", code)
    } else {
        String::new()
    };

    tracing::info!(
        "Dev server for task attempt {} completed{}",
        task_attempt_id,
        exit_text
    );

    // Update execution process status instead of creating activity
    let process_status = if success {
        ExecutionProcessStatus::Completed
    } else {
        ExecutionProcessStatus::Failed
    };

    if let Err(e) = ExecutionProcess::update_completion(
        &app_state.db_pool,
        execution_process_id,
        process_status,
        exit_code,
    )
    .await
    {
        tracing::error!(
            "Failed to update dev server execution process status: {}",
            e
        );
    }
}
