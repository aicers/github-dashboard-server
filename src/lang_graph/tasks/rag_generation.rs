use async_trait::async_trait;
use futures::future::join_all;
use graph_flow::{Context, GraphError, NextAction, Task, TaskResult};
use rig::{
    agent::Agent,
    client::CompletionClient,
    completion::{Chat, Message},
    providers::{self},
};
use tracing::{debug, error, info, instrument, Span};

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
    pub fn new(model: &str) -> Self {
        let client = providers::ollama::Client::new();
        let agent = client
            .agent(model)
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
    #[instrument(name = "rag_generation_task", skip(self, context), fields(session_id))]
    async fn run(&self, context: Context) -> graph_flow::Result<TaskResult> {
        let session_id = context
            .get::<String>("session_id")
            .await
            .unwrap_or_else(|| "unknown".to_string());
        Span::current().record("session_id", &session_id);
        info!("Starting task");

        let reranked_contexts: Vec<(Segment, Vec<VectorSearchResult>)> = context
            .get_sync(session_keys::RERANKED_CONTEXTS)
            .ok_or_else(|| GraphError::ContextError("No reranked contexts found".to_string()))?;

        if reranked_contexts.is_empty() {
            info!("No reranked contexts to process. Skipping.");
            context
                .set(session_keys::RAG_RESPONSE, Vec::<Segment>::new())
                .await;
            return Ok(TaskResult::new(
                Some("No relevant contexts found for RAG generation".to_string()),
                NextAction::Continue,
            ));
        }

        let chat_history = context.get_rig_messages().await;

        let futures = reranked_contexts.iter().map(|(segment, results)| {
            self.generate_for_segment(chat_history.clone(), segment, results)
        });

        let segment_rag_responses: Vec<QualitativeResult> =
            join_all(futures).await.into_iter().flatten().collect();

        info!(
            successful_generations = segment_rag_responses.len(),
            total_segments = reranked_contexts.len(),
            "Finished all RAG generations."
        );

        context
            .add_assistant_message(format!(
                "RAG generation completed for {} segments",
                segment_rag_responses.len()
            ))
            .await;
        context
            .set(session_keys::RAG_RESPONSE, segment_rag_responses.clone())
            .await;

        Ok(TaskResult::new(
            Some(format!(
                "RAG generation completed for {} segments.",
                segment_rag_responses.len()
            )),
            NextAction::Continue,
        ))
    }
}

impl RAGGenerationTask {
    #[instrument(
        name = "generate_for_segment",
        skip(self, chat_history, reranked_result),
        fields(segment_id = %segment.id)
    )]
    async fn generate_for_segment(
        &self,
        chat_history: Vec<Message>,
        segment: &Segment,
        reranked_result: &[VectorSearchResult],
    ) -> Option<QualitativeResult> {
        let formatted_context = self.format_contexts_for_prompt(reranked_result);
        if formatted_context.is_empty() {
            info!("No context to generate from. Skipping segment.");
            return None;
        }

        let prompt = format!(
            "Based on the following context, please answer the user's query.\n\nUSER QUERY: \"{}\"\n\nCONTEXT:\n{}",
            segment.enhanced,
            formatted_context
        );
        debug!(%prompt, "Sending generation prompt to LLM");

        match self.agent.chat(&prompt, chat_history).await {
            Ok(response) => {
                info!("Successfully generated response for segment.");
                debug!(%response, "Generated RAG response");
                Some(QualitativeResult {
                    segment_id: segment.id.clone(),
                    generated_response: response,
                    vector_search_results: reranked_result.to_vec(),
                })
            }
            Err(e) => {
                error!(error = ?e, "LLM call for RAG generation failed.");
                None
            }
        }
    }

    fn format_contexts_for_prompt(&self, results: &[VectorSearchResult]) -> String {
        results
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let source = r
                    .metadata
                    .get("source")
                    .and_then(|v| v.as_str())
                    .unwrap_or("N/A");
                format!(
                    "--- Context {} (Source: {}, Score: {:.2}) ---\n{}\n",
                    i + 1,
                    source,
                    r.score,
                    r.content
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
