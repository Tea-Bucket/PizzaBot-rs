use axum::{
    extract::{
        connect_info::ConnectInfo, ws::{WebSocket, WebSocketUpgrade}, Request
    }, http::StatusCode, response::IntoResponse, routing::get, Router
};
use axum_extra::TypedHeader;
use tracing::info;

use std::net::SocketAddr;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

async fn run() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "debug,backend=debug,tower_http=off".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();


    let app = Router::new()
        .fallback(get(|_request: Request| async {
            StatusCode::NOT_FOUND
        }))
        .route("/ws", get(ws_handler))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::default().include_headers(true)),
        );

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8081")
        .await
        .unwrap();
    tracing::debug!("listening on {}", listener.local_addr().unwrap());
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}

/// Upgrades a Websocket Connection
async fn ws_handler(
    ws: WebSocketUpgrade,
    user_agent: Option<TypedHeader<headers::UserAgent>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    let user_agent = if let Some(TypedHeader(user_agent)) = user_agent {
        user_agent.to_string()
    } else {
        String::from("Unknown browser")
    };
    info!("`{user_agent}` at {addr} connected.");

    ws.on_upgrade(move |socket| web_socket_thread(socket, addr))
}

pub async fn web_socket_thread(socket: WebSocket, who: SocketAddr) {
    
}