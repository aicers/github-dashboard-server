use std::fs;

use async_trait::async_trait;
use graph_flow::{Context, GraphError, NextAction, Task, TaskResult};
use rig::{
    agent::Agent,
    client::CompletionClient,
    completion::Chat,
    providers::{self},
};
use tracing::info;

use crate::lang_graph::{
    session_keys,
    types::query::{EnhancedQuery, Segment},
    utils::pretty_log,
};

pub struct QueryEnhancementTask {
    agent: Agent<providers::ollama::CompletionModel>,
}

impl QueryEnhancementTask {
    pub fn new() -> Self {
        let client = providers::ollama::Client::new();
        let schema_doc =
            fs::read_to_string("src/lang_graph/schema2.graphql").unwrap_or_else(|_| {
                info!("Failed to read schema.graphql, using empty schema");
                String::new()
            });

        let prompt = format!(
            r#"
            You are a JSON Generator in analyzing and enhancing user queries for GitHub repositories.
            Your task is to split complex queries into segments based on their intent:
            "Quantitative" (statistics), "Qualitative" (insights), or "Mixed" (both).
            Each segment should be enhanced to clarify the user's intent and identify relevant entities.

            For each segment:
            - If the segment is "Quantitative", it will be answered by generating and executing a GraphQL query based on the schema.
                - schema: {schema_doc}

            - If the segment is "Qualitative", relevant documents stored in a vector database will be retrieved by similarity search and used to generate the answer.
            - For "Mixed" segments, both approaches may be combined as appropriate.

            All segments' results will be integrated to form a comprehensive final answer.

            Format your response as a JSON array of segments.
            Each segment must include:
            - id: unique identifier for the segment
            - enhanced: enhanced version of the query
            - query_type: "Quantitative", "Qualitative", or "Mixed"
            - intent: brief description of what the user wants
            - entities: list of relevant entities

            Example:
            [
                {{
                    "id": "segment1",
                    "query_type": "Quantitative",
                    "enhanced": "Show me the number of commits in the last month",
                    "intent": "Get commit statistics",
                    "entities": ["commits", "last month"]
                }}
            ]

            Analyze this GitHub repository query: {{user_query}}
            If the query contains multiple intents (quantitative/statistics and qualitative/insights), split it into segments and return as a JSON array.

            No explanation. Always respond in JSON format.
            "#
        );

        let agent = client.agent("llama3.1:8b").preamble(&prompt).build();

        Self { agent }
    }
}

#[async_trait]
impl Task for QueryEnhancementTask {
    async fn run(&self, context: Context) -> graph_flow::Result<TaskResult> {
        let session_id = context
            .get::<String>("session_id")
            .await
            .unwrap_or_else(|| "unknown".to_string());

        info!("QueryEnhancementTask started. Session: {}", session_id);

        let user_query: String = context
            .get_sync(session_keys::USER_QUERY)
            .ok_or_else(|| GraphError::ContextError("No user query found".to_string()))?;

        let chat_history = context.get_rig_messages().await;

        let mut prompt = format!("Analyze this GitHub repository query: {user_query}");
        if let Some(message) = context.get::<String>("validation_message").await {
            prompt.push_str(&message);
        }

        let response = self
            .agent
            .chat(&prompt, chat_history)
            .await
            .map_err(|e| GraphError::ContextError(format!("LLM error: {e}")))?;

        let segments: Vec<Segment> = serde_json::from_str(&response)
            .map_err(|e| GraphError::TaskExecutionFailed(format!("JSON parse error: {e}")))?;

        pretty_log("QueryEnhancementTask finished. Segments:", &response);

        let enhanced_query = EnhancedQuery {
            original: user_query.clone(),
            segments: segments.clone(),
        };

        context
            .set(session_keys::ENHANCED_QUERY, enhanced_query.clone())
            .await;

        context.add_user_message(user_query).await;
        context
            .add_assistant_message(format!("Query analyzed. Segments:  {segments:?}"))
            .await;

        Ok(TaskResult::new(
            Some(format!("Query enhanced. Segments: {segments:?}")),
            NextAction::Continue,
        ))
    }
}
