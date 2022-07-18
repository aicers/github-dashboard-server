mod conf;
mod github;
mod graphql;
mod web;

use conf::{load_info, parse_socket_addr};
use github::send_github_query;
use std::env;
use std::process::exit;

#[tokio::main]
async fn main() {
    println!("AICE GitHub Dashboard Server");

    let args = env::args();
    let error: &str = "USAGE:
    github-dashboard-server <CONFIG>
    
    FLAGS:
        -h, --help       Prints help information
        -V, --version    Prints version information

    ARG:
        <CONFIG>    A TOML config file";

    let config = match load_info(args) {
        Ok(ret) => ret,
        Err(_error) => {
            eprintln!("{}", error);
            exit(1);
        }
    };

    let socket_addr = match parse_socket_addr(&config.web.address) {
        Ok(ret) => ret,
        Err(_error) => {
            println!("{}", error);
            exit(1);
        }
    };

    if let Err(e) = send_github_query(&config.repository.owner, &config.repository.name).await {
        panic!("{:?}", e);
    }

    let schema = graphql::schema();
    web::serve(schema, socket_addr).await;
}
