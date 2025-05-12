mod checkout;
mod conf;
mod database;
mod github;
mod google;
mod graphql;
mod web;

use std::path::PathBuf;
use std::process::exit;
use std::sync::Arc;

use anyhow::{Context, Result};
use conf::{load_config, parse_socket_addr};
use database::Database;
use directories::ProjectDirs;
use google::check_key;
use tokio::{task, time};

use crate::conf::PKG_NAME;

const USAGE: &str = "\
USAGE:
    github-dashboard-server <CONFIG>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

ARG:
    <CONFIG>    A TOML config file
";

const FIVE_MIN: u64 = 60 * 5;
const DEFAULT_CONFIG: &str = "config.toml";
const ONE_HOUR: u64 = 60 * 60;
const ONE_DAY: u64 = ONE_HOUR * 24;
const ORGANIZATION: &str = "cluml";
const QUALIFIER: &str = "com";

#[tokio::main]
async fn main() -> Result<()> {
    println!("AICE GitHub Dashboard Server");

    let config_filename = if let Some(config_filename) = parse() {
        PathBuf::from(config_filename)
    } else if let Some(proj_dirs) = ProjectDirs::from(QUALIFIER, ORGANIZATION, PKG_NAME) {
        proj_dirs.config_dir().join(DEFAULT_CONFIG)
    } else {
        eprintln!("No valid home directory path. Refer to usage. \n{USAGE}");
        exit(1);
    };

    let config = load_config(&config_filename).context("Problem while loading config.")?;
    let repositories = Arc::new(config.repositories);

    let socket_addr =
        parse_socket_addr(&config.web.address).context("Problem while parsing socket address.")?;

    let database = Database::connect(config.database.db_path.as_ref())
        .context("Problem while Connect Sled Database.")?;

    check_key(&database.clone())
        .await
        .context("Problem while checking for public Google key.")?;

    tracing_subscriber::fmt::init();

    // Fetches issues and pull requests from GitHub every hour, and stores them
    // in the database.
    task::spawn(github::fetch_periodically(
        Arc::clone(&repositories),
        config.certification.token,
        time::Duration::from_secs(ONE_HOUR),
        time::Duration::from_secs(FIVE_MIN),
        database.clone(),
    ));

    task::spawn(checkout::fetch_periodically(
        Arc::clone(&repositories),
        time::Duration::from_secs(ONE_DAY),
        config.certification.ssh,
    ));

    let schema = graphql::schema(database);
    web::serve(schema, socket_addr, &config.web.key, &config.web.cert).await;
    Ok(())
}

/// Parses the command line arguments and returns the first argument.
fn parse() -> Option<String> {
    use std::env;

    let mut args = env::args();
    args.next()?;
    let arg = args.next()?;
    if args.next().is_some() {
        eprintln!("Error: too many arguments");
        exit(1);
    }

    if arg == "--help" || arg == "-h" {
        print!("{USAGE}");
        exit(0);
    }
    if arg == "--version" || arg == "-V" {
        println!("github-dashboard-server {}", env!("CARGO_PKG_VERSION"));
        exit(0);
    }
    if arg.starts_with('-') {
        eprintln!("Error: unknown option: {arg}");
        eprintln!("\n{USAGE}");
        exit(1);
    }

    Some(arg)
}
