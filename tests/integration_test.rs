//! Integration tests for driftwm-wallpaper

use driftwm_wallpaper::{config, ipc, wallpaper};
use std::path::PathBuf;

#[test]
fn test_config_loading() {
    let config = config::AppConfig::load(None).unwrap();
    assert_eq!(config.module, "hyperbolic");
    assert!(config.modules.hyperbolic.pan_rate > 0.0);
    assert!(config.modules.hyperbolic.rmax > 0.0);
}

#[test]
fn test_hyperbolic_config_defaults() {
    let config = config::HyperbolicConfig::default();
    assert_eq!(config.pan_rate, 0.15);
    assert_eq!(config.rmax, 3.5);
    assert!(config.interactive);
}

#[test]
fn test_solid_config_defaults() {
    let config = config::SolidConfig::default();
    assert_eq!(config.color, "#1a1a2e");
}

#[test]
fn test_spherical_config_defaults() {
    let config = config::SphericalConfig::default();
    assert_eq!(config.radius, 1.0);
    assert_eq!(config.pan_rate, 0.15);
    assert!(config.interactive);
    assert!(config.shader_path.is_none());
}

#[test]
fn test_image_config_defaults() {
    let config = config::ImageConfig::default();
    assert_eq!(config.fit, "cover");
    assert!(config.path.is_none());
}

#[test]
fn test_layer_config_defaults() {
    let config = config::LayerConfig::default();
    assert_eq!(config.namespace, "driftwm-wallpaper");
    assert_eq!(config.z_order, -100);
    assert_eq!(config.anchors, vec!["top", "bottom", "left", "right"]);
    assert_eq!(config.exclusive_zone, 0);
}

#[test]
fn test_render_config_defaults() {
    let config = config::RenderConfig::default();
    assert_eq!(config.fps, 60);
    assert!(config.vsync);
    assert_eq!(config.power_preference, "high");
}

#[test]
fn test_theme_config_defaults() {
    let config = config::ThemeConfig::default();
    assert!(config.enable);
    assert_eq!(config.palette_key, "background");
}

#[test]
fn test_palette_config_named() {
    let palette = config::PaletteConfig::Named("noctalia".to_string());
    match palette {
        config::PaletteConfig::Named(name) => assert_eq!(name, "noctalia"),
        _ => panic!("Expected Named variant"),
    }
}

#[test]
fn test_palette_config_custom() {
    let palette = config::PaletteConfig::Custom(vec!["#ff0000".to_string(), "#00ff00".to_string()]);
    match palette {
        config::PaletteConfig::Custom(colors) => {
            assert_eq!(colors.len(), 2);
            assert_eq!(colors[0], "#ff0000");
        },
        _ => panic!("Expected Custom variant"),
    }
}

#[test]
fn test_palette_config_default() {
    let palette = config::PaletteConfig::default();
    match palette {
        config::PaletteConfig::Named(name) => assert_eq!(name, "default"),
        _ => panic!("Expected Named variant"),
    }
}

#[test]
fn test_module_configs_default() {
    let modules = config::ModuleConfigs::default();
    assert_eq!(modules.hyperbolic.pan_rate, 0.15);
    assert_eq!(modules.solid.color, "#1a1a2e");
    assert_eq!(modules.image.fit, "cover");
    assert_eq!(modules.spherical.radius, 1.0);
    assert_eq!(modules.spherical.pan_rate, 0.15);
}

#[test]
fn test_ipc_state_info_serialization() {
    let state = ipc::StateInfo {
        camera: (100.0, -200.0),
        zoom: 0.5,
        layout: "English (US)".to_string(),
        layout_short: "us".to_string(),
        windows: vec![ipc::WindowInfo {
            id: 42,
            app_id: "alacritty".to_string(),
            title: "terminal".to_string(),
            position: [10, 20],
            size: [800, 600],
            is_focused: true,
            is_widget: false,
            suspended: false,
            mode: ipc::WindowMode::Normal,
            pid: Some(1234),
        }],
        fullscreen: vec![],
        pinned: vec![],
        layers: vec!["waybar".to_string()],
        canvas_layers: vec![],
        outputs: vec![ipc::OutputInfo {
            name: "DP-1".to_string(),
            camera: (0.0, 0.0),
            zoom: 1.0,
            size: [1920, 1080],
            active: true,
            active_bookmark: Some("home".to_string()),
        }],
        active_bookmark: Some("home".to_string()),
    };

    let json = serde_json::to_string(&state).unwrap();
    let parsed: ipc::StateInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(state, parsed);
    assert_eq!(state.camera, (100.0, -200.0));
    assert_eq!(state.zoom, 0.5);
    assert_eq!(state.windows.len(), 1);
    assert_eq!(state.windows[0].app_id, "alacritty");
    assert_eq!(state.outputs[0].name, "DP-1");
}

#[test]
fn test_ipc_request_camera_serialization() {
    // Test read camera (None)
    let req = ipc::Request::Camera(None);
    let json = serde_json::to_string(&req).unwrap();
    assert_eq!(json, r#"{"Camera":null}"#);

    // Test write camera (Some)
    let req = ipc::Request::Camera(Some((100.0, -200.0)));
    let json = serde_json::to_string(&req).unwrap();
    assert_eq!(json, r#"{"Camera":[100.0,-200.0]}"#);
}

#[test]
fn test_ipc_request_zoom_serialization() {
    let req = ipc::Request::Zoom(None);
    let json = serde_json::to_string(&req).unwrap();
    assert_eq!(json, r#"{"Zoom":null}"#);

    let req = ipc::Request::Zoom(Some(0.5));
    let json = serde_json::to_string(&req).unwrap();
    assert_eq!(json, r#"{"Zoom":0.5}"#);
}

#[test]
fn test_ipc_request_layout_serialization() {
    let req = ipc::Request::Layout { short: true };
    let json = serde_json::to_string(&req).unwrap();
    assert_eq!(json, r#"{"Layout":{"short":true}}"#);

    let req = ipc::Request::Layout { short: false };
    let json = serde_json::to_string(&req).unwrap();
    assert_eq!(json, r#"{"Layout":{"short":false}}"#);
}

#[test]
fn test_ipc_window_selector_serialization() {
    // Test Id selector
    let selector = ipc::WindowSelector::Id(42);
    let json = serde_json::to_string(&selector).unwrap();
    assert_eq!(json, "42");

    // Test AppId selector
    let selector = ipc::WindowSelector::AppId("alacritty".to_string());
    let json = serde_json::to_string(&selector).unwrap();
    assert_eq!(json, "\"alacritty\"");
}
