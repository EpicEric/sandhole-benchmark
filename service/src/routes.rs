use std::sync::{
    Arc,
    atomic::{AtomicU16, Ordering},
};

use axum::{
    body,
    extract::{Path, State, WebSocketUpgrade},
    response::IntoResponse,
};
use bytes::Bytes;
use futures::StreamExt;
use hyper::StatusCode;

/* Endpoints handling */

pub(crate) async fn get_handler(
    Path(file_size): Path<usize>,
    State(data): State<(Bytes, Arc<AtomicU16>)>,
) -> impl IntoResponse {
    if file_size > data.0.len() {
        StatusCode::BAD_REQUEST.into_response()
    } else {
        let pad: usize = data.1.fetch_add(1, Ordering::AcqRel).into();
        data.0.slice(pad..file_size + pad).into_response()
    }
}

pub(crate) async fn post_handler(
    Path(file_size): Path<usize>,
    body: body::Bytes,
) -> impl IntoResponse {
    if file_size == body.len() {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::BAD_REQUEST
    }
}

/* WebSocket handling */

pub(crate) async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(|mut socket| async move {
        loop {
            match socket.next().await {
                Some(Ok(message)) => {
                    if socket.send(message).await.is_err() {
                        break;
                    }
                }
                _ => break,
            }
        }
    })
}
