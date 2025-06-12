mod embeddings;
mod rag;
mod settings;

use anyhow::Result;
use clap::Parser;
use settings::{Args, Settings};
use surrealdb::engine::remote::http::{Client as SurrealClient, Http};
use surrealdb::Surreal;
#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct Document {
    id: String,
    text: String,
    embedding: Vec<f32>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let settings = Settings::from_file(&args.config)?;
    println!("🔍 RAG settings: {:?}", settings.rag);
    println!("Starting on {}", settings.web.address);

    // 1) init the HTTP‐client‐backed Surreal handle…
    let db: Surreal<surrealdb::engine::remote::http::Client> = Surreal::init();
    // 2) connect with the HTTP engine, passing the *full* URL string
    db.connect::<surrealdb::engine::remote::http::Http>(&settings.rag.surreal_url)
        .await?;
    // :contentReference[oaicite:0]{index=0}
    // 4) Sign-in as root/namespace if required
    if !settings.rag.surreal_user.is_empty() {
        db.signin(surrealdb::opt::auth::Root {
            username: &settings.rag.surreal_user,
            password: &settings.rag.surreal_pass,
        })
        .await?;
    }
    // 5) Select your NS/DB
    db.use_ns(&settings.rag.namespace)
        .use_db(&settings.rag.database)
        .await?;

    // (Optional) spawn your GitHub-fetch tasks here…

    // 6) Ingest example docs
    let docs = vec![
        ("doc1".into(), "Hello from doc one".into()),
        ("doc2".into(), "Another document text".into()),
    ];
    rag::ingest(&db, docs).await?;

    // 7) Sample RAG query + Ollama call
    let question = "How many issues were opened by danbi2990?";
    let retrieved = rag::retrieve(&db, question, 3).await?;
    let contexts: Vec<String> = retrieved.into_iter().map(|d| d.text).collect();
    let answer = rag::generate_answer(
        question,
        &contexts,
        &settings.rag.ollama_url,
        &settings.rag.llm_model,
    )
    .await?;
    println!("\nAnswer: {answer}");

    // (Optional) Start your web server here…

    Ok(())
}
