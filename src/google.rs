use anyhow::{bail, Result};
use reqwest::{Client, Response};

use crate::database::Database;

const GOOGLE_PUBLIC_KEY: &str = "https://www.googleapis.com/oauth2/v3/certs";
const APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

pub async fn check_key(db: &Database) -> Result<bool> {
    let result = get_key().await;
    match result {
        Ok(response) => {
            let response_body = response.text().await;
            let current_key = db.select("google_key");
            match (response_body, current_key) {
                (Ok(body), Ok(value)) => {
                    if body == value {
                        Ok(true)
                    } else {
                        let remove_key = db.delete("google_key");
                        let insert_key = db.insert("google_key", &body);
                        match (remove_key, insert_key) {
                            (Ok(_), Ok(_)) => Ok(true),
                            (Ok(_), Err(err)) => {
                                eprintln!("Problem inserting the updated Google key");
                                Err(err)
                            }
                            (Err(err), Ok(_)) => {
                                eprintln!("Problem removing the old Google key");
                                Err(err)
                            }
                            (Err(_), Err(_)) => bail!("Problem switching Google key"),
                        }
                    }
                }
                (Err(_), Ok(_)) => {
                    bail!("Problem with getting a response")
                }
                (Ok(body), Err(_)) => {
                    let insert_key = db.insert("google_key", &body);
                    match insert_key {
                        Ok(_) => Ok(true),
                        Err(err) => Err(err),
                    }
                }
                (Err(_), Err(_)) => {
                    bail!("Problem with both response and db")
                }
            }
        }
        Err(err) => {
            eprintln!("Problem with API call");
            Err(err)
        }
    }
}

async fn get_key() -> Result<Response> {
    let client = Client::builder().user_agent(APP_USER_AGENT).build()?;
    let client = client.get(GOOGLE_PUBLIC_KEY).send().await?;
    Ok(client)
}
