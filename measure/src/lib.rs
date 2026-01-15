use std::{fmt::Display, net::SocketAddr, path::PathBuf, sync::Arc, time::Instant};

use bytes::Bytes;
use futures::{SinkExt, TryStreamExt, future::try_join_all};
use rand::RngCore;
use reqwest_websocket::RequestBuilderExt;
use rustls::{
    ClientConfig, RootCertStore,
    pki_types::{CertificateDer, pem::PemObject},
};
use tracing::{info, instrument};

#[derive(Debug, Copy, Clone, PartialEq, Eq, clap::ValueEnum)]
pub enum Endpoint {
    Get,
    Post,
    Websocket,
}

impl Display for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Endpoint::Get => "GET",
            Endpoint::Post => "POST",
            Endpoint::Websocket => "WebSocket",
        })
    }
}

pub struct EntrypointConfig {
    pub base_url: String,
    pub endpoint: Endpoint,
    pub size: usize,
    pub concurrency: usize,
    pub custom_ca_cert: Option<PathBuf>,
    pub host_ip: Option<SocketAddr>,
}

pub async fn entrypoint(
    EntrypointConfig {
        base_url,
        endpoint,
        size,
        concurrency,
        custom_ca_cert,
        host_ip: host,
    }: EntrypointConfig,
) -> color_eyre::Result<()> {
    let base_url: &'static str = base_url
        .leak()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches("/");
    let mut jhs = Vec::with_capacity(concurrency);
    let initial_data = match endpoint {
        Endpoint::Get => Bytes::new(),
        Endpoint::Post | Endpoint::Websocket => {
            let mut buf = vec![0u8; size];
            rand::rng().fill_bytes(&mut buf);
            Bytes::from(buf)
        }
    };
    let config = custom_ca_cert
        .map(|path| {
            CertificateDer::pem_file_iter(path)
                .and_then(|iter| iter.collect::<Result<Vec<_>, _>>())
                .map(|certs| {
                    let mut root_store = RootCertStore::empty();
                    root_store.add_parsable_certificates(certs);
                    ClientConfig::builder_with_provider(Arc::new(
                        rustls::crypto::aws_lc_rs::default_provider(),
                    ))
                    .with_safe_default_protocol_versions()
                    .unwrap()
                    .with_root_certificates(root_store)
                    .with_no_client_auth()
                })
        })
        .transpose()?;
    let client = if let Some(config) = config {
        let client = reqwest::Client::builder().tls_backend_preconfigured(config);
        if let Some(host) = host {
            client.resolve(base_url, host).build()?
        } else {
            client.build()?
        }
    } else {
        let client = reqwest::Client::builder().tls_backend_rustls();
        if let Some(host) = host {
            client.resolve(base_url, host).build()?
        } else {
            client.build()?
        }
    };
    info!(%base_url, %endpoint, %size, %concurrency, ?client, "Starting benchmark...");
    let started = Instant::now();
    for _ in 0..concurrency {
        let data = initial_data.clone();
        let client = client.clone();
        let jh = tokio::spawn(async move { handler(base_url, client, endpoint, data, size).await });
        jhs.push(jh);
    }
    try_join_all(jhs.into_iter()).await?;
    let elapsed = started.elapsed();
    info!(
        elapsed = humantime::format_duration(elapsed).to_string(),
        "Benchmark finished."
    );
    Ok(())
}

#[instrument(level = "debug")]
async fn handler(
    base_url: &str,
    client: reqwest::Client,
    endpoint: Endpoint,
    data: Bytes,
    size: usize,
) -> color_eyre::Result<()> {
    match endpoint {
        Endpoint::Get => {
            client
                .get(format!("https://{base_url}/get/{size}"))
                .send()
                .await?
                .error_for_status()?
                .bytes()
                .await?;
        }
        Endpoint::Post => {
            client
                .post(format!("https://{base_url}/post/{size}"))
                .body(data)
                .send()
                .await?
                .error_for_status()?;
        }
        Endpoint::Websocket => {
            let response = client
                .get(format!("wss://{base_url}/ws"))
                .upgrade()
                .send()
                .await?;
            let mut websocket = response.into_websocket().await?;
            websocket
                .send(reqwest_websocket::Message::Binary(data))
                .await?;
            while let Some(message) = websocket.try_next().await? {
                if let reqwest_websocket::Message::Binary(data) = message {
                    if data.len() == size {
                        break;
                    }
                }
            }
        }
    }
    Ok(())
}
