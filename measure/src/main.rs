use clap::Parser;
use sandhole_benchmark_measure::{Endpoint, EntrypointConfig, entrypoint};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(clap::Parser)]
pub struct Config {
    base_url: String,

    #[arg(long, short, value_enum, default_value_t = Endpoint::Get)]
    endpoint: Endpoint,

    #[arg(long, short, default_value_t = 10_000_000)]
    size: usize,

    #[arg(long, short, default_value_t = 1)]
    concurrency: usize,
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
    entrypoint(EntrypointConfig {
        base_url: config.base_url,
        endpoint: config.endpoint,
        size: config.size,
        concurrency: config.concurrency,
    })
    .await
}
