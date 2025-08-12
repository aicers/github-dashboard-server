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
    types::{query::EnhancedQuery, response::QualitativeResult},
};

pub struct ResponseFormatterTask {
    agent: Agent<providers::ollama::CompletionModel>,
}

impl ResponseFormatterTask {
    pub fn new(model: &str) -> Self {
        let client = providers::ollama::Client::new();
        let agent = client.agent(model).preamble(
            r#"You are an expert GitHub assistant. Your final task is to synthesize qualitative insights and quantitative data into a single, cohesive, and well-formatted **Markdown** response.

            **Inputs You Will Receive:**
            - **Original User Query**: The user's initial question.
            - **Qualitative Summary**: A text summary based on retrieved documents (issues, PRs, etc.). This provides context and narrative.
            - **Statistical Summary**: A text summary of statistical data derived from GraphQL queries. This provides numbers and metrics.

            **Your Goal:**
            Combine these inputs into a single, easy-to-read Markdown report. Do not just list the inputs; integrate them into a comprehensive answer.

            **CRITICAL RULES:**
            1.  **ALWAYS use Markdown** for formatting (headings, lists, bold text, code blocks, etc.).
            2.  Start with a direct summary answering the user's main question.
            3.  Use headings (`##`, `###`) to structure different parts of the answer (e.g., "Key Statistics", "Detailed Analysis").
            4.  Present statistical data clearly, perhaps in bullet points or tables.
            5.  Seamlessly weave the qualitative summary into the narrative to provide context for the statistics.
            6.  If either the qualitative or statistical summary is empty or unavailable, gracefully construct the answer using only the information you have.
            7.  **DO NOT** output raw JSON. Your final output is for human readers.

            ---
            **EXAMPLE:**

            **--- Inputs ---**
            **Original User Query**: "How many commits were there last month, and what was the main feature developed?"

            **Qualitative Summary**: "Based on commit messages and PR discussions, the main feature developed last month was the 'New Dashboard V2'. It involved a major UI overhaul and backend API integration. Key PRs include #123 and #135."

            **Statistical Summary**: "Last month, there were a total of 250 commits across all branches. The `main` branch received 85 of these commits."

            **--- Your Output (in Markdown) ---**
            ## Monthly Development Summary

            Last month, a total of **250 commits** were made to the repository, with a significant focus on developing the **New Dashboard V2**.

            ### Key Statistics
            - **Total Commits**: 250
            - **Commits to `main`**: 85

            ### Detailed Analysis
            The primary feature shipped was the 'New Dashboard V2', which included a major UI overhaul and new backend integrations. This work is primarily documented in Pull Requests #123 and #135. The high commit count reflects the significant effort invested in this feature.
            "#,
        ).build();
        Self { agent }
    }
}

#[async_trait]
impl Task for ResponseFormatterTask {
    fn id(&self) -> &'static str {
        "ResponseFormatterTask"
    }

    #[instrument(
        name = "response_formatter_task",
        skip(self, context),
        fields(session_id)
    )]
    async fn run(&self, context: Context) -> graph_flow::Result<TaskResult> {
        let session_id = context
            .get::<String>("session_id")
            .await
            .unwrap_or_else(|| "unknown".to_string());
        Span::current().record("session_id", &session_id);
        info!("Starting final response formatting");

        let enhanced_query: EnhancedQuery = context
            .get(session_keys::ENHANCED_QUERY)
            .await
            .ok_or_else(|| GraphError::ContextError("No enhanced query found".to_string()))?;

        let rag_results: Vec<QualitativeResult> = context
            .get(session_keys::RAG_RESPONSE)
            .await
            .unwrap_or_default();

        let statistics_response: String = context
            .get(session_keys::STATISTICS_RESPONSE)
            .await
            .unwrap_or_default();

        let prompt = self.build_prompt(&enhanced_query, &rag_results, &statistics_response);
        debug!(%prompt, "Generated final prompt for formatting");

        let chat_history = context.get_rig_messages().await;
        let final_response = self.agent.chat(&prompt, chat_history).await.map_err(|e| {
            error!(error = ?e, "LLM call for final formatting failed");
            GraphError::TaskExecutionFailed(format!("Final formatting LLM error: {e}"))
        })?;

        info!("Successfully generated final formatted response.");
        debug!(final_response = %final_response, "Final response content");

        Ok(TaskResult::new(Some(final_response), NextAction::End))
    }
}

impl ResponseFormatterTask {
    fn build_prompt(
        &self,
        enhanced_query: &EnhancedQuery,
        rag_results: &[QualitativeResult],
        statistics_response: &str,
    ) -> String {
        let qualitative_summary = rag_results
            .iter()
            .map(|r| r.generated_response.clone())
            .collect::<Vec<_>>()
            .join("\n\n");

        format!(
            "Synthesize the following information into a single, cohesive Markdown response.\n\n\
            --- Inputs ---\n\
            **Original User Query**: \"{}\"\n\n\
            **Qualitative Summary**: \"{}\"\n\n\
            **Statistical Summary**: \"{}\"",
            enhanced_query.original,
            if qualitative_summary.is_empty() {
                "N/A"
            } else {
                &qualitative_summary
            },
            if statistics_response.is_empty() {
                "N/A"
            } else {
                statistics_response
            }
        )
    }
}
