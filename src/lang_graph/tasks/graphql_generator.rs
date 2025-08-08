use std::fs;

use async_trait::async_trait;
use graph_flow::{Context, GraphError, NextAction, Task, TaskResult};
use jiff::Zoned;
use rig::{
    agent::Agent,
    client::CompletionClient,
    completion::Chat,
    providers::{self},
};
use tracing::info;

use crate::lang_graph::{session_keys, types::query::Segment};

pub struct GraphQLGeneratorTask {
    agent: Agent<providers::ollama::CompletionModel>,
}

impl GraphQLGeneratorTask {
    pub fn new() -> Self {
        let client = providers::ollama::Client::new();
        let today = Zoned::now().to_string();
        let schema_doc =
            fs::read_to_string("src/lang_graph/schema2.graphql").unwrap_or_else(|_| {
                info!("Failed to read schema.graphql, using empty schema");
                String::new()
            });

        let agent = client
        .agent("llama3.1:8b")
        .preamble(&format!(
            r#"You are a helpful assistant that translates natural language into GraphQL queries.
            You MUST strictly adhere to the provided schema.

            **CRITICAL RULES:**
            1.  **Use Double Quotes Only:** All strings in the GraphQL query (field names and values) MUST use double quotes (`"`). Single quotes (`'`) are strictly forbidden.
            2.  **Strict Schema Adherence:** You MUST NOT use any fields or arguments not explicitly defined in the provided `Schema`. If a user's request cannot be fulfilled, you MUST return {{}}
            3.  **JSON Only:** Return a single GraphQL query string or an empty JSON object {{}}
            4.  **No Explanations:** Do not use prose, markdown, or any text outside the query string.
            5.  **No Connections:** Do not use any queries that return a type ending in `Connection!`.

            **EXAMPLES:**
            - **User Intent:** 'How many open refactor issues are there in github-dashboard-server?'
            - **Analysis:** The user wants to filter `issueStat` by a label ('refactor'). However, the `IssueStatFilter` input type in the schema does NOT have a `labels` field. Therefore, this query cannot be fulfilled.
            - **Correct Output:** {{}}

            - **User Intent:** 'Show me statistics for issues in the github-dashboard-server repo.'
            - **Analysis:** This can be fulfilled using the `repo` field in `IssueStatFilter`.
            - **Correct Output:** `query {{ issueStat(filter: {{ repo: 'github-dashboard-server' }}) {{ openIssueCount }} }}`

            ---
            Today's date is {today}.
            Timezone: UTC.

            Schema:
            {schema_doc}"#,
            ))
        .build();

        Self { agent }
    }
}

#[async_trait]
impl Task for GraphQLGeneratorTask {
    fn id(&self) -> &'static str {
        "GraphQLGeneratorTask"
    }

    async fn run(&self, context: Context) -> graph_flow::Result<TaskResult> {
        let session_id = context
            .get::<String>("session_id")
            .await
            .unwrap_or_else(|| "unknown".to_string());

        info!("GraphQLGeneratorTask started. Session: {}", session_id);

        let segments: Vec<Segment> = context
            .get_sync(session_keys::QUANTITATIVE_SEGMENTS)
            .ok_or_else(|| {
                GraphError::ContextError("No quantitative segments found".to_string())
            })?;

        if segments.is_empty() {
            context
                .set(session_keys::GRAPHQL_QUERY, String::new())
                .await;
            return Ok(TaskResult::new(
                Some("No segments to convert to GraphQL".to_string()),
                NextAction::Continue,
            ));
        }

        let segments_json = serde_json::to_string_pretty(&segments).map_err(|e| {
            GraphError::TaskExecutionFailed(format!("Segment serialization error: {e}"))
        })?;

        let prompt = format!(
            "Below are multiple parsed segments representing parts of a user's natural language question:\n\
            {segments_json}\n\n\
            Generate a **single valid GraphQL query** that includes as many of these segments as possible. \
            If some segments are not answerable based on the schema, omit them.\n\n\
            Only return the final GraphQL query. Do not explain."
        );

        let chat_history = context.get_rig_messages().await;

        let graphql_query = self
            .agent
            .chat(&prompt, chat_history)
            .await
            .map_err(|e| GraphError::TaskExecutionFailed(format!("LLM error: {e}")))?;

        let cleaned_query = graphql_query
            .replace("```graphql", "")
            .replace("```", "")
            .trim()
            .to_string();

        if cleaned_query == "{}" {
            return Ok(TaskResult::new(
                Some("No valid GraphQL query could be generated.".to_string()),
                NextAction::End,
            ));
        }

        info!("Generated GraphQL Query : {}", cleaned_query);

        context
            .set(session_keys::GRAPHQL_QUERY, cleaned_query.clone())
            .await;

        context
            .add_assistant_message(
                "Generated a single GraphQL query from all segments.".to_string(),
            )
            .await;

        Ok(TaskResult::new(
            Some("GraphQL query generated successfully.".to_string()),
            NextAction::Continue,
        ))
    }
}
