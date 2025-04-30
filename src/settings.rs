use std::path::Path;
use std::{net::SocketAddr, path::PathBuf};

use anyhow::Result;
use clap::Parser;
use config::{builder::DefaultState, ConfigBuilder, ConfigError, File};
use serde::{de::Error, Deserialize, Deserializer, Serialize};

const DEFAULT_ADDR: &str = "127.0.0.1:8000";
const DEFAULT_DATABASE_NAME: &str = "github-dashboard";

#[derive(Parser, Debug)]
#[command(version)]
pub(crate) struct Args {
    /// Path to the local configuration TOML file.
    #[arg(short, value_name = "CONFIG_PATH")]
    pub(crate) config: PathBuf,

    /// Path to the certificate file.
    #[arg(long, value_name = "CERT_PATH")]
    pub(crate) cert: PathBuf,

    /// Path to the key file.
    #[arg(long, value_name = "KEY_PATH")]
    pub(crate) key: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Web {
    #[serde(deserialize_with = "deserialize_socket_addr")]
    pub(crate) address: SocketAddr,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Repository {
    pub(crate) owner: String,
    pub(crate) name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Certification {
    pub(crate) token: String,
    pub(crate) ssh: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Database {
    pub(crate) db_path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Settings {
    pub(crate) web: Web,
    pub(crate) repositories: Vec<Repository>,
    pub(crate) certification: Certification,
    pub(crate) database: Database,
}

impl Settings {
    /// Creates a new `Settings` instance, populated from the given configuration file.
    pub fn from_file(config_path: &Path) -> Result<Self, ConfigError> {
        let settings = default_config_builder()
            .add_source(File::from(config_path))
            .build()?;

        settings.try_deserialize()
    }
}

fn default_config_builder() -> ConfigBuilder<DefaultState> {
    config::Config::builder()
        .set_default("web.address", DEFAULT_ADDR)
        .expect("valid default address")
        .set_default("database.db_path", DEFAULT_DATABASE_NAME)
        .expect("valid database name")
}

/// Deserializes a socket address.
///
/// # Errors
///
/// Returns an error if the address is not in the form of 'IP:PORT'.
fn deserialize_socket_addr<'de, D>(deserializer: D) -> Result<SocketAddr, D::Error>
where
    D: Deserializer<'de>,
{
    let addr = String::deserialize(deserializer)?;
    addr.parse()
        .map_err(|e| D::Error::custom(format!("invalid address \"{addr}\": {e}")))
}
