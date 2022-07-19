use anyhow::{anyhow, bail, Result};
use serde::Deserialize;
use std::env;
use std::env::Args;
use std::fs;
use std::net::{SocketAddr, ToSocketAddrs};
use std::process::exit;

#[derive(Debug, Deserialize)]
pub struct ServerAddr {
    pub address: String,
}

#[derive(Debug, Deserialize)]
pub struct RepoInfo {
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub web: ServerAddr,
    pub repository: RepoInfo,
}

const PKG_NAME: &str = env!("CARGO_PKG_NAME");
const PKG_VER: &str = env!("CARGO_PKG_VERSION");

pub fn load_info(mut args: Args) -> Result<Config> {
    let _config_str = String::new();

    if let Some(args_val) = args.nth(1) {
        match args_val.as_str() {
            "-V" | "-version" => {
                println!("{} {}", PKG_NAME, PKG_VER);
                exit(0);
            }
            "-h" | "-help" => {
                println!(
                    "
                    USAGE: 
                        github-dashboard-server <CONFIG>

                    FLAGS:
                        -h, --help       Prints help information
                        -V, --version    Prints version information

                    ARG:
                        <CONFIG>    A TOML config file"
                );
                exit(0);
            }

            default => {
                if default.contains(".toml") {
                    let args: Vec<String> = env::args().collect();

                    let _query = &args[0];
                    let filename = &args[1];

                    let contents = fs::read_to_string(filename)
                        .expect("Something went wrong reading the file");
                    println!("\n{}", contents);

                    exit(0);
                } else {
                    bail!("Failed to load args, Pleash enter correct args value");
                }
            }
        }
    } else {
        bail!("Failed to load args, Please enter the args and run it");
    }
}

pub fn parse_socket_addr(addr_str: &str) -> Result<SocketAddr> {
    let socket_addr: SocketAddr;
    if let Ok(mut addr_iter) = addr_str.to_socket_addrs() {
        if let Some(s_addr) = addr_iter.next() {
            socket_addr = s_addr;
            return Ok(socket_addr);
        }
    }
    Err(anyhow!("Failed to convert socket address"))
}
