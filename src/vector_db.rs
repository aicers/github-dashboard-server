use anyhow::Result;
use qdrant_client::qdrant::{
    r#match::MatchValue, CreateCollectionBuilder, Distance, FieldCondition, Filter, Match,
    PointStruct, SearchPointsBuilder, UpsertPointsBuilder, VectorParamsBuilder,
};
use qdrant_client::{Payload, Qdrant};
use rust_bert::pipelines::hf_tokenizers::HFTokenizer;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
// use text_splitter::{ChunkConfig, TextSplitter};
use tokio::sync::OnceCell as AsyncOnceCell;

use crate::embedder::Embedder;
use crate::graphql::Issue;
use crate::graphql::PullRequest;
use crate::utils::tensor_to_vec;

static QDRANT: AsyncOnceCell<Qdrant> = AsyncOnceCell::const_new();

#[derive(Deserialize, Serialize, Debug, Clone)]
pub(crate) struct IssueChunk {
    pub(crate) title: String,
    pub(crate) author: String,
    pub(crate) content: String, // the actual issue body or summary you want to embed
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub(crate) struct PrChunk {
    pub(crate) title: String,
    pub(crate) authors: Vec<String>,
    pub(crate) reviewers: Vec<String>,
    pub(crate) content: String,
}

/// A “wrapper” that can hold either kind of chunk
#[derive(Deserialize, Serialize, Debug, Clone)]
pub enum Chunk {
    Issue(IssueChunk),
    PullRequest(PrChunk),
}

#[allow(clippy::missing_panics_doc)]
pub(crate) async fn get_qdrant_client() -> &'static Qdrant {
    QDRANT
        .get_or_init(|| async {
            Qdrant::from_url("http://localhost:6334")
                .build()
                .expect("Cannot connect to Qdrant")
        })
        .await
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct VectorData {
    pub point_id: u64,
    pub content_id: String,
    pub chunk_number: u16,
    pub content_chunk: String,
    pub source: String,
    pub chunk: Option<Chunk>,
}

pub(crate) async fn create_collection(vector_size: usize) -> Result<()> {
    let qdrant = get_qdrant_client().await;

    qdrant
        .create_collection(
            CreateCollectionBuilder::new("content_vectors")
                .vectors_config(VectorParamsBuilder::new(
                    vector_size as u64,
                    Distance::Cosine,
                ))
                .build(),
        )
        .await?;

    Ok(())
}

pub(crate) async fn insert_vector_data(
    point_id: u64,
    content_id: String,
    chunk_number: u16,
    content_chunk: &str,
    vector: Vec<f32>,
    source_filename: &str,
    payload_chunk: Option<&Chunk>,
) -> Result<()> {
    let qdrant = get_qdrant_client().await;

    // build base JSON
    let mut obj = json!({
        "content_id": content_id,
        "chunk_number": chunk_number,
        "content_chunk": content_chunk,
        "source": source_filename,
    });
    // if we have a structured chunk, embed it under "chunk"
    if let Some(chunk) = payload_chunk {
        obj["chunk"] = serde_json::to_value(chunk)?;
    }
    let payload = Payload::try_from(obj)?;

    let points = vec![PointStruct::new(point_id, vector, payload)];

    qdrant
        .upsert_points(UpsertPointsBuilder::new("content_vectors", points).wait(true))
        .await?;

    Ok(())
}

pub(crate) async fn get_related_chunks(
    query_vector: Vec<f32>,
    limit: usize,
) -> Result<Vec<VectorData>> {
    let qdrant = get_qdrant_client().await;

    let search_result = qdrant
        .search_points(
            SearchPointsBuilder::new("content_vectors", query_vector, limit as u64)
                .with_payload(true),
        )
        .await?;

    let vector_data = search_result
        .result
        .into_iter()
        .filter_map(|point| {
            let qdrant_client::qdrant::point_id::PointIdOptions::Num(point_id) =
                point.id?.point_id_options?
            else {
                return None;
            };

            let mut vd = VectorData {
                point_id,
                content_id: point.payload.get("content_id")?.as_str()?.to_string(),
                chunk_number: point
                    .payload
                    .get("chunk_number")?
                    .as_integer()?
                    .try_into()
                    .ok()?,
                content_chunk: point.payload.get("content_chunk")?.as_str()?.to_string(),
                source: point.payload.get("source")?.as_str()?.to_string(),
                chunk: None,
            };
            // if there's a "chunk" field, deserialize it back
            if let Some(chunk_val) = point.payload.get("chunk") {
                // Convert Qdrant's Value → serde_json::Value
                let json_val: serde_json::Value = chunk_val.clone().into();
                if let Ok(chunk) = serde_json::from_value::<Chunk>(json_val) {
                    vd.chunk = Some(chunk);
                }
            }
            Some(vd)
        })
        .collect();

    Ok(vector_data)
}

pub(crate) async fn get_related_chunks_with_filter(
    collection: &str,
    author: &str,
    limit: usize,
) -> Result<Vec<VectorData>> {
    let qdrant = get_qdrant_client().await;

    // chunk.author == author 필터
    let filter = Filter {
        must: vec![
            FieldCondition {
                key: "chunk.author".into(),
                r#match: Some(Match {
                    match_value: Some(MatchValue::from(author.to_string())),
                }),
                ..Default::default()
            }
            .into(), // ← 반드시 `.into()` 호출!
        ],
        ..Default::default()
    };

    let resp = qdrant
        .search_points(
            SearchPointsBuilder::new(collection, Vec::new(), limit as u64)
                .filter(filter) // ← with_filter → filter
                .with_payload(true), // ← 페이로드 포함
        )
        .await?;

    // 기존 get_related_chunks 로직 그대로 재활용
    let out = resp
        .result
        .into_iter()
        .filter_map(|point| {
            let qdrant_client::qdrant::point_id::PointIdOptions::Num(point_id) =
                point.id?.point_id_options?
            else {
                return None;
            };

            let mut vd = VectorData {
                point_id,
                content_id: point.payload.get("content_id")?.as_str()?.to_string(),
                chunk_number: point
                    .payload
                    .get("chunk_number")?
                    .as_integer()?
                    .try_into()
                    .ok()?,
                content_chunk: point.payload.get("content_chunk")?.as_str()?.to_string(),
                source: point.payload.get("source")?.as_str()?.to_string(),
                chunk: None,
            };
            if let Some(val) = point.payload.get("chunk") {
                let json_val: serde_json::Value = val.clone().into();
                if let Ok(c) = serde_json::from_value::<Chunk>(json_val) {
                    vd.chunk = Some(c);
                }
            }
            Some(vd)
        })
        .collect();

    Ok(out)
}

/// Render a GitHub Issue into the chunk format
pub fn render_issue_chunk(issue: &Issue) -> Chunk {
    Chunk::Issue(IssueChunk {
        title: issue.title.clone(),
        author: issue.author.clone(),
        content: format!(
            "Issue Title: {}\nAuthor: {}\nURL: https://github.com/{}/{}/issues/{}\n",
            &issue.title, &issue.author, &issue.owner, &issue.repo, issue.number
        ),
    })
}

/// Render a PullRequest into the chunk format
pub(crate) fn render_pr_chunk(pr: &PullRequest) -> Chunk {
    let title = pr.title.clone();
    let authors = pr.assignees.clone();
    let reviewers = pr.reviewers.clone();

    let content = format!(
        "PR Title: {}\n\
         Authors: {:?}\n\
         Reviewers: {:?}\n\
         URL: https://github.com/{}/{}/pull/{}\n",
        &title, &authors, &reviewers, &pr.owner, &pr.repo, pr.number
    );

    Chunk::PullRequest(PrChunk {
        title,
        authors,
        reviewers,
        content,
    })
}

pub(crate) async fn search_related_chunks(
    embedder: &Embedder,
    tokenizer: &HFTokenizer,
    query: &str,
    limit: usize,
) -> Result<Vec<VectorData>> {
    let embedding = embedder.encode_texts(tokenizer, &[query])?;
    let normalized_embedding = Embedder::normalize_embeddings(&embedding);
    let query_vector = tensor_to_vec(&normalized_embedding)?;

    get_related_chunks(query_vector, limit).await
}
