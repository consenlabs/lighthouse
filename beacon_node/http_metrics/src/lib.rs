#[macro_use]
extern crate lazy_static;

mod health;
mod metrics;

use beacon_chain::{BeaconChain, BeaconChainTypes};
use serde::{Deserialize, Serialize};
use slog::{crit, info, Logger};
use std::future::Future;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::path::PathBuf;
use std::sync::Arc;
use warp::{http::Response, Filter};

#[derive(Debug)]
pub enum Error {
    Warp(warp::Error),
    Other(String),
}

impl From<warp::Error> for Error {
    fn from(e: warp::Error) -> Self {
        Error::Warp(e)
    }
}

impl From<String> for Error {
    fn from(e: String) -> Self {
        Error::Other(e)
    }
}

pub struct Context<T: BeaconChainTypes> {
    pub config: Config,
    pub chain: Option<Arc<BeaconChain<T>>>,
    pub db_path: Option<PathBuf>,
    pub freezer_db_path: Option<PathBuf>,
    pub log: Logger,
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub enabled: bool,
    pub listen_addr: Ipv4Addr,
    pub listen_port: u16,
    pub allow_origin: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            listen_addr: Ipv4Addr::new(127, 0, 0, 1),
            listen_port: 5054,
            allow_origin: None,
        }
    }
}

pub fn serve<T: BeaconChainTypes>(
    ctx: Arc<Context<T>>,
    shutdown: impl Future<Output = ()> + Send + Sync + 'static,
) -> Result<(SocketAddr, impl Future<Output = ()>), Error> {
    let config = &ctx.config;
    let log = ctx.log.clone();

    if !config.enabled {
        crit!(log, "Cannot start disabled metrics HTTP server");
        return Err(Error::Other(
            "A disabled metrics server should not be started".to_string(),
        ));
    }

    let inner_ctx = ctx.clone();
    let routes = warp::get()
        .and(warp::path("metrics"))
        .map(move || inner_ctx.clone())
        .and_then(|ctx: Arc<Context<T>>| async move {
            Ok::<_, warp::Rejection>(
                metrics::gather_prometheus_metrics(&ctx)
                    .map(|body| Response::builder().status(200).body(body).unwrap())
                    .unwrap_or_else(|e| {
                        Response::builder()
                            .status(500)
                            .body(format!("Unable to gather metrics: {:?}", e))
                            .unwrap()
                    }),
            )
        });

    let (listening_socket, server) = warp::serve(routes).try_bind_with_graceful_shutdown(
        SocketAddrV4::new(config.listen_addr, config.listen_port),
        async {
            shutdown.await;
        },
    )?;

    info!(
        log,
        "Metrics HTTP server started";
        "listen_address" => listening_socket.to_string(),
    );

    Ok((listening_socket, server))
}