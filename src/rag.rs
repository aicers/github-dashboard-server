use anyhow::{Context, Result};
use surrealdb::{engine::remote::http::Client, Surreal};

pub async fn init_rag_schema(db: &Surreal<Client>) -> Result<()> {
    let define_schema = r"
        DEFINE TABLE docs SCHEMAFULL;
        DEFINE FIELD id        ON docs TYPE string;
        DEFINE FIELD text      ON docs TYPE string;
        DEFINE FIELD embedding ON docs TYPE array<float>;
    ";

    db.query(define_schema)
        .await
        .context("Failed to define RAG schema")?;
    Ok(())
}
