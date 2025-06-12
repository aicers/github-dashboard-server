#[allow(dead_code)]
#[path = "../database.rs"]
mod database;
#[allow(dead_code)]
#[path = "../embedder.rs"]
mod embedder;
#[allow(dead_code)]
#[path = "../github.rs"]
mod github;
#[allow(dead_code)]
#[path = "../graphql.rs"]
mod graphql;
#[allow(dead_code)]
#[path = "../llm.rs"]
mod llm;
#[allow(dead_code)]
#[path = "../settings.rs"]
mod settings;
#[allow(dead_code)]
#[path = "../utils.rs"]
mod utils;
#[allow(dead_code)]
#[path = "../vector_db.rs"]
mod vector_db;

use std::{env, path::Path};

use anyhow::Result;
use database::Database;
use embedder::{load_tokenizer, Embedder};
use llm::Localmodel;
use utils::tensor_to_vec;
use vector_db::{
    create_collection, insert_vector_data, render_issue_chunk, render_pr_chunk,
    search_related_chunks, Chunk,
};

#[allow(clippy::too_many_lines)]
#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 {
        eprintln!("Usage: cargo run --bin rag -- <sled-db-path> <owner> <repo>");
        return Ok(());
    }

    let db_path = &args[1];
    let owner = &args[2];
    let repo = &args[3];

    // 1) Open the sled DB
    let db = Database::connect(Path::new(db_path))?;

    // 2) Collect text chunks from issues and PRs
    let mut chunks = Vec::new();
    let prefix = format!("{owner}/{repo}#");
    for issue_res in db.issues(Some(prefix.as_bytes()), None) {
        let issue = issue_res?;
        chunks.push(render_issue_chunk(&issue));
    }
    for pr_res in db.pull_requests(Some(prefix.as_bytes()), None) {
        let pr = pr_res?;
        chunks.push(render_pr_chunk(&pr));
    }
    // Close sled DB to release the lock before embedding/upsert
    drop(db);

    // 3) Initialize tokenizer and embedder
    let tokenizer = load_tokenizer(
        "/Users/kiwonchung/bert_models/gte-modernbert-base/tokenizer.json",
        "/Users/kiwonchung/bert_models/gte-modernbert-base/special_tokens_map_2.json",
    )?;
    let environment = Embedder::create_environment()?;
    let embedder = Embedder::load_model(
        "/Users/kiwonchung/bert_models/gte-modernbert-base/onnx",
        &environment,
    )?;

    // 4) Create Qdrant collection (skip if already exists)
    let mut need_upsert = true;
    match create_collection(768).await {
        Ok(()) => println!("Creating Qdrant collection."),
        Err(err) => {
            let msg = err.to_string();
            if msg.contains("already exists") {
                println!("Qdrant collection already exists; skipping vector upsert altogether.");
                need_upsert = false;
            } else {
                return Err(err);
            }
        }
    }
    let mut rag_llm = Localmodel::new("qwen3:8b", owner, repo);
    // 5) Embed and upsert each chunk
    if need_upsert {
        for (idx, chunk) in chunks.iter().enumerate() {
            let point_id: u64 = idx
                .try_into()
                .map_err(|_| anyhow::anyhow!("chunk index {} doesn't fit in u64", idx))?;

            // 2) usize → u16
            let idx_: u16 = idx
                .try_into()
                .map_err(|_| anyhow::anyhow!("chunk index {} doesn't fit in u16", idx))?;

            // 1) Extract a &str to embed from the enum
            let text_for_embedding: &str = match chunk {
                Chunk::Issue(ic) => &ic.content,
                Chunk::PullRequest(pc) => &pc.content,
            };

            // 2) Encode it
            let tensor = embedder.encode_texts(&tokenizer, &[text_for_embedding])?;
            let normalized = Embedder::normalize_embeddings(&tensor);
            let vec = tensor_to_vec(&normalized)?;

            // 3) Upsert, passing the same &str
            // in your upsert loop…
            insert_vector_data(
                point_id, // u64
                format!("{owner}/{repo}/chunk_{idx}"),
                idx_,
                text_for_embedding,
                vec,
                &format!("{owner}/{repo}"),
                Some(chunk), // Option<serde_json::Value> ─ carries title/author/url/etc.
            )
            .await?;
        }
    }

    // 6) Interactive QA loop
    loop {
        print!("Enter your question (or 'exit' to quit): ");
        std::io::Write::flush(&mut std::io::stdout())?;

        let mut question = String::new();
        std::io::stdin().read_line(&mut question)?;
        let question = question.trim();
        if question.eq_ignore_ascii_case("exit") {
            break;
        }

        // 9) Retrieve top-3 chunks
        let hits = search_related_chunks(&embedder, &tokenizer, question, 5).await?;
        let contexts: Vec<String> = hits.into_iter().map(|v| v.content_chunk).collect();

        // *** DEBUG: direct database count for author ***
        // Re-open DB to count
        let debug_db = Database::connect(Path::new(db_path))?;
        let direct_count = debug_db.count_issues_by_author(owner, repo, "danbi2990");
        println!("[DEBUG] Direct DB count for author '{question}': {direct_count}");
        drop(debug_db);

        // 10) Build prompt and query
        let prompt = Localmodel::combine_question_and_chunks(question, &contexts);
        println!("--- Prompt to LLM ---\n{prompt}\n");
        let answer = rag_llm.generate_response(&prompt).await?;
        println!("LLM Response:\n{answer}");
    }

    Ok(())
}
