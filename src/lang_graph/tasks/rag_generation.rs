use async_trait::async_trait;
use graph_flow::{Context, NextAction, Task, TaskResult};
use rig_core::{agent::Agent, providers::openai};

use crate::lang_graph::session_keys;
use crate::session_keys;
use crate::types::query::{EnhancedQuery, QueryType};
use crate::types::response::VectorSearchResult;

pub struct RAGGenerationTask {
    agent: Agent,
}

impl RAGGenerationTask {
    pub fn new() -> Self {
        let client = openai::Client::from_env();
        let agent = client
            .agent("gpt-4")
            .preamble(
                r#"You are a GitHub repository expert assistant. Generate comprehensive, accurate responses using the provided context from GitHub issues, PRs, discussions, and documentation.

Guidelines:
1. Use the provided context to answer questions accurately
2. Cite specific issues, PRs, or discussions when relevant
3. If the context doesn't contain enough information, acknowledge this
4. Provide actionable insights and recommendations when possible
5. Structure responses clearly with proper formatting
6. Include relevant links or references when available in the context

Always base your response on the provided context and clearly indicate when you're making inferences."#
            )
            .build();

        Self { agent }
    }
}

#[async_trait]
impl Task for RAGGenerationTask {
    async fn run(&self, context: Context) -> graph_flow::Result<TaskResult> {
        let query_type: QueryType = context
            .get_sync(session_keys::QUERY_TYPE)
            .ok_or_else(|| graph_flow::Error::custom("No query type found"))?;

        // 정성적 쿼리가 아니면 스킵 (하지만 정량적 쿼리도 RAG 컨텍스트가 도움될 수 있음)
        let enhanced_query: EnhancedQuery = context
            .get_sync(session_keys::ENHANCED_QUERY)
            .ok_or_else(|| graph_flow::Error::custom("No enhanced query found"))?;

        let reranked_contexts: Vec<VectorSearchResult> = context
            .get_sync(session_keys::RERANKED_CONTEXTS)
            .ok_or_else(|| graph_flow::Error::custom("No reranked query found"))?;
    }
}
