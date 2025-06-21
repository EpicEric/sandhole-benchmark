use std::{path::PathBuf, sync::Arc};

use clap::Parser;
use russh::keys::load_secret_key;
use sandhole_benchmark::{get_router, ssh_entrypoint};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(clap::Parser)]
pub struct Config {
    host: String,

    #[clap(long, short, default_value_t = 22)]
    port: u16,

    #[clap(long, short = 'l', default_value = "sandhole-benchmark")]
    username: String,

    #[clap(long, short = 'i')]
    private_key: PathBuf,

    #[clap(long, short = 'd', default_value_t = 100_000_000)]
    max_data_size: usize,
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
    // ROUTER.set(get_router(config.max_data_size).await).unwrap();
    ssh_entrypoint(
        &config.host,
        config.port,
        &config.username,
        Arc::new(load_secret_key(config.private_key, None)?),
        get_router(config.max_data_size),
    )
    .await
}
