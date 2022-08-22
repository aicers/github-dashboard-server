mod conf;
mod database;
mod github;
mod google;
mod graphql;
mod web;

use crate::conf::PKG_NAME;
use conf::{load_config, parse_socket_addr};
use database::Database;
use directories::ProjectDirs;
use github::{send_github_issue_query, send_github_pr_query};
use google::check_key;
use std::env;
use std::path::PathBuf;
use std::process::exit;
use tokio::{task, time};

const USAGE: &str = "\
USAGE:
    github-dashboard-server <CONFIG>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

ARG:
    <CONFIG>    A TOML config file
";

const DEFAULT_CONFIG: &str = "config.toml";
const ISSUE_TREE_NAME: &str = "issues";
const PR_TREE_NAME: &str = "pull_requests";
const ONE_HOUR: u64 = 60 * 60;
const ORGANIZATION: &str = "einsis";
const QUALIFIER: &str = "com";

#[allow(clippy::too_many_lines)]
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
                println!("{}", USAGE);
                exit(0);
            }
            _ => PathBuf::from(args_val),
        }
    } else if let Some(proj_dirs) = ProjectDirs::from(QUALIFIER, ORGANIZATION, PKG_NAME) {
        proj_dirs.config_dir().join(DEFAULT_CONFIG)
    } else {
        eprintln!("No valid home directory path. Refer to usage. \n{}", USAGE);
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

    match check_key(&database.clone()).await {
        Ok(ret) => ret,
        Err(error) => {
            eprintln!("Problem while checking for public Google key. {}", error);
            exit(1);
        }
    };
    task::spawn(async move {
        let mut itv = time::interval(time::Duration::from_secs(ONE_HOUR));
        loop {
            itv.tick().await;
            for repoinfo in &config.repositories {
                match send_github_issue_query(
                    &repoinfo.owner,
                    &repoinfo.name,
                    &config.certification.token,
                )
                .await
                {
                    Ok(resps) => {
                        for resp in resps {
                            if let Err(error) =
                                db.insert_issues(resp, &repoinfo.owner, &repoinfo.name)
                            {
                                eprintln!("Problem while insert Sled Database. {}", error);
                            }
                        }
                    }
                    Err(error) => {
                        eprintln!("Problem while sending github query. {}", error);
                    }
                }
                match send_github_pr_query(
                    &repoinfo.owner,
                    &repoinfo.name,
                    &config.certification.token,
                )
                .await
                {
                    Ok(resps) => {
                        for resp in resps {
                            if let Err(error) = db.insert_prs(resp, &repoinfo.owner, &repoinfo.name)
                            {
                                eprintln!("Problem while insert Sled Database. {}", error);
                            }
                        }
                    }
                    Err(error) => {
                        eprintln!("Problem while sending github query. {}", error);
                    }
                }
            }
        }
    });

    let schema = graphql::schema(database);
    web::serve(schema, socket_addr).await;
}
