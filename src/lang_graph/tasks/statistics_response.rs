use async_trait::async_trait;
use graph_flow::{Context, GraphError, NextAction, Task, TaskResult};
use rig::{
    agent::Agent,
    client::CompletionClient,
    completion::Chat,
    providers::{self},
};
use tracing::{debug, error, info, instrument, Span};

use crate::lang_graph::{
    session_keys,
    types::query::{EnhancedQuery, Segment},
};

pub struct StatisticsResponseTask {
    agent: Agent<providers::ollama::CompletionModel>,
}

impl StatisticsResponseTask {
    pub fn new(model: &str) -> Self {
        let client = providers::ollama::Client::new();
        let agent = client
            .agent(model)
            .preamble(
                r"You are a statistics interpreter for GitHub data analysis.

                Given a user query that has been decomposed into multiple **semantic segments**, each with its own **enhanced interpretation**, you will be provided with:

                - The original user query
                - A list of semantic segments
                - The corresponding GraphQL query and its response

                Your task is to generate a clear, user-friendly statistical summary that integrates the results.

                Focus on:
                1. Key metrics and numbers from the GraphQL response.
                2. Trends and patterns revealed by the data.
                3. Clear explanations of what the data means in the context of the user's query.
                4. Actionable insights when appropriate.

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
        "StatisticsResponseTask"
    }

    #[instrument(
        name = "statistics_response_task",
        skip(self, context),
        fields(session_id)
    )]
    async fn run(&self, context: Context) -> graph_flow::Result<TaskResult> {
        let session_id = context
            .get::<String>("session_id")
            .await
            .unwrap_or_else(|| "unknown".to_string());
        Span::current().record("session_id", &session_id);
        info!("Starting task");

        let graphql_result: serde_json::Value = context
            .get_sync(session_keys::GRAPHQL_RESULT)
            .ok_or_else(|| GraphError::ContextError("No GraphQL results found".to_string()))?;

        if graphql_result.is_null() {
            info!("No GraphQL results to analyze. Skipping.");
            context
                .set(session_keys::STATISTICS_RESPONSE, String::new())
                .await;
            return Ok(TaskResult::new(
                Some("No GraphQL results to analyze".to_string()),
                NextAction::Continue,
            ));
        }

        let enhanced_query: EnhancedQuery = context
            .get_sync(session_keys::ENHANCED_QUERY)
            .ok_or_else(|| GraphError::ContextError("No enhanced query found".to_string()))?;

        let graphql_query: String = context
            .get_sync(session_keys::GRAPHQL_QUERY)
            .unwrap_or_default();

        let qualitative_segments: Vec<Segment> = context
            .get(session_keys::QUALITATIVE_SEGMENTS)
            .await
            .ok_or_else(|| GraphError::ContextError("No segments found".to_string()))?;

        let prompt = self.build_prompt(
            &enhanced_query,
            &qualitative_segments,
            &graphql_query,
            &graphql_result,
        )?;
        debug!(%prompt, "Generated prompt for statistics generation");

        let chat_history = context.get_rig_messages().await;
        let stats_response = self.agent.chat(&prompt, chat_history).await.map_err(|e| {
            error!(error = ?e, "LLM call for statistics generation failed");
            GraphError::TaskExecutionFailed(format!("LLM error: {e}"))
        })?;

        debug!(%stats_response, "Received raw statistics response from LLM");
        info!("Successfully generated statistical summary.");

        context
            .set(session_keys::STATISTICS_RESPONSE, stats_response.clone())
            .await;
        context
            .add_assistant_message(
                "Generated statistical analysis from GraphQL results".to_string(),
            )
            .await;

        Ok(TaskResult::new(
            Some("Statistics response generated successfully".to_string()),
            NextAction::Continue,
        ))
    }
}

impl StatisticsResponseTask {
    fn build_prompt(
        &self,
        enhanced_query: &EnhancedQuery,
        segments: &Vec<Segment>,
        graphql_query: &str,
        graphql_result: &serde_json::Value,
    ) -> graph_flow::Result<String> {
        let quantitative_segments: Vec<String> = segments
            .iter()
            .map(|segment| segment.enhanced.clone())
            .collect();

        let graphql_result_str = serde_json::to_string_pretty(graphql_result).map_err(|e| {
            error!(error = ?e, "Failed to serialize GraphQL result for prompt");
            GraphError::TaskExecutionFailed(format!("GraphQL result serialization error: {e}"))
        })?;

        let prompt = format!(
            "Analyze these GitHub GraphQL results in the context of the user's original query.\n\n\
            ORIGINAL USER QUERY: \"{}\"\n\n\
            STATISTICAL QUESTIONS (derived from the original query):\n{}\n\n\
            EXECUTED GRAPHQL QUERY:\n{}\n\n\
            GRAPHQL RESULT (JSON):\n{}\n\n\
            Based on all the above, provide a clear, user-friendly statistical summary.",
            enhanced_query.original,
            quantitative_segments
                .iter()
                .map(|s| format!("- {s}"))
                .collect::<Vec<_>>()
                .join("\n"),
            graphql_query,
            graphql_result_str
        );

        Ok(prompt)
    }
}
