use std::sync::Arc;

use anyhow::Result;
use qdrant_client::{
    qdrant::{CreateCollectionBuilder, Distance, QueryPointsBuilder, VectorParamsBuilder},
    Qdrant,
};
use rig::providers::ollama::NOMIC_EMBED_TEXT;
use rig::{
    client::EmbeddingsClient,
    embeddings::EmbeddingsBuilder,
    providers::ollama::{Client, EmbeddingModel},
    Embed,
};
use rig_qdrant::QdrantVectorStore;
use serde::{Deserialize, Serialize};
use tokio::sync::OnceCell;
use tracing::info;
use uuid::Uuid;

static URL: &str = "http://localhost:6334";
static ERROR_MESSAGE: &str = "Failed to build Qdrant client. Is Qdrant running?";
static COLLECTION_NAME: &str = "lang_graph";

static VECTOR_STORE: OnceCell<Arc<QdrantVectorStore<EmbeddingModel>>> = OnceCell::const_new();

pub async fn get_storage() -> Result<Arc<QdrantVectorStore<EmbeddingModel>>> {
    let vector_store_arc = VECTOR_STORE
        .get_or_try_init(|| async {
            info!("Initializing Qdrant client and VectorStore for the first time...");

            let client = Qdrant::from_url(URL).build().expect(ERROR_MESSAGE);

            if !client.collection_exists(COLLECTION_NAME).await? {
                info!(
                    "Collection '{}' does not exist. Creating...",
                    COLLECTION_NAME
                );
                client
                    .create_collection(
                        CreateCollectionBuilder::new(COLLECTION_NAME)
                            .vectors_config(VectorParamsBuilder::new(768, Distance::Cosine)),
                    )
                    .await?;
                info!("Collection '{}' created.", COLLECTION_NAME);
            }

            let llm = Client::new();
            let model = llm.embedding_model(NOMIC_EMBED_TEXT);

            let vector_store = QdrantVectorStore::new(
                client,
                model,
                QueryPointsBuilder::new(COLLECTION_NAME)
                    .with_payload(true)
                    .build(),
            );

            Result::<_>::Ok(Arc::new(vector_store))
        })
        .await?;

    Ok(vector_store_arc.clone())
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GithubDocument {
    id: String,
    doc_type: String,
    data: String,
}

impl Embed for GithubDocument {
    fn embed(
        &self,
        embedder: &mut rig::embeddings::TextEmbedder,
    ) -> std::result::Result<(), rig::embeddings::EmbedError> {
        embedder.embed(self.data.clone());
        Ok(())
    }
}

pub async fn add_document(document: &str, doc_type: &str) -> anyhow::Result<()> {
    let vector_store = get_storage().await?;
    info!("VectorStore instance retrieved.");

    let llm = Client::new();
    let model = llm.embedding_model(NOMIC_EMBED_TEXT);
    info!("Embedding model ready.");

    let doc_to_embed = GithubDocument {
        id: Uuid::new_v4().to_string(),
        doc_type: doc_type.to_string(),
        data: document.to_string(),
    };

    info!("Building embeddings for the document...");
    let embeddings = EmbeddingsBuilder::new(model)
        .document(doc_to_embed)?
        .build()
        .await?;
    info!("Embeddings built successfully.");

    vector_store.insert_documents(embeddings).await?;
    info!("Document successfully inserted into the vector store.");

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::lang_graph::vector_db::add_document;

    #[tokio::test]
    async fn test_add_document() {
        // add_document의 실제 시그니처에 맞게 호출
        let result = add_document("Test document content", "issue").await;
        assert!(result.is_ok(), "Failed to add document: {:?}", result);
    }
}
