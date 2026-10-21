//! driftwm IPC protocol types — copied from driftwm/src/ipc/protocol.rs
//! Keep in sync with driftwm's protocol.

use std::collections::BTreeMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// Selects a window: by stable id (JSON number) or case-insensitive `app_id` substring (JSON string).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WindowSelector {
    Id(u64),
    AppId(String),
}

/// A command from a client to the compositor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Request {
    /// Coordinates are the viewport center, Y-up.
    Camera(Option<(f64, f64)>),
    /// Set value is clamped to the supported range.
    Zoom(Option<f64>),
    Layout { short: bool },
    Cursor,
    State,
    DebugCounters,
    /// Switches the connection to push mode: after the `Ok` reply the server
    /// writes one [`Event`] line immediately and another on every rendered frame
    /// while state keeps changing.
    Subscribe,
    Focus(Option<WindowSelector>),
    Move {
        #[serde(default)]
        window: Option<WindowSelector>,
        #[serde(default)]
        to: Option<(i32, i32)>,
    },
    Resize {
        #[serde(default)]
        window: Option<WindowSelector>,
        #[serde(default)]
        to: Option<(i32, i32)>,
    },
    Opacity {
        #[serde(default)]
        window: Option<WindowSelector>,
        #[serde(default)]
        value: Option<f64>,
    },
    Pin {
        #[serde(default)]
        window: Option<WindowSelector>,
        #[serde(default)]
        value: Option<bool>,
    },
    Close(Option<WindowSelector>),
    Suspend(Option<WindowSelector>),
    Relaunch(Option<WindowSelector>),
    Action(String),
    Bookmark {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        to: Option<(f64, f64)>,
        #[serde(default)]
        delete: bool,
    },
    Screenshot {
        target: ScreenshotTarget,
        scale: f64,
        path: String,
    },
}

/// What a [`Request::Screenshot`] captures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ScreenshotTarget {
    Viewport,
    Window {
        #[serde(default)]
        window: Option<WindowSelector>,
    },
    All,
    Region {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        from_screen: bool,
    },
}

/// A successful reply payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Response {
    Camera { x: f64, y: f64 },
    Zoom(f64),
    Layout(String),
    Cursor { x: f64, y: f64, output: String },
    State(Box<StateInfo>),
    DebugCounters(BTreeMap<String, usize>),
    Focused(Option<FocusedWindow>),
    Position { x: i32, y: i32 },
    Size { width: i32, height: i32 },
    Opacity(f64),
    Pin(bool),
    Screenshot { path: String, width: u32, height: u32 },
    Bookmarks(BTreeMap<String, [f64; 2]>),
    Bookmark { x: f64, y: f64 },
    Ok,
}

/// The result of a request: `Ok(Response)` or a human-readable error string.
pub type Reply = Result<Response, String>;

/// The focused window in a [`Response::Focused`] reply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FocusedWindow {
    pub id: u64,
    pub app_id: Option<String>,
}

/// The full compositor state snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateInfo {
    pub camera: (f64, f64),
    pub zoom: f64,
    #[serde(default)]
    pub layout: String,
    #[serde(default)]
    pub layout_short: String,
    pub windows: Vec<WindowInfo>,
    #[serde(default)]
    pub fullscreen: Vec<OutputFullscreen>,
    #[serde(default)]
    pub pinned: Vec<OutputPinned>,
    #[serde(default)]
    pub layers: Vec<String>,
    #[serde(default)]
    pub canvas_layers: Vec<CanvasLayerInfo>,
    #[serde(default)]
    pub outputs: Vec<OutputInfo>,
    #[serde(default)]
    pub active_bookmark: Option<String>,
}

/// One output's viewport in the `state` reply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputInfo {
    pub name: String,
    pub camera: (f64, f64),
    pub zoom: f64,
    pub size: [i32; 2],
    pub active: bool,
    #[serde(default)]
    pub active_bookmark: Option<String>,
}

/// One window in the canvas inventory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowInfo {
    pub id: u64,
    pub app_id: String,
    pub title: String,
    pub position: [i32; 2],
    pub size: [i32; 2],
    pub is_focused: bool,
    pub is_widget: bool,
    #[serde(default)]
    pub suspended: bool,
    #[serde(default)]
    pub mode: WindowMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
}

/// Which compositor-side size mode a canvas window is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum WindowMode {
    #[default]
    Normal,
    Fit,
    Fill,
}

/// A fullscreen window in the IPC `state` reply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputFullscreen {
    pub id: u64,
    pub output: String,
    pub app_id: String,
    pub title: String,
}

/// A screen-pinned window in the IPC `state` reply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputPinned {
    pub id: u64,
    pub output: String,
    pub app_id: String,
    pub title: String,
    pub position: [i32; 2],
    pub size: [i32; 2],
}

/// A canvas-positioned layer surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasLayerInfo {
    pub app_id: String,
    pub position: [i32; 2],
    pub size: [i32; 2],
}

/// A line pushed to a subscribed connection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Event {
    State(StateInfo),
}

/// Path to the IPC socket for a given `WAYLAND_DISPLAY` name:
/// `$XDG_RUNTIME_DIR/driftwm/ipc-<wayland_display>.sock` (falls back to `/tmp`).
pub fn socket_path(wayland_display: &str) -> PathBuf {
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(dir)
        .join("driftwm")
        .join(format!("ipc-{wayland_display}.sock"))
}