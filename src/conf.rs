use anyhow::{anyhow, bail, Result};
use serde::Deserialize;
use std::env;
use std::env::Args;
use std::fs::File;
use std::io::prelude::*;
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
pub const USG: &str = "USAGE:
    github-dashboard-server <CONFIG>
    
FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

ARG:
    <CONFIG>    A TOML config file";

pub fn load_info(mut args: Args) -> Result<Config> {
    let mut config_str = String::new();

    if let Some(args_val) = args.nth(1) {
        match args_val.as_str() {
            "-V" | "--version" => {
                println!("{} {}", PKG_NAME, PKG_VER);
                exit(0);
            }
            "-h" | "--help" => {
                println!("{}", USG);
                exit(0);
            }

            default => {
                if default.contains(".toml") {
                    if let Err(e) =
                        File::open(default).and_then(|mut f| f.read_to_string(&mut config_str))
                    {
                        bail!("Failed to open file, Please check file name: {:?}", e);
                    }
                    let config = match toml::from_str::<Config>(&config_str) {
                        Ok(ret) => ret,
                        Err(e) => {
                            bail!(
                                "Failed to parse Toml document, Please check file contents: {:?}",
                                e
                            );
                        }
                    };
                    Ok(config)
                } else {
                    bail!("Failed to load args, Please enter correct args value");
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
