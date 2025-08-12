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
use tracing::{debug, error, info, instrument, Span};

use crate::lang_graph::{session_keys, types::query::Segment};

pub struct GraphQLGeneratorTask {
    agent: Agent<providers::ollama::CompletionModel>,
}

impl GraphQLGeneratorTask {
    pub fn new(model: &str) -> Self {
        let client = providers::ollama::Client::new();
        let today = Zoned::now().to_string();
        let schema_doc =
            fs::read_to_string("src/lang_graph/schema2.graphql").unwrap_or_else(|_| {
                info!("Failed to read schema.graphql, using empty schema");
                String::new()
            });

        let agent = client
        .agent(model)
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

    #[instrument(
        name = "graphql_generator_task",
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

        let segments: Vec<Segment> = context
            .get_sync(session_keys::QUANTITATIVE_SEGMENTS)
            .ok_or_else(|| {
                GraphError::ContextError("No quantitative segments found".to_string())
            })?;

        if segments.is_empty() {
            info!("No quantitative segments to process. Skipping.");
            context
                .set(session_keys::GRAPHQL_QUERY, String::new())
                .await;
            return Ok(TaskResult::new(
                Some("No segments to convert to GraphQL".to_string()),
                NextAction::Continue,
            ));
        }

        let is_retry = context
            .get::<bool>(session_keys::GRAPHQL_EXECUTE_ERROR)
            .await
            .unwrap_or(false);
        let error_message = context.get::<String>(session_keys::GRAPHQL_RESULT).await;

        let prompt = self.build_prompt(&segments, is_retry, error_message.as_deref())?;
        debug!(%prompt, "Generated prompt for LLM");

        let chat_history = context.get_rig_messages().await;
        let graphql_query_raw = self.agent.chat(&prompt, chat_history).await.map_err(|e| {
            error!(error = ?e, "LLM call for GraphQL generation failed");
            GraphError::TaskExecutionFailed(format!("LLM error: {e}"))
        })?;
        debug!(%graphql_query_raw, "Received raw response from LLM");

        let cleaned_query = graphql_query_raw
            .replace("```graphql", "")
            .replace("```", "")
            .trim()
            .to_string();

        if cleaned_query == "{}" {
            info!("LLM determined no valid GraphQL query could be generated. Ending workflow.");
            context
                .set(session_keys::GRAPHQL_QUERY, String::new())
                .await;
            return Ok(TaskResult::new(
                Some("No valid GraphQL query could be generated.".to_string()),
                NextAction::End,
            ));
        }

        info!(graphql_query = %cleaned_query, "Successfully generated and cleaned GraphQL query");

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

impl GraphQLGeneratorTask {
    fn build_prompt(
        &self,
        segments: &[Segment],
        is_retry: bool,
        error_message: Option<&str>,
    ) -> graph_flow::Result<String> {
        let segments_json = serde_json::to_string_pretty(segments).map_err(|e| {
            error!(error = ?e, "Failed to serialize segments for prompt");
            GraphError::TaskExecutionFailed(format!("Segment serialization error: {e}"))
        })?;

        let prompt = if is_retry {
            let err_msg = error_message.unwrap_or("No error message provided.");
            info!(retry_reason = %err_msg, "Building prompt for retry.");
            format!(
                "Below are parsed segments from a user's question:\n\
                {segments_json}\n\n\
                A previously generated GraphQL query failed with this error:\n\
                {err_msg}\n\n\
                Please regenerate a single, valid GraphQL query that fixes the error while strictly adhering to the schema. \
                Only use information from the segments provided.\n\n\
                Respond with ONLY the GraphQL query string."
            )
        } else {
            format!(
                "Below are parsed segments from a user's question:\n\
                {segments_json}\n\n\
                Generate a single, valid GraphQL query that answers as many of these segments as possible. \
                Omit any segments that cannot be fulfilled by the schema.\n\n\
                Respond with ONLY the GraphQL query string."
            )
        };
        Ok(prompt)
    }
}
