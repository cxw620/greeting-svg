//! Simple server for generating greeting SVG

mod config;
mod counter;
mod handler;
mod svg;
mod utils;

use anyhow::{Context, Result};
use axum::routing::get;
use macro_toolset::init_tracing_simple;
use miku_server_timing::ServerTimingLayer;
use tokio::net::TcpListener;
use tower_http::compression::CompressionLayer;

// Mimalloc
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing_simple!();

    let config = config::Config::parse()?;

    tracing::info!("{:#?}", config);

    counter::Counter::init(&config).await;

    let service = axum::Router::new()
        .route(
            "/greeting/:id",
            get(handler::axum_greeting).delete(handler::axum_greeting),
        )
        .layer(CompressionLayer::new())
        .layer(ServerTimingLayer::new(env!("CARGO_PKG_NAME")).with_description(utils::VERSION))
        .fallback(handler::fallback);

    // Main Server
    match config.listen {
        config::ListenAddr::SocketAddr(socket_addr) => {
            let tcp_listener = TcpListener::bind(socket_addr).await?;
            let _ = axum::serve(tcp_listener, service)
                .with_graceful_shutdown(shutdown_signal())
                .await;
        }
        #[cfg(not(unix))]
        config::ListenAddr::Unix(unix_path) => {
            panic!("Unix socket is not supported on this platform")
        }
        #[cfg(unix)]
        config::ListenAddr::Unix(unix_path) => {
            use std::{io, time::Duration};

            use hyper::{body::Incoming, Request};
            use hyper_util::{
                rt::{TokioExecutor, TokioIo},
                server,
            };
            use tower_service::Service;

            let _ = tokio::spawn(async move {
                let mut make_service = service.into_make_service();
                let uds_listener = tokio::net::UnixListener::bind(unix_path)
                    .context("Bind UNIX socket error")
                    .unwrap();

                loop {
                    let (stream, _) = match uds_listener.accept().await {
                        Ok(stream) => stream,
                        Err(err) => {
                            #[inline(always)]
                            fn is_connection_error(e: &io::Error) -> bool {
                                matches!(
                                    e.kind(),
                                    io::ErrorKind::ConnectionRefused
                                        | io::ErrorKind::ConnectionAborted
                                        | io::ErrorKind::ConnectionReset
                                )
                            }

                            if is_connection_error(&err) {
                                continue;
                            }

                            tracing::error!("Failed to accept unix stream: {err:#}");
                            tokio::time::sleep(Duration::from_secs(1)).await;
                            continue;
                        }
                    };

                    let tower_service = make_service.call(&stream).await.unwrap();

                    tokio::spawn(async move {
                        let socket = TokioIo::new(stream);

                        let hyper_service =
                            hyper::service::service_fn(move |request: Request<Incoming>| {
                                tower_service.clone().call(request)
                            });

                        if let Err(err) = server::conn::auto::Builder::new(TokioExecutor::new())
                            .serve_connection_with_upgrades(socket, hyper_service)
                            .await
                        {
                            tracing::error!("Failed to serve connection: {err:#}");
                        }
                    });
                }
            })
            .await;
        }
    }

    post_task().await?;

    Ok(())
}

/// axum graceful shutdown signal
async fn shutdown_signal() {
    #[cfg(unix)]
    let hangup = async {
        use tokio::signal::unix::{signal, SignalKind};
        signal(SignalKind::hangup()).unwrap().recv().await;
    };

    #[cfg(not(unix))]
    let hangup = std::future::pending::<()>();

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = hangup => {
            tracing::info!("Received SIGHUP");
        }
    }
}

/// Post task after the server stopped
async fn post_task() -> Result<()> {
    counter::Counter::persist_all().await?;

    Ok(())
}
