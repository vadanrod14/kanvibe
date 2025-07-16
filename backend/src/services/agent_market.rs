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

    pub async fn create_instance(
        &self,
        api_key: &str,
        task: &Task,
        project: &Project,
        max_reward: i32,
    ) -> Result<serde_json::Value> {
        let url = format!("{}/v1/instances", self.base_url);

        let max_credit_per_instance = max_reward as f64 / 100.0;

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
            "background": format!("Project: {}\nRepository: {}",
                project.name,
                project.git_repo_path
            ),
            "max_credit_per_instance": max_credit_per_instance,
            "percentage_reward": 0.5,
            "instance_timeout": 3600,
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
