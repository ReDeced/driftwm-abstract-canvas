//! Main entry point for driftwm-wallpaper

use driftwm_wallpaper::{config, ipc, ipc_client, layer_shell, wallpaper};
use anyhow::Result;
use clap::Parser;
use tokio::time::Duration;
use tracing::{info, error};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "driftwm-wallpaper", version, about = "External wallpaper system for driftwm")]
struct Args {
    /// Path to config file
    #[arg(short, long)]
    config: Option<String>,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,

    /// Print the current driftwm state and exit
    #[arg(long)]
    dump_state: bool,

    /// Wallpaper module to load (e.g., "hyperbolic", "solid", "image")
    #[arg(short, long)]
    module: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    let filter = if args.debug {
        EnvFilter::new("debug")
    } else {
        EnvFilter::from_default_env()
    };
    tracing_subscriber::fmt().with_env_filter(filter).init();

    info!("Starting driftwm-wallpaper");

    // Load configuration
    let config = config::AppConfig::load(args.config.as_deref())?;
    info!("Configuration loaded");

    // Connect to driftwm IPC
    let mut ipc_client = ipc_client::IpcClient::connect().await?;
    info!("Connected to driftwm IPC");

    if args.dump_state {
        let state = ipc_client.request_state().await?;
        println!("{}", serde_json::to_string_pretty(&state)?);
        return Ok(());
    }

    // Subscribe to state events
    let mut event_stream = ipc_client.subscribe().await?;
    info!("Subscribed to driftwm state events");

    // Initialize the wallpaper renderer
    let mut wallpaper = wallpaper::WallpaperManager::new(config, args.module).await?;
    info!("Wallpaper manager initialized");

    // Create layer-shell surface with SHM presentation
    let mut layer = layer_shell::LayerShell::new(&wallpaper.config().layer).await?;
    info!("Layer shell surface created (size: {}x{})", layer.width(), layer.height());

    // Request initial redraw to kick off the render loop
    layer.request_redraw();

    // Main event loop
    loop {
        // Process pending Wayland events (non-blocking)
        if let Err(e) = layer.dispatch_pending() {
            error!("Layer shell dispatch error: {}", e);
            break;
        }

        // Check for layer shell events (process all pending)
        let mut wayland_events = 0;
        loop {
            match layer.try_next_event() {
                Some(layer_shell::LayerEvent::RedrawRequested { width, height }) => {
                    wayland_events += 1;
                    tracing::info!("Wayland RedrawRequested #{} ({}x{})", wayland_events, width, height);
                    if let Err(e) = wallpaper.render_frame(&mut layer, width, height).await {
                        error!("Render error: {}", e);
                    }
                }
                Some(layer_shell::LayerEvent::Configure { width, height }) => {
                    wayland_events += 1;
                    tracing::info!("Wayland Configure #{} ({}x{})", wayland_events, width, height);
                    wallpaper.resize(width, height);
                    layer.ack_configure();
                    // Request first frame so the compositor sends us a frame callback
                    layer.request_redraw();
                }
                Some(layer_shell::LayerEvent::Close) => {
                    info!("Layer shell closed");
                    return Ok(());
                }
                None => break,
            }
        }
        if wayland_events > 0 {
            tracing::info!("Processed {} Wayland events this iteration", wayland_events);
        }

        // Handle IPC events (with timeout to allow processing layer events)
        tokio::select! {
            event = event_stream.next_event() => {
                match event {
                    Ok(Some(ipc::Event::State(state))) => {
                        wallpaper.update_state(state);
                        // Request a redraw
                        layer.request_redraw();
                    }
                    Ok(None) => {
                        info!("IPC connection closed");
                        break;
                    }
                    Err(e) => {
                        error!("IPC error: {}", e);
                        // Try to reconnect (subscribe consumes the client, so we
                        // create a fresh one and take its event stream directly)
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        match ipc_client::IpcClient::connect().await {
                            Ok(new_client) => {
                                match new_client.subscribe().await {
                                    Ok(new_stream) => {
                                        event_stream = new_stream;
                                        info!("Reconnected to driftwm IPC");
                                    }
                                    Err(e) => error!("Failed to resubscribe: {}", e),
                                }
                            }
                            Err(e) => {
                                error!("Failed to reconnect: {}", e);
                            }
                        }
                    }
                }
            }
            // Small sleep to prevent busy waiting
            _ = tokio::time::sleep(Duration::from_millis(1)) => {}
        }
    }

    info!("Shutting down");
    Ok(())
}