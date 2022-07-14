mod conf;
mod graphql;
mod web;

use conf::{load_info, parse_socket_addr};
use std::env;

#[tokio::main]
async fn main() {
    println!("AICE GitHub Dashboard Server");

    let args = env::args();

    let config = match load_info(args) {
        Ok(ret) => ret,
        Err(e) => {
            panic!("{:?}", e);
        }
    };

    let socket_addr = match parse_socket_addr(&config.web.address) {
        Ok(ret) => ret,
        Err(e) => {
            panic!("{:?}", e);
        }
    };
    let schema = graphql::schema();
    web::serve(schema, socket_addr).await;
}
