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
    types::query::{EnhancedQuery, QueryType},
};

pub struct StatisticsResponseTask {
    agent: Agent<providers::ollama::CompletionModel>,
}

impl StatisticsResponseTask {
    pub fn new() -> Self {
        let client = providers::ollama::Client::new();
        let agent = client
            .agent("llama3.1:8b")
            .preamble(
                r"You are a statistics interpreter for GitHub data analysis.

                Given a user query that has been decomposed into multiple **semantic segments**, each with its own **enhanced interpretation**, you will be provided with:

                - The original user query
                - A list of semantic segments
                - For each segment:
                - The enhanced query (natural language)
                - The corresponding GraphQL query
                - The GraphQL response

                Your task is to generate a clear, user-friendly statistical summary that integrates the results across all segments.

                Focus on:
                1. Key metrics and numbers
                2. Trends and patterns
                3. Comparative insights across segments
                4. Clear explanations of what the data means
                5. Actionable insights when appropriate

                Always provide context for the numbers and explain their significance in relation to the original user intent.
                If possible, summarize insights in a way that helps decision-making or further investigation.",
            )
            .build();

        Self { agent }
    }
}

#[async_trait]
impl Task for StatisticsResponseTask {
    fn id(&self) -> &str {
        std::any::type_name::<Self>()
    }

    async fn run(&self, context: Context) -> graph_flow::Result<TaskResult> {
        let session_id = context
            .get::<String>("session_id")
            .await
            .unwrap_or_else(|| "unknown".to_string());
        info!("StatisticsResponseTask started. Session: {}", session_id);
        let enhanced_query: EnhancedQuery = context
            .get_sync(session_keys::ENHANCED_QUERY)
            .ok_or_else(|| GraphError::ContextError("No enhanced query found".to_string()))?;

        let enhanced_segements: Vec<String> = enhanced_query
            .segments
            .into_iter()
            .filter(|segment| {
                matches!(
                    segment.query_type,
                    QueryType::Quantitative | QueryType::Qualitative
                )
            })
            .map(|segement| segement.enhanced)
            .collect();

        let graphql_query: String = context
            .get_sync(session_keys::GRAPHQL_QUERY)
            .unwrap_or_default();

        let graphql_result: serde_json::Value = context
            .get_sync(session_keys::GRAPHQL_RESULT)
            .ok_or_else(|| GraphError::ContextError("No GraphQL results found".to_string()))?;

        if graphql_result.is_null() {
            context
                .set(
                    session_keys::STATISTICS_RESPONSE,
                    "No GraphQL results to analyze".to_string(),
                )
                .await;
            return Ok(TaskResult::new(
                Some("No GraphQL results to analyze".to_string()),
                NextAction::Continue,
            ));
        }

        let chat_history = context.get_rig_messages().await;

        let prompt = format!(
            "Analyze these GitHub GraphQL results for the user query:\n\"{}\"\n\n\
            Enhanced queries:\n{}\n\n\
            Generated GraphQL queries:\n{}\n\n\
            Corresponding GraphQL results:\n{}\n\n\
            Provide a clear, user-friendly statistical summary.",
            enhanced_query.original,
            enhanced_segements
                .iter()
                .map(|s| format!("- {s}"))
                .collect::<Vec<_>>()
                .join("\n"),
            graphql_query,
            serde_json::to_string_pretty(&graphql_result).unwrap()
        );

        let stats_response = self
            .agent
            .chat(&prompt, chat_history)
            .await
            .map_err(|e| GraphError::TaskExecutionFailed(format!("LLM error: {e}")))?;

        context
            .set(session_keys::STATISTICS_RESPONSE, stats_response.clone())
            .await;
        context
            .add_assistant_message(
                "Generated statistical analysis from GraphQL results".to_string(),
            )
            .await;

        info!(
            "StatisticsResponseTask finished. Response: {}",
            &stats_response
        );

        Ok(TaskResult::new(
            Some("Statistics response generated successfully".to_string()),
            NextAction::Continue,
        ))
    }
}
