use async_trait::async_trait;
use graph_flow::{Context, NextAction, Task, TaskResult};
use rig_core::{agent::Agent, providers::openai};

use crate::session_keys;
use crate::types::query::QueryType;

pub struct StatisticsResponseTask {
    agent: Agent,
}

impl StatisticsResponseTask {
    pub fn new() -> Self {
        let client = openai::Client::from_env();
        let agent = client
            .agent("gpt-4")
            .preamble(
                r#"You are a statistics interpreter for GitHub data analysis.
Given GraphQL query results, generate clear, user-friendly statistical summaries.

Focus on:
1. Key metrics and numbers
2. Trends and patterns
3. Comparative insights
4. Clear explanations of what the data means
5. Actionable insights when possible

Always provide context for the numbers and explain their significance."#,
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
        let query_type: QueryType = context
            .get_sync(session_keys::QUERY_TYPE)
            .ok_or_else(|| graph_flow::Error::custom("No query type found"))?;

        // 정량적 쿼리가 아니면 스킵
        if !matches!(query_type, QueryType::Quantitative) {
            return Ok(TaskResult::new(
                Some("Skipping statistics response for qualitative query".to_string()),
                NextAction::Continue,
            ));
        }

        let graphql_result: serde_json::Value = context
            .get_sync(session_keys::GRAPHQL_RESULT)
            .unwrap_or_default();

        if graphql_result.is_null() {
            return Ok(TaskResult::new(
                Some("No GraphQL results to analyze".to_string()),
                NextAction::Continue,
            ));
        }

        let user_query: String = context
            .get_sync(session_keys::USER_QUERY)
            .unwrap_or_default();

        let chat_history = context.get_rig_messages().await;

        let prompt = format!(
            "Analyze these GitHub GraphQL results for the user query: '{}'\n\nResults:\n{}\n\nProvide a clear, user-friendly statistical summary.",
            user_query,
            serde_json::to_string_pretty(&graphql_result).unwrap()
        );

        let stats_response = self
            .agent
            .chat(&prompt, chat_history)
            .await
            .map_err(|e| graph_flow::Error::custom(format!("LLM error: {}", e)))?;

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
