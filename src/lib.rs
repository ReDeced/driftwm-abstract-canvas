//! driftwm-wallpaper - External wallpaper system for driftwm via IPC

pub mod config;
pub mod ipc;
pub mod ipc_client;
pub mod layer_shell;
pub mod renderer;
pub mod wallpaper;

// Shaders are included via include_str! in wallpaper.rs

#[cfg(test)]
mod tests {
    use super::ipc::{Request, Response, StateInfo, WindowInfo, WindowMode, socket_path};
    use std::path::PathBuf;

    #[test]
    fn test_socket_path() {
        let path = socket_path("wayland-0");
        assert!(path.to_string_lossy().contains("ipc-wayland-0.sock"));
    }

    #[test]
    fn test_request_serialization() {
        let req = Request::Camera(None);
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(json, r#"{"Camera":null}"#);

        let req = Request::Camera(Some((100.0, -200.0)));
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(json, r#"{"Camera":[100.0,-200.0]}"#);
    }

    #[test]
    fn test_subscribe_request() {
        // Unit variants in externally tagged enums serialize as just the variant name
        let req = Request::Subscribe;
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(json, r#""Subscribe""#);
    }

    #[test]
    fn test_state_info_roundtrip() {
        let state = StateInfo {
            camera: (0.0, 0.0),
            zoom: 1.0,
            layout: "English (US)".into(),
            layout_short: "us".into(),
            windows: vec![WindowInfo {
                id: 1,
                app_id: "foot".into(),
                title: "terminal".into(),
                position: [100, -100],
                size: [800, 600],
                is_focused: true,
                is_widget: false,
                suspended: false,
                mode: WindowMode::Normal,
                pid: Some(1234),
            }],
            fullscreen: vec![],
            pinned: vec![],
            layers: vec!["waybar".into()],
            canvas_layers: vec![],
            outputs: vec![],
            active_bookmark: Some("home".into()),
        };

        let json = serde_json::to_string(&state).unwrap();
        let parsed: StateInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(state, parsed);
    }
}