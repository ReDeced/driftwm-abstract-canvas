//! driftwm IPC client

use crate::ipc::{Event, Reply, Request, Response, StateInfo, socket_path};
use anyhow::{Result, Context, bail};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader as AsyncBufReader};
use tokio::net::UnixStream as AsyncUnixStream;
use tokio::sync::mpsc;
use tracing::{debug, warn};

/// Connected IPC client
pub struct IpcClient {
    stream: AsyncUnixStream,
}

impl IpcClient {
    /// Connect to the driftwm IPC socket
    pub async fn connect() -> Result<Self> {
        let path = Self::resolve_socket_path()?;
        debug!("Connecting to driftwm IPC at {}", path.display());

        let stream = AsyncUnixStream::connect(&path)
            .await
            .context(format!("Failed to connect to {}", path.display()))?;

        Ok(Self { stream })
    }

    /// Resolve the IPC socket path
    fn resolve_socket_path() -> Result<PathBuf> {
        // Check for explicit override
        if let Ok(path) = std::env::var("DRIFTWM_SOCKET") {
            return Ok(PathBuf::from(path));
        }

        // Derive from WAYLAND_DISPLAY
        let display = std::env::var("WAYLAND_DISPLAY")
            .context("WAYLAND_DISPLAY is not set — are you in a driftwm session?")?;

        Ok(socket_path(&display))
    }

    /// Send a request and wait for a reply
    pub async fn send_request(&mut self, request: Request) -> Result<Reply> {
        let mut payload = serde_json::to_vec(&request)?;
        payload.push(b'\n');

        self.stream.write_all(&payload).await?;
        self.stream.flush().await?;

        let mut reader = AsyncBufReader::new(&mut self.stream);
        let mut line = String::new();
        reader.read_line(&mut line).await?;

        if line.trim().is_empty() {
            bail!("No response from compositor");
        }

        let reply: Reply = serde_json::from_str(line.trim_end())?;
        Ok(reply)
    }

    /// Get current state
    pub async fn request_state(&mut self) -> Result<StateInfo> {
        let reply = self.send_request(Request::State).await?;
        match reply {
            Ok(Response::State(info)) => Ok(*info),
            Ok(other) => bail!("Unexpected response: {:?}", other),
            Err(e) => bail!("IPC error: {}", e),
        }
    }

    /// Subscribe to state events
    ///
    /// After the `Ok` reply, the compositor pushes [`Event`] lines on the same
    /// connection, so this method takes ownership of the client's stream and
    /// hands it to the [`EventStream`]. The client is consumed and must not be
    /// used afterwards (create a new [`IpcClient`] to reconnect).
    pub async fn subscribe(mut self) -> Result<EventStream> {
        let reply = self.send_request(Request::Subscribe).await?;
        match reply {
            Ok(Response::Ok) => Ok(EventStream::new(self.stream)),
            Ok(other) => bail!("Unexpected subscribe response: {:?}", other),
            Err(e) => bail!("Subscribe failed: {}", e),
        }
    }

    /// Get camera position
    pub async fn get_camera(&mut self) -> Result<(f64, f64)> {
        let reply = self.send_request(Request::Camera(None)).await?;
        match reply {
            Ok(Response::Camera { x, y }) => Ok((x, y)),
            Ok(other) => bail!("Unexpected response: {:?}", other),
            Err(e) => bail!("IPC error: {}", e),
        }
    }

    /// Set camera position
    pub async fn set_camera(&mut self, x: f64, y: f64) -> Result<()> {
        let reply = self.send_request(Request::Camera(Some((x, y)))).await?;
        match reply {
            Ok(Response::Ok) => Ok(()),
            Ok(other) => bail!("Unexpected response: {:?}", other),
            Err(e) => bail!("IPC error: {}", e),
        }
    }

    /// Get zoom level
    pub async fn get_zoom(&mut self) -> Result<f64> {
        let reply = self.send_request(Request::Zoom(None)).await?;
        match reply {
            Ok(Response::Zoom(z)) => Ok(z),
            Ok(other) => bail!("Unexpected response: {:?}", other),
            Err(e) => bail!("IPC error: {}", e),
        }
    }

    /// Set zoom level
    pub async fn set_zoom(&mut self, zoom: f64) -> Result<()> {
        let reply = self.send_request(Request::Zoom(Some(zoom))).await?;
        match reply {
            Ok(Response::Ok) => Ok(()),
            Ok(other) => bail!("Unexpected response: {:?}", other),
            Err(e) => bail!("IPC error: {}", e),
        }
    }
}

/// Stream of events from a subscribed connection
pub struct EventStream {
    reader: AsyncBufReader<AsyncUnixStream>,
}

impl EventStream {
    fn new(stream: AsyncUnixStream) -> Self {
        Self {
            reader: AsyncBufReader::new(stream),
        }
    }

    /// Get the next event
    pub async fn next_event(&mut self) -> Result<Option<Event>> {
        loop {
            let mut line = String::new();
            let bytes_read = self.reader.read_line(&mut line).await?;

            if bytes_read == 0 {
                return Ok(None); // Connection closed
            }

            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                continue; // Skip empty lines
            }

            let event: Event = serde_json::from_str(trimmed)
                .context(format!("Failed to parse event: {}", trimmed))?;

            return Ok(Some(event));
        }
    }
}

/// Spawn a background task that reconnects and forwards events
pub fn spawn_event_stream() -> mpsc::UnboundedReceiver<Event> {
    let (tx, rx) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        loop {
            match IpcClient::connect().await {
                Ok(mut client) => {
                    if let Ok(mut stream) = client.subscribe().await {
                        while let Ok(Some(event)) = stream.next_event().await {
                            if tx.send(event).is_err() {
                                return; // Receiver dropped
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to connect to driftwm IPC: {}", e);
                }
            }

            // Wait before retrying
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        }
    });

    rx
}