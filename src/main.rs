mod conf;
mod github;
mod graphql;
mod web;

use conf::{load_config, load_usage, parse_socket_addr};
use github::send_github_query;
use std::env;
use std::process::exit;

#[tokio::main]
async fn main() {
    println!("AICE GitHub Dashboard Server");

    let usage_args = env::args();
    let config_args = env::args();

    let _usage_config = match load_usage(usage_args) {
        Ok(ret) => ret,
        Err(error) => {
            eprintln!("Problem while loading configuration toml. {}", error);
            exit(1);
        }
    };

    let config = match load_config(config_args) {
        Ok(ret) => ret,
        Err(error) => {
            eprintln!("Problem while loading usage. {}", error);
            exit(1);
        }
    };

    let socket_addr = match parse_socket_addr(&config.web.address) {
        Ok(ret) => ret,
        Err(error) => {
            println!("Problem while parsing socket address. {}", error);
            exit(1);
        }
    };

    if let Err(error) = send_github_query(
        &config.repository.owner,
        &config.repository.name,
        &config.certification.token,
    )
    .await
    {
        eprintln!("Problem while sending github query. {}", error);
    }

    let schema = graphql::schema();
    web::serve(schema, socket_addr).await;
}
