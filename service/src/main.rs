use std::{path::PathBuf, sync::Arc};

use clap::Parser;
use russh::{
    cipher::{AES_256_GCM, CHACHA20_POLY1305, Name},
    keys::load_secret_key,
};
use sandhole_benchmark_service::{get_router, ssh_entrypoint};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Clone)]
struct CipherName(Name);

impl ToString for CipherName {
    fn to_string(&self) -> String {
        self.0.as_ref().to_string()
    }
}

#[derive(clap::Parser)]
pub struct Config {
    /// SSH hostname.
    host: String,

    /// SSH port.
    #[arg(long, short, default_value_t = 22)]
    port: u16,

    /// SSH user name.
    #[arg(long, short = 'l', default_value = "sandhole-benchmark")]
    username: String,

    /// SSH private key.
    #[arg(long, short = 'i')]
    private_key: PathBuf,

    /// Maximum data size to handle for GET requests.
    #[arg(long, short = 'd', default_value_t = 100_000_000)]
    max_data_size: usize,

    /// Ciphers to use with SSH.
    #[arg(long, short, value_parser = validate_cipher, default_values_t = vec![CipherName(CHACHA20_POLY1305), CipherName(AES_256_GCM)])]
    cipher: Vec<CipherName>,

    /// Flags to pass via exec.
    #[arg(long, short)]
    exec: Option<String>,
}

fn validate_cipher(value: &str) -> Result<CipherName, String> {
    Name::try_from(value)
        .map(CipherName)
        .map_err(|_| "invalid domain".to_string())
}

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(tracing::level_filters::LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .with(tracing_subscriber::fmt::Layer::default().compact())
        .init();
    color_eyre::install()?;
    let config = Config::parse();
    ssh_entrypoint(
        &config.host,
        config.port,
        &config.username,
        Arc::new(load_secret_key(config.private_key, None)?),
        config
            .cipher
            .into_iter()
            .map(|cipher_name| cipher_name.0)
            .collect(),
        get_router(config.max_data_size),
        config.exec.as_ref().map(String::as_str),
    )
    .await
}
