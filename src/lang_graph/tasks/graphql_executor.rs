use async_trait::async_trait;
use graph_flow::{Context, GraphError, NextAction, Task, TaskResult};
use tracing::{debug, error, info, instrument, Span};

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
    #[instrument(
        name = "graphql_executor_task",
        skip(self, context),
        fields(session_id, graphql_query)
    )]
    async fn run(&self, context: Context) -> graph_flow::Result<TaskResult> {
        let session_id = context
            .get::<String>("session_id")
            .await
            .unwrap_or_else(|| "unknown".to_string());
        let schema = crate::api::schema_origin(self.database.clone());
        Span::current().record("session_id", &session_id);
        info!("Starting task");

        let graphql_query: String = context
            .get_sync(session_keys::GRAPHQL_QUERY)
            .unwrap_or_default();
        Span::current().record("graphql_query", &graphql_query);

        if graphql_query.is_empty() {
            info!("No GraphQL query to execute. Skipping.");
            context
                .set(session_keys::GRAPHQL_RESULT, serde_json::Value::Null)
                .await;
            context
                .set(session_keys::GRAPHQL_EXECUTE_ERROR, false)
                .await;
            return Ok(TaskResult::new(
                Some("No GraphQL query to execute".to_string()),
                NextAction::Continue,
            ));
        }

        let schema = crate::api::schema_origin(self.database.clone());
        let execution_result = schema.execute(&graphql_query).await;

        debug!(
            ?execution_result,
            "Received execution result from GraphQL schema"
        );

        if execution_result.is_err() {
            let error_messages: Vec<String> = execution_result
                .errors
                .iter()
                .map(|e| e.message.clone())
                .collect();

            error!(errors = ?error_messages, "GraphQL execution failed");

            context.set(session_keys::GRAPHQL_EXECUTE_ERROR, true).await;
            context
                .set(session_keys::GRAPHQL_RESULT, error_messages.join(", "))
                .await;

            return Ok(TaskResult::new(
                Some("GraphQL query execution failed".to_string()),
                NextAction::Continue,
            ));
        }

        info!("GraphQL execution successful");
        context
            .set(session_keys::GRAPHQL_EXECUTE_ERROR, false)
            .await;

        let result_json = serde_json::to_value(&execution_result.data).map_err(|e| {
            error!(error = ?e, "Failed to serialize successful GraphQL result to JSON");
            GraphError::TaskExecutionFailed(format!("Result serialization error: {}", e))
        })?;

        debug!(result = ?result_json, "Serialized execution data");

        context.set(session_keys::GRAPHQL_RESULT, result_json).await;
        context
            .add_assistant_message("Executed GraphQL query against GitHub API".to_string())
            .await;

        Ok(TaskResult::new(
            Some("GraphQL query executed successfully".to_string()),
            NextAction::Continue,
        ))
    }
}
