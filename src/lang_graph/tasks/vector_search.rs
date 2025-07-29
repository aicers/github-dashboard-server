use async_trait::async_trait;
use graph_flow::{Context, GraphError, NextAction, Task, TaskResult};
use rig::vector_store::VectorStoreIndexDyn;
use tracing::{error, info};

use crate::{
    lang_graph::{
        session_keys,
        types::{query::Segment, response::VectorSearchResult},
    },
    vector_db::get_storage,
};

pub struct VectorSearchTask;
impl VectorSearchTask {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
#[allow(clippy::too_many_lines)]
impl Task for VectorSearchTask {
    async fn run(&self, context: Context) -> graph_flow::Result<TaskResult> {
        info!("{}", self.id());
        let session_id = context
            .get::<String>("session_id")
            .await
            .unwrap_or_else(|| "unknown".to_string());
        info!("VectorSearchTask started. Session: {}", session_id);
        let qualitative_segments: Vec<Segment> = context
            .get_sync(session_keys::QUALITATIVE_SEGMENTS)
            .unwrap_or_default();

        if qualitative_segments.is_empty() {
            context
                .set(
                    session_keys::VECTOR_SEARCH_RESULTS,
                    Vec::<(Segment, Vec<VectorSearchResult>)>::new(),
                )
                .await;
            return Ok(TaskResult::new(
                Some("No qualitative segments found".to_string()),
                NextAction::Continue,
            ));
        }

        info!("VectorSearchTask started");

        let vector_store = get_storage().await?;
        let mut segement_vector_results = Vec::new();

        for segment in &qualitative_segments {
            info!("Processing segment: {:?}", segment);
            info!("Query text: {}", segment.enhanced);

            info!("Performing vector search...");
            let results = match vector_store.top_n(&segment.enhanced, 10).await {
                Ok(r) => r,
                Err(e) => {
                    error!("Vector search error: {}", e);
                    return Err(GraphError::ContextError(format!(
                        "Vector search error: {e}"
                    )));
                }
            };
            info!("Vector search returned {} results", results.len());

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

            info!(
                "Segment '{}' vector search results: {:?}",
                segment.enhanced, vector_results
            );
            segement_vector_results.push((segment.clone(), vector_results));
        }

        context
            .set(
                session_keys::VECTOR_SEARCH_RESULTS,
                segement_vector_results.clone(),
            )
            .await;
        context
            .add_assistant_message(format!(
                "Found {} relevant documents",
                segement_vector_results.len()
            ))
            .await;
        info!("Context updated with vector search results");

        Ok(TaskResult::new(
            Some(format!(
                "Vector search completed with {} results",
                segement_vector_results.len()
            )),
            NextAction::Continue, // or NextAction::Continue if you want to keep the flow going
        ))
    }
}
