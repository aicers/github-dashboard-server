mod conf;
mod database;
mod github;
mod graphql;
mod web;

use conf::{load_config, parse_socket_addr};
use database::Database;
use github::send_github_issue_query;
use std::env;
use std::process::exit;

const DB_TREE_NAME: &str = "issues";

#[tokio::main]
async fn main() {
    println!("AICE GitHub Dashboard Server");

    let mut usage_args = env::args();

    let path = if let Some(args_val) = usage_args.nth(1) {
        match args_val.as_str() {
            "-V" | "--version" => {
                println!("{} {}", conf::PKG_NAME, conf::PKG_VER);
                exit(0);
            }
            "-h" | "--help" => {
                println!("{}", conf::USG);
                exit(0);
            }
            _ => args_val,
        }
    } else {
        eprintln!("No file name given. Refer to usage. \n{}", conf::USG);
        exit(1);
    };

    let config = match load_config(path.as_ref()) {
        Ok(ret) => ret,
        Err(error) => {
            eprintln!("Problem while loading config. {}", error);
            exit(1);
        }
    };

    let socket_addr = match parse_socket_addr(&config.web.address) {
        Ok(ret) => ret,
        Err(error) => {
            eprintln!("Problem while parsing socket address. {}", error);
            exit(1);
        }
    };

    let database = match Database::connect(&config.database.db_name, DB_TREE_NAME) {
        Ok(ret) => ret,
        Err(error) => {
            eprintln!("Problem while Connect Sled Database. {}", error);
            exit(1);
        }
    };

    match send_github_issue_query(
        &config.repository.owner,
        &config.repository.name,
        &config.certification.token,
    )
    .await
    {
        Ok(resp) => {
            if let Err(error) = database.insert_issues(resp) {
                eprintln!("Problem while insert Sled Database. {}", error);
            }
        }
        Err(error) => {
            eprintln!("Problem while sending github query. {}", error);
        }
    }

    let schema = graphql::schema(database);
    web::serve(schema, socket_addr).await;
}
