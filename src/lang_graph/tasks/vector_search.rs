use async_trait::async_trait;
use graph_flow::{Context, GraphError, NextAction, Task, TaskResult};
use qdrant_client::{
    qdrant::{CreateCollectionBuilder, Distance, QueryPointsBuilder, VectorParamsBuilder},
    Qdrant,
};
use rig::providers::ollama::NOMIC_EMBED_TEXT;
use rig::{client::EmbeddingsClient, providers};
use rig_qdrant::QdrantVectorStore;

use crate::lang_graph::{
    session_keys,
    types::{
        query::{EnhancedQuery, QueryType},
        response::VectorSearchResult,
    },
};

pub struct VectorSearchTask;

// impl VectorSearchTask {
//     // 임베딩 생성 (실제로는 OpenAI embeddings API 사용)
//     async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
//         // TODO: 실제 임베딩 API 호출
//         // 현재는 모의 임베딩 반환
//         Ok(vec![0.1; 1536]) // OpenAI ada-002 차원
//     }
// }

#[async_trait]
impl Task for VectorSearchTask {
    async fn run(&self, context: Context) -> graph_flow::Result<TaskResult> {
        const COLLECTION_NAME: &str = "rig-collection";

        let db_client = Qdrant::from_url("http://localhost:6334").build()?;

        let llm = providers::ollama::Client::new();

        let model = llm.embedding_model(NOMIC_EMBED_TEXT);
        let query_params = QueryPointsBuilder::new(COLLECTION_NAME).with_payload(true);
        let vector_store = QdrantVectorStore::new(db_client, model, query_params.build());

        // Create a collection with 1536 dimensions if it doesn't exist
        // Note: Make sure the dimensions match the size of the embeddings returned by the
        // model you are using
        if !db_client.collection_exists(COLLECTION_NAME).await? {
            db_client
                .create_collection(
                    CreateCollectionBuilder::new(COLLECTION_NAME)
                        .vectors_config(VectorParamsBuilder::new(1536, Distance::Cosine)),
                )
                .await?;
        }

        let query_type: QueryType = context
            .get_sync(session_keys::QUERY_TYPE)
            .ok_or_else(|| GraphError::ContextError("No query type found".to_string()))?;

        // 정성적 쿼리가 아니면 스킵 (하지만 정량적 쿼리도 컨텍스트가 필요할 수 있음)
        let enhanced_query: EnhancedQuery = context
            .get_sync(session_keys::ENHANCED_QUERY)
            .ok_or_else(|| GraphError::ContextError("No enhanced query found".to_string()))?;

        // 검색 쿼리 준비
        let search_text = match query_type {
            QueryType::Qualitative => &enhanced_query.enhanced,
            QueryType::Quantitative => {
                // 정량적 쿼리도 관련 컨텍스트를 위해 검색할 수 있음
                &enhanced_query.enhanced
            }
        };

        // 임베딩 생성
        let query_embedding = self
            .generate_embedding(search_text)
            .await
            .map_err(|e| GraphError::ContextError(format!("Embedding error: {}", e)))?;

        // 현재는 모의 검색 결과
        let mock_results = vec![
            VectorSearchResult {
                id: "issue_123".to_string(),
                content: "This is a sample GitHub issue about async Rust performance".to_string(),
                metadata: serde_json::json!({
                    "type": "issue",
                    "repo": "rust-lang/rust",
                    "number": 123,
                    "author": "rustacean",
                    "created_at": "2024-01-15T10:30:00Z"
                }),
                score: 0.85,
            },
            VectorSearchResult {
                id: "pr_456".to_string(),
                content: "Pull request implementing new async features".to_string(),
                metadata: serde_json::json!({
                    "type": "pull_request",
                    "repo": "rust-lang/rust",
                    "number": 456,
                    "author": "contributor",
                    "created_at": "2024-01-10T14:20:00Z"
                }),
                score: 0.78,
            },
        ];

        context
            .set(session_keys::VECTOR_SEARCH_RESULTS, mock_results.clone())
            .await;
        context
            .add_assistant_message(format!("Found {} relevant documents", mock_results.len()))
            .await;

        Ok(TaskResult::new(
            Some(format!(
                "Vector search completed with {} results",
                mock_results.len()
            )),
            NextAction::Continue,
        ))
    }
}
