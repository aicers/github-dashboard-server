use std::fmt::Write;

use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use surrealdb::engine::remote::http::Client as SurrealClient;
use surrealdb::Surreal;

use crate::embeddings::get_embeddings;

#[derive(Debug, Serialize, Deserialize)]
pub struct Doc {
    pub id: String,
    pub text: String,
    pub embedding: Vec<f32>,
}

/// Ingest a batch of (id, text) documents into `SurrealDB`.
pub async fn ingest(db: &Surreal<SurrealClient>, docs: Vec<(String, String)>) -> Result<()> {
    for (id, text) in docs {
        // 1) Compute the embedding
        let vec = get_embeddings(&[text.clone()])?[0].clone();

        // 2) Build the record
        let rec = Doc {
            id: id.clone(),
            text: text.clone(),
            embedding: vec,
        };

        // 3) Run a raw CREATE query, moving `rec` into the bind
        db.query("CREATE docs CONTENT $rec")
            .bind(("rec", rec))
            .await?;
    }
    Ok(())
}

/// Retrieve the `top_k` most‐similar documents for `query`.
pub async fn retrieve(db: &Surreal<SurrealClient>, query: &str, top_k: usize) -> Result<Vec<Doc>> {
    let q_vec = get_embeddings(&[query.to_string()])?[0].clone();
    let sql = format!(
        "SELECT id, text, embedding FROM docs \
         ORDER vector::distance(embedding, $q) ASC LIMIT {top_k}"
    );
    let mut resp = db.query(sql).bind(("q", q_vec)).await?;
    let docs: Vec<Doc> = resp.take(0)?;
    Ok(docs)
}

/// Build a RAG prompt and call Ollama’s HTTP `/chat` endpoint.
pub async fn generate_answer(
    question: &str,
    contexts: &[String],
    ollama_url: &str,
    llm_model: &str,
) -> Result<String> {
    // 1) Assemble prompt
    let mut prompt = String::new();
    for ctx in contexts {
        writeln!(prompt, "Context: {ctx}\n")?;
    }
    write!(prompt, "Question: {question}\nAnswer:")?;

    // 2) POST to Ollama
    let client = Client::new();
    let resp = client
        .post(format!("{ollama_url}/chat"))
        .json(&serde_json::json!({
            "model": llm_model,
            "prompt": prompt,
        }))
        .send()
        .await?
        .text()
        .await?;
    Ok(resp)
}
