mod checkout;
mod database;
mod embeddings;
mod github;
mod google;
mod graphql;
mod rag;
mod settings;
mod web;

use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use database::Database;
use google::check_key;
use rag::init_rag_schema;
use settings::{Args, Settings};
use surrealdb::engine::remote::http::{Client as SurrealClient, Http};
use surrealdb::Surreal;
use tokio::{task, time};

const FIVE_MIN: u64 = 60 * 5;
const ONE_HOUR: u64 = 60 * 60;
const ONE_DAY: u64 = ONE_HOUR * 24;

#[tokio::main]
async fn main() -> Result<()> {
    println!("AICE GitHub Dashboard Server");

    let args = Args::parse();
    let settings = Settings::from_file(&args.config)
        .context("Failed to parse config file, Please check file contents")?;

    let repositories = Arc::new(settings.repositories);

    let database = Database::connect(&settings.database.db_path)
        .context("Problem while Connect Sled Database.")?;

    check_key(&database.clone())
        .await
        .context("Problem while checking for public Google key.")?;

    tracing_subscriber::fmt::init();

    // Set up SurrealDB for RAG
    let db_rag: Surreal<SurrealClient> = Surreal::new::<Http>(&settings.rag.surreal_url)
        .await
        .context("Failed to connect to SurrealDB HTTP endpoint")?;

    if !settings.rag.surreal_user.is_empty() {
        db_rag
            .signin(surrealdb::opt::auth::Root {
                username: &settings.rag.surreal_user,
                password: &settings.rag.surreal_pass,
            })
            .await
            .context("Failed to sign in to SurrealDB")?;
    }

    db_rag
        .use_ns(&settings.rag.namespace)
        .use_db(&settings.rag.database)
        .await
        .context("Failed to select SurrealDB namespace/database")?;

    // ✅ Define schema for RAG
    init_rag_schema(&db_rag)
        .await
        .context("Failed to initialize RAG schema")?;

    // 🔁 Manual fetch + ingestion test
    let docs = github::fetch_issues(
        "aicers",
        "github-dashboard-server",
        &settings.certification.token,
    )
    .await
    .context("GitHub issue fetch failed")?;

    embeddings::ingest(
        &db_rag,
        docs,
        &settings.rag.ollama_url,
        &settings.rag.embed_model,
    )
    .await
    .context("RAG ingest failed")?;

    // 🧪 Demo: Ask a question
    let question = "How many issues were opened by danbi2990?";
    let retrieved = embeddings::retrieve(
        &db_rag,
        question,
        3,
        &settings.rag.ollama_url,
        &settings.rag.embed_model,
    )
    .await
    .context("Failed to retrieve from SurrealDB")?;

    let contexts: Vec<String> = retrieved.into_iter().map(|d| d.text).collect();

    let answer = embeddings::generate_answer(
        question,
        &contexts,
        &settings.rag.ollama_url,
        &settings.rag.llm_model,
    )
    .await
    .context("Failed to generate answer")?;

    println!("\nGenerated Answer: {answer}");

    // Spawn background tasks
    task::spawn(github::fetch_periodically(
        Arc::clone(&repositories),
        settings.certification.token,
        time::Duration::from_secs(ONE_HOUR),
        time::Duration::from_secs(FIVE_MIN),
        database.clone(),
    ));

    task::spawn(checkout::fetch_periodically(
        Arc::clone(&repositories),
        time::Duration::from_secs(ONE_DAY),
        settings.certification.ssh,
    ));

    let schema = graphql::schema(database);
    web::serve(schema, settings.web.address, &args.key, &args.cert).await;

    Ok(())
}
