mod checkout;
mod database;
mod github;
mod google;
mod graphql;
mod settings;
mod web;

use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use database::Database;
use google::check_key;
use settings::{Args, Settings};
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

    // Fetches issues and pull requests from GitHub every hour, and stores them
    // in the database.
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
