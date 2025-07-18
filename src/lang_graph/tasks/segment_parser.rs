use async_trait::async_trait;
use graph_flow::{Context, GraphError, NextAction, Task, TaskResult};
use tracing::info;

use crate::lang_graph::{
    session_keys,
    types::query::{EnhancedQuery, Segment},
};

pub struct SegmentParserTask;

impl SegmentParserTask {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Task for SegmentParserTask {
    async fn run(&self, context: Context) -> graph_flow::Result<TaskResult> {
        let session_id = context
            .get::<String>("session_id")
            .await
            .unwrap_or_else(|| "unknown".to_string());

        info!("SegmentParserTask started. Session: {}", session_id);

        // EnhancedQuery 가져오기
        let enhanced_query: EnhancedQuery = context
            .get::<EnhancedQuery>(session_keys::ENHANCED_QUERY)
            .await
            .ok_or_else(|| GraphError::ContextError("No enhanced query found".to_string()))?;

        // 정량/정성 segment 분리
        let mut quantitative_segments = Vec::new();
        let mut qualitative_segments = Vec::new();

        for segment in &enhanced_query.segments {
            match segment.query_type.as_str() {
                "Quantitative" => quantitative_segments.push(segment.clone()),
                "Qualitative" => qualitative_segments.push(segment.clone()),
                _ => {}
            }
        }

        // context에 저장 (다음 태스크에서 사용)
        context
            .set("quantitative_segments", quantitative_segments.clone())
            .await;
        context
            .set("qualitative_segments", qualitative_segments.clone())
            .await;

        info!(
            "SegmentParserTask finished. Quantitative: {}, Qualitative: {}",
            quantitative_segments.len(),
            qualitative_segments.len()
        );

        Ok(TaskResult::new(
            Some(format!(
                "Segment parsing complete: {} quantitative, {} qualitative",
                quantitative_segments.len(),
                qualitative_segments.len()
            )),
            NextAction::End,
        ))
    }
}
