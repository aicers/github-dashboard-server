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
                r"You are a context reranking specialist. Your task is to reorder a given JSON array of documents based on their relevance to a user's query.
                You will receive a user query and a JSON array of documents. Your goal is to return the exact same array, with the documents rearranged from most relevant to least relevant.
                CRITICAL INSTRUCTIONS:
                - Your response MUST be the reordered JSON array of the original documents.
                - Do NOT add, remove, or modify any fields within the document objects.
                - Do NOT add new fields like a score. You are only changing the order of the objects.
                - Your output must be only the raw JSON array.
                - Do NOT wrap the response in triple backticks (```), markdown, or any other formatting.
                - Do NOT include any text, explanation, or commentary.
                To determine the best order, consider:
                - Direct Relevance: How closely the document's content matches the user query's intent.
                - Content Quality: How complete and informative the document is.
                - Recency: Newer content may be more relevant.
                - Source Authority: Official documentation or comments from project maintainers are generally more authoritative.
                Simply reorder the provided documents and output the resulting JSON array.
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

            let reranked = serde_json::from_str(&response).map_err(|e| {
                error!("Failed to parse reranked JSON: {}", e);
                GraphError::ContextError(format!("JSON parse error: {e}"))
            });

            match reranked {
                Ok(r) => {
                    reranked_segments.push((segment, r));
                }
                Err(e) => {
                    error!("{e}");
                    reranked_segments.push((segment, results));
                }
            }
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
            NextAction::ContinueAndExecute,
        ))
    }
}
