// mod api;
// mod checkout;
// mod database;
// mod github;
// mod google;
// mod graphql;
mod lang_graph;
// mod settings;
// mod web;
// mod outbound;

use std::sync::Arc;

use anyhow::{Context, Result};

// use clap::Parser;
// use database::Database;
// use google::check_key;
// use settings::{Args, Settings};
// use tokio::{task, time};
use crate::lang_graph::GitHubRAGSystem;

const FIVE_MIN: u64 = 60 * 5;
const ONE_HOUR: u64 = 60 * 60;
const ONE_DAY: u64 = ONE_HOUR * 24;

#[tokio::main]
async fn main() -> Result<()> {
    // println!("AICE GitHub Dashboard Server");
    // let args = Args::parse();
    // let settings = Settings::from_file(&args.config)
    //     .context("Failed to parse config file, Please check file contents")?;

    // let repositories = Arc::new(settings.repositories);

    // let database = Database::connect(&settings.database.db_path)
    //     .context("Problem while Connect Sled Database.")?;

    // check_key(&database.clone())
    //     .await
    //     .context("Problem while checking for public Google key.")?;

    tracing_subscriber::fmt::init();

    let rag_system = GitHubRAGSystem::new().await?;

    // 쿼리 실행
    let queries = [
        // "How many issues were opened last month in rust-lang/rust?",
        // "What are the main discussions about async Rust?",
        // "Which issues received responses from the repo owner within 24 hours, and how often does that happen?",
        // "Identify PRs labeled \"enhancement\" that were merged but not mentioned in any discussion thread.",
        // "Find contributors who reopened their own closed issues and later submitted a pull request to address them.",
        // "Are there issues that remained open for over 60 days but were immediately closed after a new discussion thread was posted?",
        // "Who are the most influential users based on cross-entity activity (issues opened, comments made, PRs merged)?",
        // "Do any users consistently tag issues with labels that differ from the labels applied by maintainers?",
        // "What proportion of closed discussions contain action items that are not tracked in any issues or PRs?",
        // "Are there any spikes in activity (issues, PRs, discussions) that correlate with external events (e.g., release cycles)?",
        // "Identify contributors who initiated a discussion and later implemented the solution via a PR.",
        "Are there contributors whose activity patterns suggest they work on this repo during specific times of day or week?",
    ];

    for query in queries {
        match rag_system.query(query).await {
            Ok(response) => println!("Query: {}\nResponse: {}\n", query, response),
            Err(e) => eprintln!("Error processing query '{}': {}", query, e),
        }
    }

    // Fetches issues and pull requests from GitHub every hour, and stores them
    // in the database.
    // task::spawn(outbound::fetch_periodically(
    //     Arc::clone(&repositories),
    //     settings.certification.token,
    //     time::Duration::from_secs(ONE_HOUR),
    //     time::Duration::from_secs(FIVE_MIN),
    //     database.clone(),
    // ));
    // task::spawn(github::fetch_periodically(
    //     Arc::clone(&repositories),
    //     settings.certification.token,
    //     time::Duration::from_secs(ONE_HOUR),
    //     time::Duration::from_secs(FIVE_MIN),
    //     database.clone(),
    // ));

    // task::spawn(checkout::fetch_periodically(
    //     Arc::clone(&repositories),
    //     time::Duration::from_secs(ONE_DAY),
    //     settings.certification.ssh,
    // ));

    // let schema = graphql::schema(database);

    // web::serve(schema, settings.web.address, &args.key, &args.cert).await;
    Ok(())
}
