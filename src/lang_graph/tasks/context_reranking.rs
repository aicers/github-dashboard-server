use async_trait::async_trait;
use graph_flow::{Context, GraphError, NextAction, Task, TaskResult};
use rig::client::CompletionClient;
use rig::completion::{Chat, Message};
use rig::{
    agent::Agent,
    providers::{self, ollama::CompletionModel},
};
use tracing::{debug, error, info, instrument, warn, Span};

use crate::lang_graph::{
    session_keys,
    types::{query::Segment, response::VectorSearchResult},
};

pub struct ContextRerankTask {
    agent: Agent<CompletionModel>,
}

impl ContextRerankTask {
    pub fn new(model: &str) -> Self {
        let client = providers::ollama::Client::new();
        let agent = client
            .agent(model)
            .preamble(
                r#"You are a context reranking specialist. Your ONLY task is to reorder a given JSON array of documents based on their relevance to a user's query.

                You will be given a `USER_QUERY` and a JSON array of `DOCUMENTS_TO_RERANK`.
                Your goal is to return the exact same array, with the documents rearranged from most relevant to least relevant.

                **CRITICAL INSTRUCTIONS:**
                1.  **OUTPUT MUST BE A VALID JSON ARRAY**: Your entire response must be ONLY the reordered JSON array.
                2.  **DO NOT MODIFY CONTENT**: Do not add, remove, or change any fields or values within the document objects. Only change the order.
                3.  **NO EXTRA TEXT**: Do not include explanations, apologies, commentary, or markdown formatting like ```json.

                **Reorder the provided documents and output nothing but the raw, reordered JSON array.**
                "#,
            )
            .build();

        Self { agent }
    }
}

#[async_trait]
impl Task for ContextRerankTask {
    #[instrument(name = "context_rerank_task", skip(self, context), fields(session_id))]
    async fn run(&self, context: Context) -> graph_flow::Result<TaskResult> {
        let session_id = context
            .get::<String>("session_id")
            .await
            .unwrap_or_else(|| "unknown".to_string());
        Span::current().record("session_id", &session_id);
        info!("Starting task");

        let segment_vector_results: Vec<(Segment, Vec<VectorSearchResult>)> = context
            .get(session_keys::VECTOR_SEARCH_RESULTS)
            .await
            .ok_or_else(|| GraphError::ContextError("No vector search results found".into()))?;

        if segment_vector_results.is_empty() {
            info!("No vector search results to rerank. Skipping.");
            context
                .set(session_keys::RERANKED_CONTEXTS, Vec::<Segment>::new())
                .await;
            return Ok(TaskResult::new(
                Some("No vector search results found".to_string()),
                NextAction::Continue,
            ));
        }

        let mut reranked_segments = Vec::new();
        let chat_history = context.get_rig_messages().await;

        for (segment, results) in segment_vector_results {
            let reranked_results = self
                .rerank_segment(chat_history.clone(), &segment, results)
                .await;
            reranked_segments.push((segment, reranked_results));
        }

        info!(
            reranked_segment_count = reranked_segments.len(),
            "Finished reranking all segments."
        );

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

impl ContextRerankTask {
    #[instrument(
        name = "rerank_segment",
        skip(self, chat_history, segment, results),
        fields(segment_id = %segment.id)
    )]
    async fn rerank_segment(
        &self,
        chat_history: Vec<Message>,
        segment: &Segment,
        results: Vec<VectorSearchResult>,
    ) -> Vec<VectorSearchResult> {
        if results.is_empty() {
            info!("Segment has no results to rerank.");
            return results;
        }

        let prompt = format!(
            "USER_QUERY: \"{}\"\n\nDOCUMENTS_TO_RERANK:\n{}",
            segment.enhanced,
            serde_json::to_string(&results).unwrap_or_default()
        );
        debug!(%prompt, "Sending rerank prompt to LLM");

        let response = match self.agent.chat(&prompt, chat_history).await {
            Ok(res) => res,
            Err(e) => {
                error!(error = ?e, "LLM call for reranking failed. Returning original order.");
                return results;
            }
        };
        debug!(raw_response = %response, "Received raw rerank response");

        let cleaned_json_str = self.extract_json_array_from_response(&response);

        match serde_json::from_str(cleaned_json_str) {
            Ok(reranked) => {
                info!("Successfully reranked and parsed documents.");
                reranked
            }
            Err(e) => {
                warn!(error = ?e, raw_response = %response, "Failed to parse reranked JSON. Returning original order.");
                results
            }
        }
    }

    fn extract_json_array_from_response<'a>(&self, response: &'a str) -> &'a str {
        if let Some(start) = response.find('[') {
            if let Some(end) = response.rfind(']') {
                if end > start {
                    return &response[start..=end];
                }
            }
        }
        response
    }
}
