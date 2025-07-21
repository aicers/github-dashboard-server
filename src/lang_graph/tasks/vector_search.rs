use core::prelude;

use async_trait::async_trait;
use graph_flow::{Context, GraphError, NextAction, Task, TaskResult};
use qdrant_client::{
    qdrant::{CreateCollectionBuilder, Distance, QueryPointsBuilder, VectorParamsBuilder},
    Qdrant,
};
use rig::client::EmbeddingsClient;
use rig::providers::ollama::NOMIC_EMBED_TEXT;
use rig::{providers::ollama::Client, vector_store::VectorStoreIndexDyn};
use rig_qdrant::QdrantVectorStore;
use tracing::{error, info};

use crate::lang_graph::{
    session_keys,
    types::{query::EnhancedQuery, response::VectorSearchResult},
    utils::pretty_log,
};

pub struct VectorSearchTask;
impl VectorSearchTask {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl Task for VectorSearchTask {
    async fn run(&self, context: Context) -> graph_flow::Result<TaskResult> {
        const COLLECTION_NAME: &str = "rag";
        info!("VectorSearchTask started");
        let db_client = Qdrant::from_url("http://localhost:6334")
            .build()
            .map_err(|e| {
                error!("Qdrant client build error: {}", e);
                anyhow::Error::from(e)
            })?;

        if !db_client
            .collection_exists(COLLECTION_NAME)
            .await
            .map_err(|e| anyhow::Error::from(e))?
        {
            info!(
                "Collection '{}' does not exist. Creating...",
                COLLECTION_NAME
            );
            db_client
                .create_collection(
                    CreateCollectionBuilder::new(COLLECTION_NAME)
                        .vectors_config(VectorParamsBuilder::new(768, Distance::Cosine)),
                )
                .await
                .map_err(|e| anyhow::Error::from(e))?;
            info!("Collection '{}' created.", COLLECTION_NAME);
        }
        let llm = Client::new();
        let model = llm.embedding_model(NOMIC_EMBED_TEXT);
        let query_params = QueryPointsBuilder::new(COLLECTION_NAME).with_payload(true);
        let vector_store = QdrantVectorStore::new(db_client, model, query_params.build());

        let enhanced_query: EnhancedQuery = context
            .get_sync(session_keys::ENHANCED_QUERY)
            .ok_or_else(|| {
                error!("No enhanced query found in context");
                GraphError::ContextError("No enhanced query found".to_string())
            })?;
        let search_text = &enhanced_query.original;
        info!("Search text: {}", search_text);

        // Qdrant 벡터 검색 수행
        info!("Performing vector search...");
        let results = match vector_store.top_n(search_text, 10).await {
            Ok(r) => r,
            Err(e) => {
                error!("Vector search error: {}", e);
                return Err(GraphError::ContextError(format!(
                    "Vector search error: {}",
                    e
                )));
            }
        };
        info!("Vector search returned {} results", results.len());

        // 검색 결과를 VectorSearchResult로 변환
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

        context
            .set(session_keys::VECTOR_SEARCH_RESULTS, vector_results.clone())
            .await;
        context
            .add_assistant_message(format!("Found {} relevant documents", vector_results.len()))
            .await;
        info!("Context updated with vector search results");
        pretty_log(
            "VectorSearchTask finished. Results:",
            &serde_json::to_string(&vector_results).unwrap_or_default(),
        );

        Ok(TaskResult::new(
            Some(format!(
                "Vector search completed with {} results",
                vector_results.len()
            )),
            NextAction::Continue, // or NextAction::Continue if you want to keep the flow going
        ))
    }
}
