use async_trait::async_trait;
use graph_flow::{Context, NextAction, Task, TaskResult};
use rig_core::{agent::Agent, providers::openai};

use crate::session_keys;
use crate::types::query::EnhancedQuery;
use crate::types::response::VectorSearchResult;

pub struct ContextRerankingTask {
    agent: Agent,
}

impl ContextRerankingTask {
    pub fn new() -> Self {
        let client = openai::Client::from_env();
        let agent = client
            .agent("gpt-4")
            .preamble(
                r#"You are a context reranking specialist. Given a user query and a list of search results, rerank them by relevance.

Consider:
1. Direct relevance to the query
2. Content quality and completeness
3. Recency (newer content may be more relevant)
4. Authority (official documentation, maintainer comments)

Return the results in order of relevance with scores from 0.0 to 1.0.
Format: JSON array with objects containing 'id' and 'relevance_score' fields."#
            )
            .build();

        Self { agent }
    }
}

#[async_trait]
impl Task for ContextRerankingTask {
    async fn run(&self, context: Context) -> graph_flow::Result<TaskResult> {
        let search_results: Vec<VectorSearchResult> = context
            .get_sync(session_keys::VECTOR_SEARCH_RESULTS)
            .unwrap_or_default();

        if search_results.is_empty() {
            context
                .set(
                    session_keys::RERANKED_CONTEXTS,
                    Vec::<VectorSearchResult>::new(),
                )
                .await;
            return Ok(TaskResult::new(
                Some("No search results to rerank".to_string()),
                NextAction::Continue,
            ));
        }

        let enhanced_query: EnhancedQuery = context
            .get_sync(session_keys::ENHANCED_QUERY)
            .ok_or_else(|| graph_flow::Error::custom("No enhanced query found"))?;

        let chat_history = context.get_rig_messages().await;

        // 검색 결과를 요약하여 LLM에 전달
        let results_summary: Vec<serde_json::Value> = search_results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.id,
                    "content_preview": r.content.chars().take(200).collect::<String>(),
                    "metadata": r.metadata,
                    "original_score": r.score
                })
            })
            .collect();

        let prompt = format!(
            "Rerank these search results for the query: '{}'\n\nResults:\n{}\n\nReturn reranked results with relevance scores.",
            enhanced_query.enhanced,
            serde_json::to_string_pretty(&results_summary).unwrap()
        );

        let response = self
            .agent
            .chat(&prompt, chat_history)
            .await
            .map_err(|e| graph_flow::Error::custom(format!("LLM error: {}", e)))?;

        // 재순위 결과 파싱
        let rerank_scores: Vec<serde_json::Value> =
            serde_json::from_str(&response).unwrap_or_else(|_| {
                // 파싱 실패시 원래 순서 유지
                search_results
                    .iter()
                    .enumerate()
                    .map(|(i, r)| {
                        serde_json::json!({
                            "id": r.id,
                            "relevance_score": 1.0 - (i as f64 * 0.1)
                        })
                    })
                    .collect()
            });

        // 재순위된 결과 생성
        let mut reranked_results = search_results.clone();
        reranked_results.sort_by(|a, b| {
            let score_a = rerank_scores
                .iter()
                .find(|s| s["id"].as_str() == Some(&a.id))
                .and_then(|s| s["relevance_score"].as_f64())
                .unwrap_or(a.score as f64);
            let score_b = rerank_scores
                .iter()
                .find(|s| s["id"].as_str() == Some(&b.id))
                .and_then(|s| s["relevance_score"].as_f64())
                .unwrap_or(b.score as f64);

            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // 상위 결과만 유지 (예: 상위 5개)
        reranked_results.truncate(5);

        context
            .set(session_keys::RERANKED_CONTEXTS, reranked_results.clone())
            .await;
        context
            .add_assistant_message(format!(
                "Reranked and filtered to top {} results",
                reranked_results.len()
            ))
            .await;

        Ok(TaskResult::new(
            Some(format!(
                "Context reranking completed, selected top {} results",
                reranked_results.len()
            )),
            NextAction::Continue,
        ))
    }
}
