use async_trait::async_trait;
use graph_flow::{Context, GraphError, NextAction, Task, TaskResult};
use tracing::{info, instrument, warn, Span};

use crate::lang_graph::{session_keys, types::query::EnhancedQuery};

pub struct SegmentParserTask;

impl SegmentParserTask {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SegmentParserTask {
    fn default() -> Self {
        Self::new()
    }
}
#[async_trait]
impl Task for SegmentParserTask {
    #[instrument(name = "segment_parser_task", skip(self, context), fields(session_id))]
    async fn run(&self, context: Context) -> graph_flow::Result<TaskResult> {
        let session_id = context
            .get::<String>("session_id")
            .await
            .unwrap_or_else(|| "unknown".to_string());
        Span::current().record("session_id", &session_id);

        info!("Starting task");

        let enhanced_query: EnhancedQuery = context
            .get::<EnhancedQuery>(session_keys::ENHANCED_QUERY)
            .await
            .ok_or_else(|| GraphError::ContextError("No enhanced query found".to_string()))?;

        let mut quantitative_segments = Vec::new();
        let mut qualitative_segments = Vec::new();

        for segment in &enhanced_query.segments {
            match segment.query_type.as_str() {
                "Quantitative" => quantitative_segments.push(segment.clone()),
                "Qualitative" => qualitative_segments.push(segment.clone()),
                "Mixed" => {
                    quantitative_segments.push(segment.clone());
                    qualitative_segments.push(segment.clone());
                }
                unknown_type => {
                    warn!(%unknown_type, "Ignoring segment with unknown type");
                }
            }
        }

        context
            .set(
                session_keys::QUANTITATIVE_SEGMENTS,
                quantitative_segments.clone(),
            )
            .await;
        context
            .set(
                session_keys::QUALITATIVE_SEGMENTS,
                qualitative_segments.clone(),
            )
            .await;

        info!(
            quantitative_count = quantitative_segments.len(),
            qualitative_count = qualitative_segments.len(),
            "Segment parsing finished"
        );

        Ok(TaskResult::new(
            Some(format!(
                "Segment parsing complete: {} quantitative, {} qualitative",
                quantitative_segments.len(),
                qualitative_segments.len()
            )),
            NextAction::Continue,
        ))
    }
}
