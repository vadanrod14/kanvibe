use anyhow::Result;
use reqwest::Client;
use serde_json::json;

use crate::models::{project::Project, task::Task};

pub struct AgentMarketClient {
    client: Client,
    base_url: String,
}

impl Default for AgentMarketClient {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentMarketClient {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            base_url: "https://api.agent.market".to_string(),
        }
    }

    /// Get the actual repository URL from a git repo path
    async fn get_repo_url(&self, repo_path: &str) -> Result<String, &'static str> {
        use tokio::process::Command;
        
        let output = Command::new("git")
            .args(&["remote", "get-url", "origin"])
            .current_dir(repo_path)
            .output()
            .await
            .map_err(|_| "Failed to execute git command")?;
            
        if !output.status.success() {
            return Err("Failed to get git remote URL");
        }
        
        let url = String::from_utf8(output.stdout)
            .map_err(|_| "Invalid UTF-8 in git remote URL")?
            .trim()
            .to_string();
            
        Ok(url)
    }

    pub async fn create_instance(
        &self,
        api_key: &str,
        task: &Task,
        project: &Project,
        max_reward: i32,
    ) -> Result<serde_json::Value> {
        let url = format!("{}/v1/instances", self.base_url);

        let max_credit_per_instance = max_reward as f64 / 100.0;

        // Get the actual repository URL
        let repo_url = match self.get_repo_url(&project.git_repo_path).await {
            Ok(url) => url,
            Err(_) => project.git_repo_path.clone(), // Fallback to the path if we can't get the URL
        };

        let payload = json!({
            "messages": [
                {
                    "role": "user",
                    "content": format!("Task: {}\n\nDescription: {}",
                        task.title,
                        task.description.as_deref().unwrap_or("No description provided")
                    )
                }
            ],
            "model": "gpt-4",
            "background": format!(
                r#"Repository URL: {}
Issue Title: {}
Issue URL: N/A (Agent Market instance)
Issue Number: N/A

Issue Description:
{}

Please analyze this issue and provide suggestions for resolution.
Make sure to include "Fixes #N/A" in your answer."#,
                repo_url,
                task.title,
                task.description.as_deref().unwrap_or("No description provided")
            ),
            "max_credit_per_instance": max_credit_per_instance,
            "percentage_reward": 0.5,
            "instance_timeout": 300,
            "gen_reward_timeout": 172800
        });

        let response = self
            .client
            .post(&url)
            .header("X-API-KEY", api_key)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;

        if response.status().is_success() {
            let result: serde_json::Value = response.json().await?;
            Ok(result)
        } else {
            let error_text = response.text().await?;
            Err(anyhow::anyhow!("Agent Market API error: {}", error_text))
        }
    }
}
