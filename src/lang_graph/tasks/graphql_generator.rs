use async_trait::async_trait;
use graph_flow::{Context, NextAction, Task, TaskResult};
use rig_core::{agent::Agent, providers::openai};

use crate::session_keys;
use crate::types::query::{ParsedSegment, QueryType};

pub struct GraphQLGeneratorTask {
    agent: Agent,
}

impl GraphQLGeneratorTask {
    pub fn new() -> Self {
        let client = openai::Client::from_env();
        let agent = client
            .agent("gpt-4")
            .preamble(
                r#"You are a GitHub GraphQL query generator. Generate valid GitHub GraphQL queries based on parsed segments.

GitHub GraphQL schema knowledge:
- Use `repository(owner: "owner", name: "repo")` for repo queries
- For issues: `issues(first: 100, states: [OPEN, CLOSED], orderBy: {field: CREATED_AT, direction: DESC})`
- For PRs: `pullRequests(first: 100, states: [OPEN, CLOSED, MERGED])`
- For commits: `object(expression: "main") { ... on Commit { history { nodes { ... } } } }`
- Use date filtering with `createdAt` ranges
- Use label filtering with `labels(first: 10) { nodes { name } }`

Always generate complete, executable GraphQL queries with proper pagination and error handling."#
            )
            .build();

        Self { agent }
    }
}

#[async_trait]
impl Task for GraphQLGeneratorTask {
    async fn run(&self, context: Context) -> graph_flow::Result<TaskResult> {
        let query_type: QueryType = context
            .get_sync(session_keys::QUERY_TYPE)
            .ok_or_else(|| graph_flow::Error::custom("No query type found"))?;

        // 정량적 쿼리가 아니면 스킵
        if !matches!(query_type, QueryType::Quantitative) {
            return Ok(TaskResult::new(
                Some("Skipping GraphQL generation for qualitative query".to_string()),
                NextAction::Continue,
            ));
        }

        let segments: Vec<ParsedSegment> = context
            .get_sync(session_keys::PARSED_SEGMENTS)
            .unwrap_or_default();

        if segments.is_empty() {
            return Ok(TaskResult::new(
                Some("No segments to convert to GraphQL".to_string()),
                NextAction::Continue,
            ));
        }

        let chat_history = context.get_rig_messages().await;

        let segments_json = serde_json::to_string_pretty(&segments).map_err(|e| {
            graph_flow::Error::custom(format!("Segments serialization error: {}", e))
        })?;

        let prompt = format!(
            "Generate a GitHub GraphQL query for these parsed segments:\n{}\n\nReturn only the GraphQL query string.",
            segments_json
        );

        let graphql_query = self
            .agent
            .chat(&prompt, chat_history)
            .await
            .map_err(|e| graph_flow::Error::custom(format!("LLM error: {}", e)))?;

        // GraphQL 쿼리 정리 (```graphql 태그 제거 등)
        let cleaned_query = graphql_query
            .replace("```graphql", "")
            .replace("```", "")
            .trim()
            .to_string();

        context
            .set(session_keys::GRAPHQL_QUERY, cleaned_query.clone())
            .await;
        context
            .add_assistant_message("Generated GraphQL query from segments".to_string())
            .await;

        Ok(TaskResult::new(
            Some("GraphQL query generated successfully".to_string()),
            NextAction::Continue,
        ))
    }
}
