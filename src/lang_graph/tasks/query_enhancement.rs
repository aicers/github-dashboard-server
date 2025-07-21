use async_trait::async_trait;
use graph_flow::{Context, GraphError, NextAction, Task, TaskResult};
use rig::{
    agent::Agent,
    client::CompletionClient,
    completion::Chat,
    providers::{self},
};
use serde_json::Value;
use tracing::info;
use tracing_subscriber::util;

use crate::lang_graph::{
    session_keys,
    types::query::{EnhancedQuery, Segment},
    utils::pretty_log,
};

pub struct QueryEnhancementTask {
    agent: Agent<providers::ollama::CompletionModel>,
}

impl QueryEnhancementTask {
    pub fn new() -> Self {
        let client = providers::ollama::Client::new();
        let agent = client
            .agent("llama3.1:8b")
            .preamble(
                r#"
                You are an Json Generator in analyzing and enhancing user queries for GitHub repositories.
                Your task is to split complex queries into segments based on their intent (quantitative/statistics or qualitative/insights).
                Each segment should be enhanced to clarify the user's intent and identify relevant entities.
                Format your response as a JSON array of segments.
                Each segment should include:
                - enhanced: enhanced version of the query
                - query_type: "Quantitative" or "Qualitative"
                - intent: brief description of what the user wants
                - entities: list of relevant entities

                Example response:
                [
                    {
                        "query_type": "Quantitative",
                        "enhanced": "Show me the number of commits in the last month",
                        "intent": "Get commit statistics",
                        "entities": ["commits", "last month"]
                    }
                ]
                ```
                Analyze this GitHub repository query: {user_query}
                If the query contains multiple intents (quantitative/statistics and qualitative/insights), split it into segments and return as a JSON array.

                No explanation. Always respond in JSON format.

                "#
            )
            .build();

        Self { agent }
    }
}

#[async_trait]
impl Task for QueryEnhancementTask {
    async fn run(&self, context: Context) -> graph_flow::Result<TaskResult> {
        let session_id = context
            .get::<String>("session_id")
            .await
            .unwrap_or_else(|| "unknown".to_string());

        info!("QueryEnhancementTask started. Session: {}", session_id);

        let user_query: String = context
            .get_sync(session_keys::USER_QUERY)
            .ok_or_else(|| GraphError::ContextError("No user query found".to_string()))?;

        let chat_history = context.get_rig_messages().await;

        // LLM에게 쿼리 분석 요청
        let prompt = format!("Analyze this GitHub repository query: {user_query}");
        let response = self
            .agent
            .chat(&prompt, chat_history)
            .await
            .map_err(|e| GraphError::ContextError(format!("LLM error: {e}")))?;

        // JSON 응답 파싱 (배열로)
        let segments: Vec<Segment> = serde_json::from_str(&response)
            .map_err(|e| GraphError::ContextError(format!("JSON parse error: {e}")))?;

        pretty_log("QueryEnhancementTask finished. Segments:", &response);

        // EnhancedQuery 생성
        let enhanced_query = EnhancedQuery {
            original: user_query.clone(),
            segments: segments.clone(),
        };

        // 컨텍스트에 저장
        context
            .set(session_keys::ENHANCED_QUERY, enhanced_query.clone())
            .await;

        // 각 segment의 타입을 저장 (필요시)
        let query_types: Vec<_> = segments.iter().map(|s| s.query_type.clone()).collect();
        context.set(session_keys::QUERY_TYPE, query_types).await;

        // 대화 기록 업데이트
        context.add_user_message(user_query).await;
        context
            .add_assistant_message(format!("Query analyzed. Segments: {}", segments.len()))
            .await;

        Ok(TaskResult::new(
            Some(format!(
                "Query enhanced. Segments: {:?}",
                segments.iter().map(|s| &s.query_type).collect::<Vec<_>>()
            )),
            NextAction::Continue,
        ))
    }
}
