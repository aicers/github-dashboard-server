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
    types::{query::EnhancedQuery, response::QualitativeResult},
};

pub struct ResponseFormatterTask {
    agent: Agent<providers::ollama::CompletionModel>,
}

impl ResponseFormatterTask {
    pub fn new() -> Self {
        let client = providers::ollama::Client::new();
        let agent = client
            .agent("llama3.1:8b")
            .preamble(
                "
                You are an expert assistant that generates final answers by combining both quantitative and qualitative information.

                Given:
                - An enhanced user query that may contain quantitative, qualitative, or mixed intents.
                - RAG (Retrieval-Augmented Generation) qualitative results from vector search.
                - Quantitative statistics as a JSON string.

                Instructions:
                - For queries that are purely quantitative, produce a concise JSON summary with relevant data.
                - For purely qualitative queries, generate a coherent, well-structured natural language answer.
                - For mixed queries, provide a combined response that includes both JSON data and a narrative explanation.
                - Always ensure the response is clear, accurate, and tailored to the user's original question.
                - Avoid unnecessary technical jargon or formatting unless requested.
                - If information is missing, acknowledge it gracefully.

                Respond only with the formatted answer without extra commentary.

                ",
            )
            .build();

        Self { agent }
    }
}

#[async_trait]
impl Task for ResponseFormatterTask {
    fn id(&self) -> &'static str {
        "ResponseFormatterTask"
    }

    async fn run(&self, context: Context) -> graph_flow::Result<TaskResult> {
        let session_id = context
            .get::<String>("session_id")
            .await
            .unwrap_or_else(|| "unknown".to_string());

        info!("ResponseFormatterTask started. Session: {}", session_id);

        let enchanded_query: EnhancedQuery = context
            .get::<EnhancedQuery>(session_keys::ENHANCED_QUERY)
            .await
            .ok_or_else(|| GraphError::ContextError("No enhanced query found".to_string()))?;

        let rag_response: Vec<QualitativeResult> = context
            .get(session_keys::RAG_RESPONSE)
            .await
            .ok_or_else(|| GraphError::ContextError("No RAG response found".to_string()))?;

        let statistics_response: String = context
            .get::<String>(session_keys::STATISTICS_RESPONSE)
            .await
            .unwrap_or_else(|| "No statistics available".to_string());

        let prompt = format!(
            "You are a response formatter. Given the following enhanced query and RAG response, format the response according to the query type.\n\n\
            Enhanced Query: {}\n\n\
            RAG Response: {:?}\n\n\
            Statistics Response: {}\n\n\
            Format your response as follows:\n\
            - For Quantitative queries, return a JSON object with relevant data.\n\
            - For Qualitative queries, return a well-structured text response.\n\
            - For Mixed queries, combine both formats appropriately.",
            enchanded_query.original,
            serde_json::to_string(&rag_response),
            statistics_response
        );

        let chat_history = context.get_rig_messages().await;
        let response =
            self.agent.chat(&prompt, chat_history).await.map_err(|_| {
                GraphError::TaskExecutionFailed("Chat completion failed".to_string())
            })?;

        info!("ResponseFormatterTask finished. Response: {}", response);

        Ok(TaskResult::new(Some(response), NextAction::End))
    }
}
