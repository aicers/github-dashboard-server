mod conf;
mod database;
mod github;
mod graphql;
mod web;

use crate::conf::PKG_NAME;
use conf::{load_config, parse_socket_addr};
use database::Database;
use directories::ProjectDirs;
use github::{send_github_issue_query, send_github_pr_query};
use std::path::PathBuf;
use std::process::exit;
use std::{env, iter::zip};
use tokio::{task, time};

const DEFAULT_CONFIG: &str = "config.toml";
const ISSUE_TREE_NAME: &str = "issues";
const PR_TREE_NAME: &str = "pull_requests";
const ONE_HOUR: u64 = 60 * 60;
const ORGANIZATION: &str = "einsis";
const QUALIFIER: &str = "com";

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
            _ => PathBuf::from(args_val),
        }
    } else if let Some(proj_dirs) = ProjectDirs::from(QUALIFIER, ORGANIZATION, PKG_NAME) {
        proj_dirs.config_dir().join(DEFAULT_CONFIG)
    } else {
        eprintln!(
            "No valid home directory path. Refer to usage. \n{}",
            conf::USG
        );
        exit(1);
    };

    let config = match load_config(&path) {
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
    let trees = vec![ISSUE_TREE_NAME, PR_TREE_NAME];
    let database = match Database::connect(&config.database.db_name, &trees) {
        Ok(ret) => ret,
        Err(error) => {
            eprintln!("Problem while Connect Sled Database. {}", error);
            exit(1);
        }
    };
    let db = database.clone();

    task::spawn(async move {
        let mut itv = time::interval(time::Duration::from_secs(ONE_HOUR));
        loop {
            itv.tick().await;
            match send_github_issue_query(
                &config.repository.owner,
                &config.repository.names,
                &config.certification.token,
            )
            .await
            {
                Ok(resps) => {
                    for (name, resp) in zip(&config.repository.names, resps) {
                        if let Err(error) = db.insert_issues(resp, &config.repository.owner, name) {
                            eprintln!("Problem while insert Sled Database. {}", error);
                        }
                    }
                }
                Err(error) => {
                    eprintln!("Problem while sending github query. {}", error);
                }
            }
            match send_github_pr_query(
                &config.repository.owner,
                &config.repository.names,
                &config.certification.token,
            )
            .await
            {
                Ok(resps) => {
                    for (name, resp) in zip(&config.repository.names, resps) {
                        if let Err(error) = db.insert_prs(resp, &config.repository.owner, name) {
                            eprintln!("Problem while insert Sled Database. {}", error);
                        }
                    }
                }
                Err(error) => {
                    eprintln!("Problem while sending github query. {}", error);
                }
            }
        }
    });
    let schema = graphql::schema(database);
    web::serve(schema, socket_addr).await;
}
