# driftwm-wallpaper

External wallpaper system for [driftwm](https://github.com/roman/driftwm) via IPC.

A standalone wallpaper application that communicates with driftwm through its IPC socket, providing flexible, GPU-accelerated wallpapers with live camera/zoom synchronization.

## Features

- **IPC Integration**: Communicates with driftwm via Unix socket for real-time camera/zoom state
- **GPU Accelerated**: Uses wgpu for hardware-accelerated rendering
- **Multiple Modules**: 
  - **Hyperbolic**: {7,3} tiling with animated pan and customizable palettes
  - **Solid**: Simple solid color background
  - **Image**: Image wallpaper with fit modes (cover, contain, fill, center, tile)
- **Hot Reload**: Configuration changes apply without restart
- **Theme Integration**: Subscribes to Noctalia theme for automatic palette updates
- **Layer Shell**: Renders as a Wayland layer-shell surface at the background layer

## Installation

### From Source

```bash
cargo install --git https://github.com/roman/driftwm-wallpaper
```

Or build locally:

```bash
git clone https://github.com/roman/driftwm-wallpaper
cd driftwm-wallpaper
cargo build --release
./target/release/driftwm-wallpaper
```

## Configuration

Copy `config.toml.example` to `~/.config/driftwm-wallpaper/config.toml` or `/etc/driftwm-wallpaper/config.toml` and customize:

```toml
# Wallpaper module: "hyperbolic", "solid", "image"
module = "hyperbolic"

[modules.hyperbolic]
pan_rate = 0.15      # Radians per second
rmax = 3.5           # Maximum radius
palette = { type = "named", value = "noctalia" }
interactive = true

[modules.solid]
color = "#1a1a2e"

[modules.image]
path = "~/Pictures/wallpaper.jpg"
fit = "cover"

[layer]
namespace = "driftwm-wallpaper"
z_order = -100
output = ""
anchors = ["top", "bottom", "left", "right"]
exclusive_zone = 0

[render]
fps = 60
vsync = true
power_preference = "high"

[theme]
enable = true
socket = ""
palette_key = "background"
```

## Usage

```bash
# Run with default config
driftwm-wallpaper

# Run with custom config
driftwm-wallpaper -c /path/to/config.toml

# Enable debug logging
driftwm-wallpaper --debug

# Dump current driftwm state and exit
driftwm-wallpaper --dump-state

# Override module
driftwm-wallpaper -m solid
```

## Architecture

```
┌─────────────────┐     IPC (Unix socket)      ┌──────────────────┐
│   driftwm       │ ◄─────────────────────────► │ driftwm-wallpaper │
│  (compositor)   │   State events, commands    │  (wallpaper app)  │
└─────────────────┘                              └──────────────────┘
                                                        │
                                                        ▼
                                               ┌──────────────────┐
                                               │  wgpu Renderer   │
                                               │  (Vulkan/Metal/   │
                                               │   DX12/OpenGL)   │
                                               └──────────────────┘
                                                        │
                                                        ▼
                                               ┌──────────────────┐
                                               │ Wayland Layer    │
                                               │ Shell Surface    │
                                               │ (Background)     │
                                               └──────────────────┘
```

## Module Development

Create custom wallpaper modules by implementing the `WallpaperModule` trait:

```rust
pub trait WallpaperModule: Send + Sync {
    fn new(config: &HyperbolicConfig) -> Result<Self>;
    fn update(&mut self, frame: &FrameData);
    fn render(&mut self, renderer: &mut Renderer, frame: &FrameData) -> Result<TextureView>;
    fn resize(&mut self, width: u32, height: u32);
    fn reload_config(&mut self, modules: &ModuleConfigs) -> Result<()>;
    fn name(&self) -> &str;
}
```

## License

GPL-3.0-or-later - see [LICENSE](LICENSE) for details.

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Run tests: `cargo test`
5. Submit a pull request

## Related Projects

- [driftwm](https://github.com/roman/driftwm) - The compositing window manager
- [Noctalia](https://github.com/roman/noctalia) - Theme system