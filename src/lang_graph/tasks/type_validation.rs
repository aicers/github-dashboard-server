use async_trait::async_trait;
use graph_flow::{Context, GraphError, NextAction, Task, TaskResult};
use rig::{
    agent::Agent,
    client::CompletionClient,
    completion::Chat,
    providers::{self},
};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, instrument, Span};

use crate::lang_graph::{session_keys, types::query::EnhancedQuery};

#[derive(Debug, Deserialize, Serialize)]
struct ValidationResponse {
    is_correct: bool,
    reason: String,
}

pub struct TypeValidationTask {
    agent: Agent<providers::ollama::CompletionModel>,
}

impl TypeValidationTask {
    pub fn new(model: &str) -> Self {
        let client = providers::ollama::Client::new();

        let prompt = r#"
        You are a meticulous Query Type Validator for a GitHub data analysis system.
        Your task is to determine if the set of generated segments accurately and completely captures the user's intent.

        **1. Query Type Definitions:**
        - "Quantitative": Asks for statistics, numbers, counts (e.g., "how many commits?"). Answerable by structured data queries.
        - "Qualitative": Asks for reasons, summaries, descriptions (e.g., "what was the reason for this bug?"). Answerable by searching documents.
        - "Mixed": Contains elements of BOTH quantitative and qualitative intents within a single segment.

        **2. Validation Logic:**
        Your primary goal is to ensure the user's complete intent is captured.

        - A user query with **both** quantitative and qualitative aspects is considered **CORRECTLY classified** in two scenarios:
            - **(A)** It is classified as a single segment with `"query_type": "Mixed"`.
            - **(B)** It is split into multiple segments, where at least one `"Quantitative"` segment AND at least one `"Qualitative"` segment are present. This is a valid way to handle mixed intent.

        - The classification is **INCORRECT** if a query with clear mixed intent is classified as *only* "Quantitative" segment(s) or *only* "Qualitative" segment(s), completely missing the other aspect.

        **3. Response Format:**
        Do NOT include any text, explanation, or commentary.
        Respond ONLY with a JSON object in the following format:
        {
            "is_correct": boolean,
            "reason": "A brief explanation for your decision. Explain why it is correct or what was missed."
        }

        Example of a CORRECT classification (Split Segments):
        - User Query: "How many commits were there last month, and what was the main feature developed?"
        - Generated Segments: [ {"query_type": "Quantitative", "enhanced": "Count commits from last month"}, {"query_type": "Qualitative", "enhanced": "Summarize the main feature developed last month"} ]
        - Your Response: { "is_correct": true, "reason": "Correct. The user's mixed intent was properly split into separate quantitative and qualitative segments." }

        Example of an INCORRECT classification (Missed Intent):
        - User Query: "Find the issue with the most comments and summarize the discussion."
        - Generated Segments: [ { "query_type": "Quantitative", ... } ]
        - Your Response: { "is_correct": false, "reason": "Incorrect. The query also asks for a summary, which is a qualitative intent. The classification missed this aspect and should have been 'Mixed' or split into two segments." }
        "#.to_string();

        let agent = client
            .agent(model)
            .preamble(&prompt)
            .temperature(0.3)
            .build();
        Self { agent }
    }
}

#[async_trait]
impl Task for TypeValidationTask {
    #[instrument(
        name = "type_validation_task",
        skip(self, context),
        fields(session_id, user_query)
    )]
    async fn run(&self, context: Context) -> graph_flow::Result<TaskResult> {
        let session_id = context
            .get::<String>("session_id")
            .await
            .unwrap_or_else(|| "unknown".to_string());
        Span::current().record("session_id", &session_id);

        let user_query: String = context
            .get_sync(session_keys::USER_QUERY)
            .ok_or_else(|| GraphError::ContextError("No user query found".to_string()))?;
        Span::current().record("user_query", &user_query);

        info!("Starting task");

        let enhanced_query: EnhancedQuery = context
            .get::<EnhancedQuery>(session_keys::ENHANCED_QUERY)
            .await
            .ok_or_else(|| GraphError::ContextError("No enhanced query found".to_string()))?;

        if enhanced_query.segments.is_empty() {
            info!("No segments to validate. Skipping validation and marking as passed.");
            context.set(session_keys::VALIDATION_PASS, true).await;
            return Ok(TaskResult::new(
                Some("No segments to validate.".to_string()),
                NextAction::Continue,
            ));
        }

        let generated_segments_json = serde_json::to_string_pretty(&enhanced_query.segments)
            .map_err(|e| {
                error!(error = ?e, "Failed to serialize segments for validation prompt");
                GraphError::TaskExecutionFailed(format!("Failed to serialize segments: {e}"))
            })?;

        let prompt_with_context = format!(
            "validate the following.\nOriginal User Query: {user_query}\nGenerated Segments: {generated_segments_json}",
        );

        debug!(prompt = %prompt_with_context, "Sending validation prompt to LLM");

        let chat_history = context.get_rig_messages().await;
        let response_str = self
            .agent
            .chat(&prompt_with_context, chat_history)
            .await
            .map_err(|e| {
                error!(error = ?e, "LLM validation API call failed");
                GraphError::ContextError(format!("LLM validation error: {e}"))
            })?;

        debug!(raw_response = %response_str, "Received raw validation response");

        let validation_response: ValidationResponse =
            serde_json::from_str(&response_str).map_err(|e| {
                error!(error = ?e, raw_response = %response_str, "Failed to parse validation JSON");
                GraphError::ContextError(format!(
                    "Validation JSON parse error: {e} - Response was: {response_str}"
                ))
            })?;

        info!(
            validation_pass = validation_response.is_correct,
            reason = %validation_response.reason,
            "Type validation finished"
        );

        context
            .set(
                session_keys::VALIDATION_PASS,
                validation_response.is_correct,
            )
            .await;

        let result_message = if validation_response.is_correct {
            format!("Validation passed: {}", validation_response.reason)
        } else {
            format!(
                "Validation failed, please regenerate segments. Feedback: {}",
                validation_response.reason
            )
        };

        context.set("validation_message", &result_message).await;

        Ok(TaskResult::new(Some(result_message), NextAction::Continue))
    }
}
