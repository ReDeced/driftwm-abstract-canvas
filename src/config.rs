//! Configuration for driftwm-wallpaper

use anyhow::Result;
use config::{Config as ConfigCrate, File, FileFormat};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Wallpaper module to use
    #[serde(default = "default_module")]
    pub module: String,

    /// Module-specific configuration
    #[serde(default)]
    pub modules: ModuleConfigs,

    /// Layer shell configuration
    #[serde(default)]
    pub layer: LayerConfig,

    /// Rendering configuration
    #[serde(default)]
    pub render: RenderConfig,

    /// Theme integration (Noctalia)
    #[serde(default)]
    pub theme: ThemeConfig,
}

fn default_module() -> String {
    "hyperbolic".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModuleConfigs {
    #[serde(default)]
    pub hyperbolic: HyperbolicConfig,
    #[serde(default)]
    pub solid: SolidConfig,
    #[serde(default)]
    pub image: ImageConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HyperbolicConfig {
    /// Pan rate (radians per second)
    #[serde(default = "default_pan_rate")]
    pub pan_rate: f64,
    /// Maximum radius
    #[serde(default = "default_rmax")]
    pub rmax: f64,
    /// Color palette name or custom colors
    #[serde(default)]
    pub palette: PaletteConfig,
    /// Enable mouse interaction
    #[serde(default = "default_true")]
    pub interactive: bool,
    /// Shader path (optional, uses built-in if not set)
    pub shader_path: Option<PathBuf>,
}

impl Default for HyperbolicConfig {
    fn default() -> Self {
        Self {
            pan_rate: default_pan_rate(),
            rmax: default_rmax(),
            palette: Default::default(),
            interactive: default_true(),
            shader_path: None,
        }
    }
}

fn default_pan_rate() -> f64 { 0.15 }
fn default_rmax() -> f64 { 3.5 }
fn default_true() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolidConfig {
    /// Color in hex format (#RRGGBB or #RRGGBBAA)
    #[serde(default = "default_solid_color")]
    pub color: String,
}

impl Default for SolidConfig {
    fn default() -> Self {
        Self {
            color: default_solid_color(),
        }
    }
}

fn default_solid_color() -> String {
    "#1a1a2e".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageConfig {
    /// Path to image file
    #[serde(default)]
    pub path: Option<PathBuf>,
    /// Fit mode: "cover", "contain", "fill", "center", "tile"
    #[serde(default = "default_fit_mode")]
    pub fit: String,
}

impl Default for ImageConfig {
    fn default() -> Self {
        Self {
            path: None,
            fit: default_fit_mode(),
        }
    }
}

fn default_fit_mode() -> String {
    "cover".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum PaletteConfig {
    /// Use a named palette from the theme
    Named(String),
    /// Custom colors
    Custom(Vec<String>),
}

impl Default for PaletteConfig {
    fn default() -> Self {
        PaletteConfig::Named("default".to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerConfig {
    /// Layer namespace
    #[serde(default = "default_namespace")]
    pub namespace: String,
    /// Z-order (lower = further back)
    #[serde(default)]
    pub z_order: i32,
    /// Output name to target (empty = all outputs)
    #[serde(default)]
    pub output: String,
    /// Anchor to edges
    #[serde(default = "default_anchors")]
    pub anchors: Vec<String>,
    /// Exclusive zone (0 = no reservation)
    #[serde(default)]
    pub exclusive_zone: i32,
}

fn default_namespace() -> String {
    "driftwm-wallpaper".to_string()
}

fn default_anchors() -> Vec<String> {
    vec!["top".to_string(), "bottom".to_string(), "left".to_string(), "right".to_string()]
}

impl Default for LayerConfig {
    fn default() -> Self {
        Self {
            namespace: default_namespace(),
            z_order: -100,
            output: String::new(),
            anchors: default_anchors(),
            exclusive_zone: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderConfig {
    /// Target FPS (0 = unlimited/vsync)
    #[serde(default)]
    pub fps: u32,
    /// Enable vsync
    #[serde(default = "default_true")]
    pub vsync: bool,
    /// Power preference: "low", "high"
    #[serde(default = "default_power_pref")]
    pub power_preference: String,
}

fn default_power_pref() -> String {
    "high".to_string()
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            fps: 60,
            vsync: true,
            power_preference: default_power_pref(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    /// Enable Noctalia theme subscription
    #[serde(default = "default_true")]
    pub enable: bool,
    /// Theme socket path (empty = auto-detect)
    #[serde(default)]
    pub socket: String,
    /// Palette key to use
    #[serde(default = "default_palette_key")]
    pub palette_key: String,
}

fn default_palette_key() -> String {
    "background".to_string()
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            enable: true,
            socket: String::new(),
            palette_key: default_palette_key(),
        }
    }
}

impl AppConfig {
    /// Load configuration from file
    pub fn load(path: Option<&str>) -> Result<Self> {
        let mut builder = ConfigCrate::builder();

        // Default config paths
        let config_dirs = [
            PathBuf::from("/etc/driftwm-wallpaper"),
            dirs::config_dir().map(|d| d.join("driftwm-wallpaper")).unwrap_or_default(),
            std::env::current_dir().unwrap_or_default().join("config"),
        ];

        // Add default config file
        for dir in &config_dirs {
            let file = dir.join("config.toml");
            if file.exists() {
                builder = builder.add_source(File::from(file).format(FileFormat::Toml));
                break;
            }
        }

        // Add user-specified config
        if let Some(path) = path {
            builder = builder.add_source(File::from(PathBuf::from(path)).format(FileFormat::Toml));
        }

        // Environment variable override
        builder = builder.add_source(config::Environment::with_prefix("DRIFTWM_WALLPAPER"));

        let config: AppConfig = builder.build()?.try_deserialize()?;
        info!("Loaded configuration");
        Ok(config)
    }
}

/// Watch for config file changes
pub async fn watch_config_changes() -> broadcast::Receiver<()> {
    let (_tx, rx) = broadcast::channel(1);

    // In a real implementation, this would use notify crate to watch files
    // For now, just return a receiver that never fires
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
    });

    rx
}