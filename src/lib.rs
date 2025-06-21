use std::{sync::Arc, time::Duration};

use axum::{
    Router,
    body::Bytes,
    extract::DefaultBodyLimit,
    routing::{RouterIntoService, get, post},
};
use backon::{ExponentialBuilder, Retryable};
use color_eyre::eyre::WrapErr;
use hyper::body::Incoming;
use hyper_util::service::TowerToHyperService;
use rand::RngCore;
use russh::{client, keys::PrivateKey};
use tracing::{debug, error, info};

mod routes;
mod ssh;

use crate::{
    routes::{echo_handler, get_handler, post_handler, ws_handler},
    ssh::TcpForwardSession,
};

/* Router definitions */

type RouterService = TowerToHyperService<RouterIntoService<Incoming>>;

/// A lazily-created Router, to be used by the SSH client tunnels.
pub fn get_router(max_data_size: usize) -> RouterService {
    let mut data = vec![0u8; max_data_size];
    rand::rng().fill_bytes(&mut data);
    TowerToHyperService::new(
        Router::new()
            .route("/get/{file_size}", get(get_handler))
            .with_state(Bytes::from_static(data.leak()))
            .route(
                "/post/{file_size}",
                post(post_handler).layer(DefaultBodyLimit::max(max_data_size)),
            )
            .route("/echo", post(echo_handler))
            .route("/ws", get(ws_handler))
            .into_service(),
    )
}

/// Begins remote port forwarding (reverse tunneling) with Russh to serve an Axum application.
pub async fn ssh_entrypoint(
    host: &str,
    port: u16,
    login_name: &str,
    key: Arc<PrivateKey>,
    service: RouterService,
) -> color_eyre::Result<()> {
    let config = Arc::new(client::Config {
        ..Default::default()
    });
    loop {
        let connect = async || {
            TcpForwardSession::connect_key(
                host,
                port,
                login_name,
                Arc::clone(&key),
                Arc::clone(&config),
                service.clone(),
            )
            .await
        };
        let mut session = connect
            .retry(
                ExponentialBuilder::default()
                    .with_jitter()
                    .with_max_delay(Duration::from_secs(20)),
            )
            .await
            .wrap_err_with(|| "SSH connection failed.")?;
        match session.start_forwarding().await {
            Err(e) => error!(error = ?e, "TCP forward session failed."),
            _ => info!("Connection closed."),
        }
        debug!("Attempting graceful disconnect.");
        if let Err(e) = session.close().await {
            debug!(error = ?e, "Graceful disconnect failed.")
        }
        debug!("Restarting connection.");
    }
}
