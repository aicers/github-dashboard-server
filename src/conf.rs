use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::Path;

use anyhow::{anyhow, bail, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct ServerAddr {
    pub(super) address: String,
    pub(super) key: String,
    pub(super) cert: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct RepoInfo {
    pub(super) owner: String,
    pub(super) name: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct Certification {
    pub(super) token: String,
    pub(super) ssh: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct Database {
    pub(super) db_path: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct Config {
    pub(super) web: ServerAddr,
    pub(super) repositories: Vec<RepoInfo>,
    pub(super) certification: Certification,
    pub(super) database: Database,
}

pub(super) const PKG_NAME: &str = env!("CARGO_PKG_NAME");

pub(super) fn load_config(path: &Path) -> Result<Config> {
    let mut config_str = String::new();

    if let Err(e) = File::open(path).and_then(|mut f| f.read_to_string(&mut config_str)) {
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
}

pub(super) fn parse_socket_addr(addr_str: &str) -> Result<SocketAddr> {
    let socket_addr: SocketAddr;
    if let Ok(mut addr_iter) = addr_str.to_socket_addrs() {
        if let Some(s_addr) = addr_iter.next() {
            socket_addr = s_addr;
            return Ok(socket_addr);
        }
    }
    Err(anyhow!("Failed to convert socket address"))
}
