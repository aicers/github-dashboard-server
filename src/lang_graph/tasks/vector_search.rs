use anyhow::Result;
use async_trait::async_trait;
use graph_flow::{Context, NextAction, Task, TaskResult};
use qdrant_client::qdrant::{Condition, Filter};
use rig::{
    agent::Agent,
    client::CompletionClient,
    completion::Chat,
    completion::Message,
    providers::{self, ollama::CompletionModel},
    vector_store::VectorStoreIndexDyn,
};
use tracing::{debug, error, info, instrument, Span};

use crate::{
    lang_graph::{
        session_keys,
        types::{query::Segment, response::VectorSearchResult},
    },
    vector_db::get_storage,
};

pub struct VectorSearchTask {
    agent: Agent<CompletionModel>,
}
impl VectorSearchTask {
    pub fn new(model: &str) -> Self {
        let client = providers::ollama::Client::new();
        let agent = client
            .agent(model)
            .preamble(
                r#"You are an AI assistant that specializes in parsing user queries to extract structured data for filtering GitHub information.
                Your sole function is to analyze the user's query and return a JSON object containing the appropriate filters based on the schema provided below.

                ---
                **FILTER SCHEMA:**

                You can only use the following keys. The values must match the specified type.

                - `metadata.type`: (String) Must be one of "Issue", "Pull Request", or "Discussion".
                - `metadata.repo`: (String) The repository in "owner/name" format.
                - `metadata.author`: (String) The GitHub username of the author.
                - `metadata.number`: (Integer) The number of an issue or pull request or discussion.

                ---
                **OUTPUT RULES:**
                Respond ONLY with a JSON object in the following format:
                1.  The output MUST be a single, valid JSON object.
                2.  If filterable criteria are found in the query, use the keys from the FILTER SCHEMA.
                3.  If NO filterable criteria are found, you MUST output exactly: `{"no_filter": true}`
                4.  Do NOT include any explanations, markdown formatting (like ```json), or any text outside of the JSON object itself.

                ---
                **EXAMPLES:**

                - User Query: `show me john's issues`
                - Your Output: `{"metadata.type": "Issue", "metadata.author": "john"}`

                - User Query: `What is the status of PR #123 in the aicers/dashboard repo?`
                - Your Output: `{"metadata.type": "Pull Request", "metadata.number": 123, "metadata.repo": "aicers/dashboard"}`

                - User Query: `what is vector db?`
                - Your Output: `{"no_filter": true}`
                "#,
            )
            .build();
        Self { agent }
    }
}
#[async_trait]
impl Task for VectorSearchTask {
    #[instrument(name = "vector_search_task", skip(self, context), fields(session_id))]
    async fn run(&self, context: Context) -> graph_flow::Result<TaskResult> {
        let session_id = context
            .get::<String>("session_id")
            .await
            .unwrap_or_else(|| "unknown".to_string());
        Span::current().record("session_id", &session_id);

        info!("Starting task");

        let qualitative_segments: Vec<Segment> = context
            .get_sync(session_keys::QUALITATIVE_SEGMENTS)
            .unwrap_or_default();

        if qualitative_segments.is_empty() {
            info!("No qualitative segments to process. Skipping.");
            context
                .set(session_keys::VECTOR_SEARCH_RESULTS, Vec::<Segment>::new())
                .await;
            return Ok(TaskResult::new(
                Some("No qualitative segments found".to_string()),
                NextAction::Continue,
            ));
        }

        let mut all_results = Vec::new();
        let chat_history = context.get_rig_messages().await;

        for segment in qualitative_segments {
            match self
                .search_for_segment(chat_history.clone(), &segment)
                .await
            {
                Ok(vector_results) => {
                    all_results.push((segment, vector_results));
                }
                Err(e) => {
                    error!(segment_id = %segment.id, error = ?e, "Failed to process segment");
                }
            }
        }

        info!(
            processed_segments = all_results.len(),
            "Finished processing all segments."
        );

        context
            .set(session_keys::VECTOR_SEARCH_RESULTS, all_results.clone())
            .await;
        context
            .add_assistant_message(format!("Found {} relevant documents", all_results.len()))
            .await;

        Ok(TaskResult::new(
            Some(format!(
                "Vector search completed for {} segments",
                all_results.len()
            )),
            NextAction::Continue,
        ))
    }
}

impl VectorSearchTask {
    #[instrument(
        name = "search_for_segment",
        skip(self, chat_history),
        fields(segment_id = %segment.id, segment_enhanced = %segment.enhanced)
    )]
    async fn search_for_segment(
        &self,
        chat_history: Vec<Message>,
        segment: &Segment,
    ) -> Result<Vec<VectorSearchResult>> {
        info!("Processing segment");
        let filter = self.generate_filter(chat_history, segment).await?;
        debug!(?filter, "Generated filter for segment");

        let vector_store = get_storage(filter.clone()).await?;
        let search_results = vector_store.top_n(&segment.enhanced, 10).await;

        let results = match search_results {
            Ok(docs) if docs.is_empty() && filter.is_some() => {
                info!("Search with filter yielded 0 results. Retrying without filter...");
                let vector_store_no_filter = get_storage(None).await?;
                vector_store_no_filter.top_n(&segment.enhanced, 10).await?
            }
            Ok(docs) => docs,
            Err(e) => {
                error!(error = %e, "Initial vector search failed");
                return Err(anyhow::anyhow!("Vector search error: {e}"));
            }
        };

        info!(found_documents = results.len(), "Vector search successful");

        let vector_results: Vec<VectorSearchResult> = results
            .into_iter()
            .map(|(score, id, payload)| VectorSearchResult {
                id: id.to_string(),
                content: payload
                    .get("page_content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                metadata: serde_json::to_value(payload.get("metadata")).unwrap_or_default(),
                score: score as f32,
            })
            .collect();

        debug!(results = ?vector_results, "Mapped search results");

        Ok(vector_results)
    }

    #[instrument(name = "generate_filter", skip(self, chat_history, segment))]
    async fn generate_filter(
        &self,
        chat_history: Vec<Message>,
        segment: &Segment,
    ) -> Result<Option<Filter>> {
        let prompt = format!("Analyze this query: {}", &segment.enhanced);
        debug!(%prompt, "Generating filter with LLM");

        let response = self.agent.chat(&prompt, chat_history).await.map_err(|e| {
            error!(error = ?e, "LLM call for filter generation failed");
            anyhow::anyhow!("LLM error: {e}")
        })?;

        debug!(raw_response = %response, "Received raw filter response from LLM");

        let generated_filter: serde_json::Value = serde_json::from_str(&response).map_err(|e| {
            error!(error = ?e, raw_response = %response, "Failed to parse filter JSON");
            anyhow::anyhow!("JSON parse error: {e}")
        })?;

        let filter = if generated_filter.get("no_filter").is_some() {
            None
        } else {
            let mut conditions = Vec::new();

            if let Some(doc_type) = generated_filter
                .get("metadata.type")
                .and_then(|v| v.as_str())
            {
                conditions.push(Condition::matches("metadata.type", doc_type.to_string()));
            }
            if let Some(author) = generated_filter
                .get("metadata.author")
                .and_then(|v| v.as_str())
            {
                conditions.push(Condition::matches("metadata.author", author.to_string()));
            }
            if let Some(repo) = generated_filter
                .get("metadata.repo")
                .and_then(|v| v.as_str())
            {
                let full_repo_name = if repo.contains('/') {
                    repo.to_string()
                } else {
                    format!("aicers/{repo}")
                };
                conditions.push(Condition::matches("metadata.repo", full_repo_name));
            }
            if let Some(number) = generated_filter
                .get("metadata.number")
                .and_then(serde_json::Value::as_i64)
            {
                conditions.push(Condition::matches("metadata.number", number));
            }
            Some(Filter::must(conditions))
        };

        Ok(filter)
    }
}
