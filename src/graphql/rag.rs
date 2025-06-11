use std::sync::Arc;

use async_graphql::{Context, Object, Result, SimpleObject};
use tokio::sync::Mutex;

use crate::rag_sample::RagOllamaSystem;

#[derive(SimpleObject)]
pub struct RagResponse {
    pub query: String,
    pub answer: String,
    pub timestamp: Option<String>,
}

#[derive(Default)]
pub struct RagQuery;
#[Object]
impl RagQuery {
    async fn query(&self, ctx: &Context<'_>, query: String) -> Result<RagResponse> {
        let rag = ctx.data::<Arc<Mutex<RagOllamaSystem>>>()?;
        self.execute_rag_query(rag.clone(), query).await
    }

    async fn simple_query(&self, ctx: &Context<'_>, query: String) -> Result<String> {
        let rag = ctx.data::<Arc<Mutex<RagOllamaSystem>>>()?;
        let rag_guard = rag.lock().await;

        match rag_guard.query(&query).await {
            Ok(answer) => Ok(answer),
            Err(e) => Err(async_graphql::Error::new(format!("RAG query failed: {e}"))),
        }
    }
    async fn batch_query(
        &self,
        ctx: &Context<'_>,
        queries: Vec<String>,
    ) -> Result<Vec<RagResponse>> {
        let rag = ctx.data::<Arc<Mutex<RagOllamaSystem>>>()?;
        let mut results = Vec::new();

        for query in queries {
            match self.execute_rag_query(Arc::clone(rag), query).await {
                Ok(response) => results.push(response),
                Err(e) => return Err(e),
            }
        }

        Ok(results)
    }
}
impl RagQuery {
    /// RAG 쿼리 실행을 위한 헬퍼 메서드
    async fn execute_rag_query(
        &self,
        rag: Arc<Mutex<RagOllamaSystem>>,
        query: String,
    ) -> Result<RagResponse> {
        let mut rag_guard = rag.lock().await;

        match rag_guard.query_with_filter(&query.clone()).await {
            Ok(answer) => {
                let timestamp = chrono::Utc::now().to_rfc3339();
                Ok(RagResponse {
                    query,
                    answer,
                    timestamp: Some(timestamp),
                })
            }
            Err(e) => Err(async_graphql::Error::new(format!("RAG query failed: {e}"))),
        }
    }
}
