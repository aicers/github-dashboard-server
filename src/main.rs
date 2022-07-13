mod conf;
mod web;

use conf::{load_config, parse_socket_addr};
use std::env;

#[tokio::main]
async fn main() {
    println!("AICE GitHub Dashboard Server");
    let args = env::args();
    let config = match load_config(args) {
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
    web::serve(socket_addr).await;
}
