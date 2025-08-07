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
    types::{
        query::Segment,
        response::{QualitativeResult, VectorSearchResult},
    },
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

        let reranked_contexts: Vec<(Segment, Vec<VectorSearchResult>)> = context
            .get_sync(session_keys::RERANKED_CONTEXTS)
            .ok_or_else(|| GraphError::ContextError("No reranked query found".to_string()))?;

        let mut segment_rag_responses = Vec::new();
        if reranked_contexts.is_empty() {
            context
                .set(session_keys::RAG_RESPONSE, segment_rag_responses.clone())
                .await;
            return Ok(TaskResult::new(
                Some("No relevant contexts found for RAG generation".to_string()),
                NextAction::Continue,
            ));
        }
        for (segment, reranked_result) in &reranked_contexts {
            info!("Reranked Segment: {}", segment.enhanced);
            let prompt = format!(
                "Analyze this GitHub repository query: {}, \n\nContext:\n{}",
                segment.enhanced,
                serde_json::to_string(reranked_result)
                    .unwrap_or_else(|_| "No context available".to_string())
            );
            info!(
                "{}",
                reranked_result
                    .iter()
                    .map(|r| format!(
                        "ID: {}, Score: {}, Content: {}, metadata: {}",
                        r.id, r.score, r.content, r.metadata
                    ))
                    .collect::<Vec<_>>()
                    .join("\n")
            );

            let response = self
                .agent
                .chat(&prompt, context.get_rig_messages().await)
                .await
                .map_err(|e| GraphError::TaskExecutionFailed(format!("LLM error: {e}")))?;
            info!("RAG generation response received");
            info!("RAGGenerationTask finished. Response: {}", response);
            let rag_response = QualitativeResult {
                segment_id: segment.id.clone(),
                generated_response: response,
                vector_search_results: reranked_result.clone(),
            };

            segment_rag_responses.push(rag_response);
        }

        context
            .add_assistant_message(format!(
                "RAG generation completed for {} segments",
                segment_rag_responses.len()
            ))
            .await;
        context
            .set(session_keys::RAG_RESPONSE, segment_rag_responses.clone())
            .await;
        info!("Context updated with RAG generation response");
        Ok(TaskResult::new(
            Some(format!(
                "RAG generation completed: {} segments, \nSegments: {} ",
                segment_rag_responses.len(),
                segment_rag_responses
                    .iter()
                    .map(|res| res.generated_response.clone())
                    .collect::<String>()
            )),
            NextAction::End,
        ))
    }
}
