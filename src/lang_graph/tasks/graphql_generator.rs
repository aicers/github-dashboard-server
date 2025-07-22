use std::fs;

use async_trait::async_trait;
use graph_flow::{Context, GraphError, NextAction, Task, TaskResult};
use rig::{
    agent::Agent,
    client::CompletionClient,
    completion::Chat,
    providers::{self},
};
use serde_json::Value;
use tracing::info;
use tracing_subscriber::util;

use crate::lang_graph::{
    session_keys,
    types::query::{EnhancedQuery, ParsedSegment, QueryType, Segment},
    utils::pretty_log,
};
pub struct GraphQLGeneratorTask {
    agent: Agent<providers::ollama::CompletionModel>,
}

impl GraphQLGeneratorTask {
    pub fn new() -> Self {
        let client = providers::ollama::Client::new();
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let schema_doc = fs::read_to_string("src/lang_graph/schema.graphql").unwrap_or_else(|_| {
            info!("Failed to read schema.graphql, using empty schema");
            String::new()
        });
        let agent = client
            .agent("llama3.1:8b")
            .preamble(
                format!(
                    "You are a helpful assistant that translates natural language into GraphQL queries.\n\n\
                    There are some rules you must follow:\n\n\
                    - Return {{}} if the answer cannot be found in the schema.\n\n\
                    - Return a GraphQL query that answers the natural language query based on the schema.\n\n\
                    - Don't make up an answer if one cannot be found.\n\n\
                    - Don't use any queries that return a type ending in `Connection!`.\n\n\
                    - Don't explain the query, just return it.\n\n\
                    - If an answer is found, return it in the format query {{ ... }} or {{}}.\n\n\
                    - When you return a query, it should be a valid GraphQL query that can be executed against the schema.\n\n
                    - If the user query is unanswerable based on the schema, don't try to generate a query, just return empty query
                    - Today's date is {today}.\n\n\
                    - Timezone: UTC.\n\n\
                    Schema:\n{schema_doc}\n\n
                    "
                ).as_str()
            )
            .build();

        Self { agent }
    }
}

#[async_trait]
impl Task for GraphQLGeneratorTask {
    async fn run(&self, context: Context) -> graph_flow::Result<TaskResult> {
        let session_id = context
            .get::<String>("session_id")
            .await
            .unwrap_or_else(|| "unknown".to_string());

        info!("GraphQLGeneratorTask started. Session: {}", session_id);

        let enhanced_query: EnhancedQuery = context
            .get::<EnhancedQuery>(session_keys::ENHANCED_QUERY)
            .await
            .ok_or_else(|| GraphError::ContextError("No enhanced query found".to_string()))?;
        let segments: Vec<Segment> = enhanced_query
            .segments
            .into_iter()
            .filter(|segement| {
                matches!(
                    segement.query_type,
                    QueryType::Quantitative | QueryType::Qualitative
                )
            })
            .collect();

        if segments.is_empty() {
            return Ok(TaskResult::new(
                Some("No segments to convert to GraphQL".to_string()),
                NextAction::End,
            ));
        }

        let chat_history = context.get_rig_messages().await;

        let segments_json = serde_json::to_string_pretty(&segments).map_err(|e| {
            GraphError::TaskExecutionFailed(format!("Segments serialization error: {e}"))
        })?;

        let prompt = format!(
            "Generate a GitHub GraphQL query for these parsed segments:\n{segments_json}\n\nReturn only the GraphQL query string."
        );

        let graphql_query = self
            .agent
            .chat(&prompt, chat_history)
            .await
            .map_err(|e| GraphError::TaskExecutionFailed(format!("LLM error: {e}")))?;

        // GraphQL 쿼리 정리 (```graphql 태그 제거 등)
        let cleaned_query = graphql_query
            .replace("```graphql", "")
            .replace("```", "")
            .trim()
            .to_string();

        info!("Generated GraphQL query: {}", cleaned_query);

        context
            .set(session_keys::GRAPHQL_QUERY, cleaned_query.clone())
            .await;
        context
            .add_assistant_message("Generated GraphQL query from segments".to_string())
            .await;

        Ok(TaskResult::new(
            Some("GraphQL query generated successfully".to_string()),
            NextAction::End,
        ))
    }
}
