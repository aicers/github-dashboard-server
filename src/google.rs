use anyhow::{Context, Result};
use reqwest::{Client, Response};

use crate::database::Database;

const GOOGLE_PUBLIC_KEY: &str = "https://www.googleapis.com/oauth2/v3/certs";
const APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

pub(super) async fn check_key(db: &Database) -> Result<()> {
    let public_key = get_key()
        .await
        .context("Problem with API call")?
        .text()
        .await?;
    db.insert_db("google_key", public_key)?;
    Ok(())
}

async fn get_key() -> Result<Response> {
    let client = Client::builder().user_agent(APP_USER_AGENT).build()?;
    let client = client.get(GOOGLE_PUBLIC_KEY).send().await?;
    Ok(client)
}
