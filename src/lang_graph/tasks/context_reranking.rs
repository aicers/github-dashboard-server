use async_trait::async_trait;
use graph_flow::{Context, GraphError, NextAction, Task, TaskResult};
use rig::client::CompletionClient;
use rig::completion::Chat;
use rig::{
    agent::Agent,
    providers::{self, ollama::CompletionModel},
};
use tracing::{error, info};

use crate::lang_graph::{
    session_keys,
    types::{query::Segment, response::VectorSearchResult},
};

pub struct ContextRerankTask {
    agent: Agent<CompletionModel>,
}

impl ContextRerankTask {
    pub fn new() -> Self {
        let client = providers::ollama::Client::new();
        let agent = client
            .agent("llama3.1:8b")
            .preamble(
                r"You are a context reranking specialist. Given a user query and a list of search results, rerank them by relevance.
                - Your response must be a raw JSON array of objects.
                - Do NOT wrap the response in triple backticks (```), markdown, or code block.
                - Do NOT include any text, explanation, or commentary.
                - Just output a valid JSON array as plain text.

                Consider:
                1. Direct relevance to the query
                2. Content quality and completeness
                3. Recency (newer content may be more relevant)
                4. Authority (official documentation, maintainer comments)

                Format your response as a JSON array of objects, each containing:

                - `id`: Unique identifier for the context
                - `score`: Relevance score (higher is better)
                - `content`: The content of the context
                - `metadata`: Additional metadata (e.g., source, date)
                Ensure the response is well-structured and easy to parse.
                "
            )
            .build();

        Self { agent }
    }
}

#[async_trait]
impl Task for ContextRerankTask {
    async fn run(&self, context: Context) -> graph_flow::Result<TaskResult> {
        let session_id = context
            .get::<String>("session_id")
            .await
            .unwrap_or_else(|| "unknown".to_string());
        info!("ContextRerankTask started. Session: {}", session_id);

        let segment_vector_results: Vec<(Segment, Vec<VectorSearchResult>)> = context
            .get(session_keys::VECTOR_SEARCH_RESULTS)
            .await
            .ok_or_else(|| GraphError::ContextError("No vector search results found".into()))?;

        if segment_vector_results.is_empty() {
            context
                .set(
                    session_keys::RERANKED_CONTEXTS,
                    Vec::<(Segment, Vec<VectorSearchResult>)>::new(),
                )
                .await;
            return Ok(TaskResult::new(
                Some("No vector search results found".to_string()),
                NextAction::Continue,
            ));
        }

        let mut reranked_segments = Vec::new();

        for (segment, results) in segment_vector_results {
            info!("Reranking for segment: {}", segment.enhanced);
            let chat_history = context.get_rig_messages().await;
            let prompt = format!(
                "Rerank the following contexts based on their relevance to the question: '{}'.\n\nContexts:\n{}",
                segment.enhanced,
                serde_json::to_string(&results).unwrap_or_default()
            );

            let response = self.agent.chat(&prompt, chat_history).await.map_err(|e| {
                error!("LLM error: {}", e);
                GraphError::ContextError(format!("LLM error: {e}"))
            })?;

            info!("Reranked response: {}", response);
            // pretty_log("Reranked Response", &response);

            let reranked: Vec<VectorSearchResult> =
                serde_json::from_str(&response).map_err(|e| {
                    error!("Failed to parse reranked JSON: {}", e);
                    GraphError::ContextError(format!("JSON parse error: {e}"))
                })?;

            reranked_segments.push((segment, reranked));
        }
        context
            .set(session_keys::RERANKED_CONTEXTS, reranked_segments.clone())
            .await;
        context
            .add_assistant_message(format!("Reranked {} segments.", reranked_segments.len()))
            .await;

        Ok(TaskResult::new(
            Some(format!(
                "Context reranking completed for {} segments.",
                reranked_segments.len()
            )),
            NextAction::Continue,
        ))
    }
}
