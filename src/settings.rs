use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
};

use clap::Parser;
use config::{builder::DefaultState, ConfigBuilder, ConfigError, File};
use serde::{Deserialize, Serialize};

const DEFAULT_ADDR: &str = "127.0.0.1:8000";

#[derive(Parser, Debug)]
#[command(version)]
pub struct Args {
    /// Path to the local configuration TOML file.
    #[arg(short, value_name = "CONFIG_PATH")]
    pub config: std::path::PathBuf,

    /// Path to the certificate file.
    #[arg(long, value_name = "CERT_PATH")]
    pub cert: std::path::PathBuf,

    /// Path to the key file.
    #[arg(long, value_name = "KEY_PATH")]
    pub key: std::path::PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Web {
    #[serde(deserialize_with = "deserialize_socket_addr")]
    pub address: SocketAddr,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RagSettings {
    pub surreal_url: String,
    pub surreal_user: String,
    pub surreal_pass: String,
    pub namespace: String,
    pub database: String,
    pub ollama_url: String,
    pub embed_model: String, // <-- newly added
    pub llm_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
pub struct Settings {
    pub web: Web,
    pub rag: RagSettings,
    pub(crate) repositories: Vec<Repository>,
    pub(crate) certification: Certification,
    pub(crate) database: Database,
}

impl Settings {
    /// Load settings from the given TOML file, with sane defaults.
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let builder = ConfigBuilder::<DefaultState>::default()
            .set_default("web.address", DEFAULT_ADDR)?
            .set_default("rag.surreal_url", "http://localhost:8000")?
            .set_default("rag.surreal_user", "root")?
            .set_default("rag.surreal_pass", "root")?
            .set_default("rag.namespace", "test")?
            .set_default("rag.database", "test")?
            .set_default("rag.ollama_url", "http://127.0.0.1:11434")?
            .set_default("rag.llm_model", "ollama")?;

        let cfg = builder.add_source(File::from(path)).build()?;

        cfg.try_deserialize()
    }
}

fn deserialize_socket_addr<'de, D>(deserializer: D) -> Result<SocketAddr, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    s.parse().map_err(serde::de::Error::custom)
}
