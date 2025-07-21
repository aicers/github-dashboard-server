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
    types::{query::EnhancedQuery, response::VectorSearchResult},
    utils::pretty_log,
};

pub struct RAGGenerationTask {
    agent: Agent<providers::ollama::CompletionModel>,
}

impl RAGGenerationTask {
    pub fn new() -> Self {
        let client = providers::ollama::Client::new();
        let agent = client
            .agent("llama3.1:8b")
            .preamble(
                r"You are a GitHub repository expert assistant. Generate comprehensive, accurate responses using the provided context from GitHub issues, PRs, discussions, and documentation.

                Guidelines:
                1. Use the provided context to answer questions accurately
                2. Cite specific issues, PRs, or discussions when relevant
                3. If the context doesn't contain enough information, acknowledge this
                4. Provide actionable insights and recommendations when possible
                5. Structure responses clearly with proper formatting
                6. Include relevant links or references when available in the context

                Always base your response on the provided context and clearly indicate when you're making inferences."
            )
            .build();

        Self { agent }
    }
}

#[async_trait]
impl Task for RAGGenerationTask {
    async fn run(&self, context: Context) -> graph_flow::Result<TaskResult> {
        let session_id = context
            .get::<String>("session_id")
            .await
            .unwrap_or_else(|| "unknown".to_string());

        info!("RAGGenerationTask started. Session: {}", session_id);

        let enhanced_query: EnhancedQuery = context
            .get_sync(session_keys::ENHANCED_QUERY)
            .ok_or_else(|| GraphError::ContextError("No enhanced query found".to_string()))?;

        let reranked_contexts: Vec<VectorSearchResult> = context
            .get_sync(session_keys::RERANKED_CONTEXTS)
            .ok_or_else(|| GraphError::ContextError("No reranked query found".to_string()))?;

        if reranked_contexts.is_empty() {
            return Ok(TaskResult::new(
                Some("No relevant contexts found for RAG generation".to_string()),
                NextAction::Continue,
            ));
        }
        info!(
            "RAGGenerationTask found {} relevant contexts for query: {}",
            reranked_contexts.len(),
            enhanced_query.original
        );

        let prompt = format!(
            "Analyze this GitHub repository query: {}, \n\nContext:\n{}",
            enhanced_query.original,
            reranked_contexts
                .iter()
                .map(|r| format!(
                    "ID: {}, Score: {}, Content: {}, metadata: {}",
                    r.id, r.score, r.content, r.metadata
                ))
                .collect::<Vec<_>>()
                .join("\n")
        );
        info!(
            "{}",
            reranked_contexts
                .iter()
                .map(|r| format!(
                    "ID: {}, Score: {}, Content: {}, metadata: {}",
                    r.id, r.score, r.content, r.metadata
                ))
                .collect::<Vec<_>>()
                .join("\n")
        );
        info!("Sending chat to LLM for RAG generation...");
        let response = self
            .agent
            .chat(&prompt, context.get_rig_messages().await)
            .await
            .map_err(|e| GraphError::TaskExecutionFailed(format!("LLM error: {e}")))?;
        info!("RAG generation response received");
        // pretty_log("RAGGenerationTask finished. Response:", &response);
        info!("RAGGenerationTask finished. Response: {}", &response);
        context.add_assistant_message(response.clone()).await;
        context
            .set(session_keys::RAG_RESPONSE, response.clone())
            .await;
        info!("Context updated with RAG generation response");
        Ok(TaskResult::new(
            Some(format!("RAG generation completed: {response}")),
            NextAction::End,
        ))
    }
}
