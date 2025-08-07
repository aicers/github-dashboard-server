use core::fmt;
use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    sync::Arc,
};

use anyhow::Result;
use qdrant_client::{
    qdrant::{
        CreateCollectionBuilder, Distance, Filter, PointStruct, QueryPointsBuilder,
        UpsertPointsBuilder, VectorParamsBuilder,
    },
    Payload, Qdrant,
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
use serde_json::{json, Value};
use tokio::sync::OnceCell;
use tracing::info;
use uuid::Uuid;

static URL: &str = "http://localhost:6334";
static ERROR_MESSAGE: &str = "Failed to build Qdrant client. Is Qdrant running?";
static COLLECTION_NAME: &str = "lang_graph";

static VECTOR_STORE: OnceCell<Arc<QdrantVectorStore<EmbeddingModel>>> = OnceCell::const_new();

pub async fn get_storage(filter: Option<Filter>) -> Result<Arc<QdrantVectorStore<EmbeddingModel>>> {
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
            let mut query_builder = QueryPointsBuilder::new(COLLECTION_NAME).with_payload(true);

            if let Some(filter) = filter {
                query_builder = query_builder.filter(filter);
            }

            let vector_store = QdrantVectorStore::new(client, model, query_builder.build());

            Result::<_>::Ok(Arc::new(vector_store))
        })
        .await?;

    Ok(vector_store_arc.clone())
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GithubDocument {
    pub(crate) id: String,
    pub(crate) page_content: String,
    pub(crate) metadata: HashMap<String, Value>,
}

#[derive(Serialize, Deserialize)]
pub enum DocumentType {
    Issue,
    PullRequest,
    Discussion,
}

impl fmt::Display for DocumentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            DocumentType::Issue => "Issue",
            DocumentType::PullRequest => "PullRequest",
            DocumentType::Discussion => "Discussion",
        };
        write!(f, "{s}")
    }
}

impl Embed for GithubDocument {
    fn embed(
        &self,
        embedder: &mut rig::embeddings::TextEmbedder,
    ) -> std::result::Result<(), rig::embeddings::EmbedError> {
        let payload = serde_json::json!({
            "id": self.id,
            "page_content": self.page_content,
            "metadata": self.metadata,
        });
        embedder.embed(serde_json::to_string(&payload).unwrap_or_default());
        Ok(())
    }
}

fn generate_deterministic_uuid(input: &str) -> String {
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    let hash = hasher.finish();

    Uuid::from_u128(u128::from(hash)).to_string()
}

pub async fn add_documents_in_vector_store(documents: Vec<GithubDocument>) -> anyhow::Result<()> {
    if documents.is_empty() {
        info!("No documents provided to add to the vector store.");
        return Ok(());
    }
    let client = Qdrant::from_url(URL).build()?;
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

    info!("VectorStore instance retrieved.");

    let llm = Client::new();
    let model = llm.embedding_model(NOMIC_EMBED_TEXT);

    info!(
        "Starting to process {} documents for embedding.",
        documents.len()
    );

    let texts: Vec<String> = documents.iter().map(|d| d.page_content.clone()).collect();
    let ids = documents
        .iter()
        .map(|doc| generate_deterministic_uuid(&doc.id));
    let payloads = documents.iter().map(|d| {
        json!({
            "page_content": d.page_content,
            "metadata": d.metadata,
        })
    });
    let embeddings = EmbeddingsBuilder::new(model.clone())
        .document(&texts)?
        .build()
        .await?;
    #[allow(clippy::cast_possible_truncation)]
    let vectors: Vec<Vec<f32>> = embeddings
        .into_iter()
        .flat_map(|(_, embedding)| {
            embedding
                .iter()
                .map(|f| {
                    f.vec
                        .clone()
                        .into_iter()
                        .map(|val| val as f32)
                        .collect::<Vec<f32>>()
                })
                .collect::<Vec<Vec<f32>>>()
        })
        .collect::<Vec<Vec<f32>>>();

    let mut points: Vec<PointStruct> = Vec::with_capacity(documents.len());

    for (id, (vector, payload)) in ids.clone().zip(vectors.into_iter().zip(payloads)) {
        let point = PointStruct::new(id, vector, Payload::try_from(payload).unwrap());
        points.push(point);
    }

    if points.is_empty() {
        info!("No valid points were generated from the documents.");
        return Ok(());
    }

    info!(
        "Upserting {} points into collection '{}'...",
        points.len(),
        COLLECTION_NAME
    );

    client
        .upsert_points(
            UpsertPointsBuilder::new(COLLECTION_NAME, points)
                .wait(true)
                .build(),
        )
        .await?;

    info!("Successfully inserted points into the vector store.");
    Ok(())
}
