use async_trait::async_trait;
use graph_flow::{Context, NextAction, Task, TaskResult};
use tracing::info;

use crate::lang_graph::session_keys;
pub struct GraphQLExecutorTask;

impl GraphQLExecutorTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task for GraphQLExecutorTask {
    async fn run(&self, context: Context) -> graph_flow::Result<TaskResult> {
        let session_id = context
            .get::<String>("session_id")
            .await
            .unwrap_or_else(|| "unknown".to_string());

        info!("GraphQLExecutorTask started. Session: {}", session_id);

        let graphql_query: String = context
            .get_sync(session_keys::GRAPHQL_QUERY)
            .unwrap_or_default();

        if graphql_query.is_empty() {
            context
                .set(session_keys::GRAPHQL_RESULT, serde_json::Value::Null)
                .await;
            return Ok(TaskResult::new(
                Some("No GraphQL query to execute".to_string()),
                NextAction::Continue,
            ));
        }

        // TODO: 실제 GitHub GraphQL API 호출 로직
        // 현재는 모의 실행
        let mock_result = serde_json::json!({
            "data": {
                "repository": {
                    "issues": {
                        "totalCount": 42,
                        "nodes": []
                    }
                }
            }
        });

        context
            .set(session_keys::GRAPHQL_RESULT, mock_result.clone())
            .await;
        context
            .add_assistant_message("Executed GraphQL query against GitHub API".to_string())
            .await;

        Ok(TaskResult::new(
            Some("GraphQL query executed successfully".to_string()),
            NextAction::Continue,
        ))
    }
}
