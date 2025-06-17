use std::fmt::Write;

use anyhow::{Context, Result};
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use surrealdb::engine::remote::http::Client as SurrealClient;
use surrealdb::sql::Thing;
use surrealdb::Surreal;

#[derive(Debug, Serialize, Deserialize)]
pub struct Doc {
    pub id: Thing,
    pub text: String,
    pub embedding: Vec<f32>,
}

pub async fn ingest(
    db: &Surreal<SurrealClient>,
    docs: Vec<(String, String)>,
    ollama_url: &str,
    embed_model: &str,
) -> Result<()> {
    for (id, text) in docs {
        let vecs = get_embeddings_ollama(&text, ollama_url, embed_model).await?;

        let rec = serde_json::json!({
            "id": id,
            "text": text,
            "embedding": vecs
        });

        db.query("CREATE docs CONTENT $rec")
            .bind(("rec", rec))
            .await?;
    }
    Ok(())
}

pub async fn retrieve(
    db: &Surreal<SurrealClient>,
    query: &str,
    top_k: usize,
    ollama_url: &str,
    embed_model: &str,
) -> Result<Vec<Doc>> {
    let query_embedding = get_embeddings_ollama(query, ollama_url, embed_model).await?;

    let mut docs: Vec<Doc> = db
        .query("SELECT id, text, embedding FROM docs;")
        .await?
        .take(0)?;

    docs.sort_by(|a, b| {
        cosine_distance(&query_embedding, &a.embedding)
            .partial_cmp(&cosine_distance(&query_embedding, &b.embedding))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(docs.into_iter().take(top_k).collect())
}

fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    1.0 - (dot / (norm_a * norm_b + 1e-8))
}

#[allow(clippy::cast_possible_truncation)]
async fn get_embeddings_ollama(input: &str, ollama_url: &str, model: &str) -> Result<Vec<f32>> {
    let client = Client::new();
    let resp = client
        .post(format!("{ollama_url}/api/embeddings"))
        .json(&serde_json::json!({
            "model": model,
            "prompt": input
        }))
        .send()
        .await
        .context("Failed to contact Ollama for embeddings")?
        .json::<serde_json::Value>()
        .await
        .context("Failed to parse Ollama response")?;

    let embedding = resp
        .get("embedding")
        .and_then(|v| v.as_array())
        .context("Missing 'embedding' array in Ollama response")?
        .iter()
        .map(|v| v.as_f64().unwrap_or(0.0) as f32)
        .collect();

    Ok(embedding)
}

pub async fn generate_answer(
    question: &str,
    contexts: &[String],
    ollama_url: &str,
    llm_model: &str,
) -> Result<String> {
    let mut prompt = String::new();
    for ctx in contexts {
        writeln!(prompt, "Context: {ctx}\n")?;
    }
    write!(
        prompt,
        "Give a short, direct answer with no explanation. Only return the answer.\nQuestion: {question}\nAnswer:"
    )?;

    let client = Client::new();
    let resp = client
        .post(format!("{ollama_url}/api/generate"))
        .json(&serde_json::json!({
            "model": llm_model,
            "prompt": prompt,
            "stream": true
        }))
        .send()
        .await
        .context("Failed to contact Ollama for generation")?;

    let mut output = String::new();
    let mut stream = resp.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("Failed to read stream chunk")?;
        if let Ok(text) = std::str::from_utf8(&chunk) {
            for line in text.lines() {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                    if let Some(fragment) = json.get("response").and_then(|v| v.as_str()) {
                        output.push_str(fragment);
                    }
                }
            }
        }
    }

    Ok(output.trim().to_string())
}
