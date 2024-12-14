use std::{
    fs::File,
    net::SocketAddr,
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, OnceLock,
    },
};

use anyhow::{Context, Result};
use arc_swap::ArcSwap;
use clap::Parser;
use serde::{Deserialize, Serialize};

// === Configs ===

/// New counter `access_key`
pub(crate) static CONF_ACCESS_KEY: OnceLock<ArcSwap<String>> = OnceLock::new();
/// Max number of counters
pub(crate) static CONF_MAX_COUNTERS: AtomicUsize = AtomicUsize::new(131072);

#[derive(Debug, Parser, Serialize, Deserialize)]
#[command(version, about, long_about = None)]
pub(crate) struct Config {
    #[arg(short, long, default_value = "0.0.0.0:8989")]
    /// Listen address
    pub listen: SocketAddr,

    #[arg(long)]
    /// `access_key` for adding new counters
    ///
    /// If not set, no new counters can be added and use the existing ones from
    /// the config.
    pub access_key: Option<Arc<String>>,

    #[arg(short, long)]
    /// Authorized user ids
    pub user_id: Vec<Arc<str>>,

    #[arg(short, long, default_value_t = 131072)]
    /// Max number of counters
    ///
    /// Notice: for public service, this should not be set to `0`
    /// since your server may run out of memory.
    pub max_counter: usize,
}

impl Config {
    /// Parse command line arguments, or read from config file
    pub(crate) fn parse() -> Result<Self> {
        let args = Config::try_parse();

        let file = Path::new("./config.json");
        if file.exists() {
            tracing::info!("Reading config file from {}", file.to_str().unwrap());

            let fs = File::open(file).with_context(|| "Read config.json error")?;
            let config: Config =
                serde_json::from_reader(fs).with_context(|| "Parse config.json error")?;

            config.update_config();

            return Ok(config);
        }

        tracing::info!("Read command line arguments...");
        args.inspect(|config| config.update_config())
            .map_err(Into::into)
    }

    /// Update counter related config from given
    /// [Config](crate::config::Config).
    pub(crate) fn update_config(&self) {
        // * Update max counters limits
        CONF_MAX_COUNTERS.store(self.max_counter, Ordering::Relaxed);

        // * Update access_key
        //
        // * If we have access_key set, we replace the old access_key with the new one.
        // * If new access_key is None, we do nothing and we have to restart the server.
        if self
            .access_key
            .as_ref()
            .is_some_and(|access_key| access_key.len() > 0)
        {
            let new_access_key = self.access_key.clone().unwrap();

            match CONF_ACCESS_KEY.get() {
                Some(access_key) => access_key.store(new_access_key),
                None => CONF_ACCESS_KEY.set(ArcSwap::new(new_access_key)).unwrap(),
            }
        }
    }
}
