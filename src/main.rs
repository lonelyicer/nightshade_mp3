#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod charset;
mod clock;
mod command;
mod config;
mod error;
mod media;
mod model;
mod osc;
mod oscquery;
mod runtime;
mod settings;
mod text;
mod tray;

use crate::error::AppResult;

use tokio::{runtime::Builder, sync::mpsc::unbounded_channel};

use tracing_subscriber::EnvFilter;

fn main() -> AppResult<()> {
    initialize_logging();

    if settings_mode() {
        return settings::run();
    }

    let backend_runtime = Builder::new_multi_thread().enable_all().build()?;

    let (runtime_sender, runtime_receiver) = unbounded_channel();

    let _backend_task = backend_runtime.spawn(async move {
        if let Err(error) = runtime::run(runtime_receiver).await {
            tracing::error!(
                error = %error,
                "backend runtime stopped"
            );
        }
    });

    tray::run(runtime_sender)
}

fn settings_mode() -> bool {
    std::env::args()
        .skip(1)
        .any(|argument| argument == "--settings")
}

fn initialize_logging() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("nightshade_mp3=info"));

    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}
