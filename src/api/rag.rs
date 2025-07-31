use async_graphql::{Context, Object, Result, SimpleObject};

use crate::lang_graph::GitHubRAGSystem;

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
        let rag = ctx.data::<GitHubRAGSystem>()?;
        self.execute_rag_query(rag, query).await
    }
}
impl RagQuery {
    async fn execute_rag_query(&self, rag: &GitHubRAGSystem, query: String) -> Result<RagResponse> {
        match rag.query(&query).await {
            Ok(answer) => {
                let timestamp = jiff::Timestamp::now().to_string();
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
