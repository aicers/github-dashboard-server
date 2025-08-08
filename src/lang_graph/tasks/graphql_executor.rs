use async_trait::async_trait;
use graph_flow::{NextAction, Task, TaskResult};
use tracing::info;

use crate::{database::Database, lang_graph::session_keys};
pub struct GraphQLExecutorTask {
    database: Database,
}
impl GraphQLExecutorTask {
    pub fn new(database: Database) -> Self {
        Self { database }
    }
}

#[async_trait]
impl Task for GraphQLExecutorTask {
    async fn run(&self, context: graph_flow::Context) -> graph_flow::Result<TaskResult> {
        let session_id = context
            .get::<String>("session_id")
            .await
            .unwrap_or_else(|| "unknown".to_string());

        info!("GraphQLExecutorTask started. Session: {}", session_id);
        let schema = crate::api::schema_origin(self.database.clone());

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

        let execution_result = schema.execute(&graphql_query).await;
        info!("{:?}", execution_result);

        if execution_result.is_err() {
            context.set(session_keys::GRAPHQL_EXECUTE_ERROR, true).await;
            context
                .set(
                    session_keys::GRAPHQL_RESULT,
                    execution_result
                        .errors
                        .iter()
                        .map(|e| e.message.clone())
                        .collect::<Vec<_>>()
                        .join(", "),
                )
                .await;
            return Ok(TaskResult::new(
                Some("GraphQL query Fail".to_string()),
                NextAction::Continue,
            ));
        }
        context
            .set(session_keys::GRAPHQL_EXECUTE_ERROR, false)
            .await;

        context
            .set(
                session_keys::GRAPHQL_RESULT,
                execution_result.data.to_string(),
            )
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
