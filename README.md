# driftwm-abstract-canvas

**[Русский](README.ru.md)** | English

External wallpaper system for [driftwm](https://github.com/roman/driftwm) via IPC.

A standalone wallpaper application that communicates with driftwm through its IPC socket, providing flexible, GPU-accelerated wallpapers with live camera/zoom synchronization.

## Features

- **IPC Integration**: Communicates with driftwm via Unix socket for real-time camera/zoom state
- **GPU Accelerated**: Uses wgpu for hardware-accelerated rendering
- **Multiple Modules**: 
  - **Hyperbolic**: {7,3} tiling with animated pan and customizable palettes (negative curvature)
  - **Solid**: Simple solid color background
  - **Image**: Image wallpaper with fit modes (cover, contain, fill, center, tile)
  - **Spherical**: Positive-curvature (elliptic) tiling on a sphere — cells converge toward the poles and the space wraps around itself
- **Hot Reload**: Configuration changes apply without restart
- **Smooth Palette Transitions**: Colors fade smoothly when switching palettes (no abrupt jumps)
- **Theme Integration**: Optional Noctalia theme subscription for automatic palette updates (disabled by default)
- **Layer Shell**: Renders as a Wayland layer-shell surface at the background layer

## Installation

### From Source

```bash
cargo install --git https://github.com/ReDeced/driftwm-abstract-canvas
```

Or build locally:

```bash
git clone https://github.com/ReDeced/driftwm-abstract-canvas
cd driftwm-abstract-canvas
cargo build --release
./target/release/driftwm-abstract-canvas
```

## Configuration

Copy `config.toml.example` to `~/.config/driftwm-abstract-canvas/config.toml` or `/etc/driftwm-abstract-canvas/config.toml` and customize:

```toml
# Wallpaper module: "hyperbolic", "solid", "image", "spherical"
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

[modules.spherical]
radius = 1.0         # Sphere radius (positive curvature)
pan_rate = 0.15      # Radians per second
palette = { type = "named", value = "sphere" }
interactive = true

[layer]
namespace = "driftwm-abstract-canvas"
z_order = -100
output = ""
anchors = ["top", "bottom", "left", "right"]
exclusive_zone = 0

[render]
fps = 60
vsync = true
power_preference = "high"

[theme]
# Enable theme subscription (optional, disabled by default)
enable = false
# Theme socket path (empty = auto-detect)
socket = ""
# Palette key to use from theme
palette_key = "background"
```

## Usage

```bash
# Run with default config
driftwm-abstract-canvas

# Run with custom config
driftwm-abstract-canvas -c /path/to/config.toml

# Enable debug logging
driftwm-abstract-canvas --debug

# Dump current driftwm state and exit
driftwm-abstract-canvas --dump-state

# Override module
driftwm-abstract-canvas -m solid
```

## Spherical Module (Positive Curvature)

The `spherical` module renders a tiling on the surface of a sphere using orthographic projection. Unlike the hyperbolic disk (negative curvature, open space), the sphere has **positive curvature**: space is closed, cells converge toward the poles, and the pattern wraps around itself.

### Template example

Add this to your `config.toml` for a Noctalia-inspired spherical wallpaper:

```toml
module = "spherical"

[modules.spherical]
# Radius of the sphere (positive curvature)
radius = 1.0
# Animation speed in radians per second
pan_rate = 0.15
# Palette: named ("default", "monochrome", "solarized", "sphere") or custom hex array
palette = { type = "named", value = "sphere" }
# Enable mouse parallax interaction
interactive = true

[layer]
namespace = "driftwm-abstract-canvas"
z_order = -100
```

Run with:

```bash
driftwm-abstract-canvas -m spherical
```

Or override at runtime:

```bash
./target/release/driftwm-abstract-canvas -m spherical
```

## Integration with driftwm

Add to your driftwm configuration (`~/.config/driftwm/config.toml` or equivalent):

```toml
autostart = [
    "driftwm-abstract-canvas",
]

[background]
type = "none"
```

- `autostart` launches the wallpaper when the driftwm session starts.
- `[background] type = "none"` disables driftwm's own background so the layer-shell surface of the wallpaper is visible.

## Architecture

```
┌─────────────────┐     IPC (Unix socket)      ┌──────────────────┐
│   driftwm       │ ◄─────────────────────────► │ driftwm-abstract-canvas │
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

## Capturing Examples for README

To add real screenshots or GIFs of each module to this README, run driftwm under Wayland (e.g., in a nested compositor like `wayfire` or `weston`), then capture:

```bash
# Start driftwm-abstract-canvas with a specific module
./target/release/driftwm-abstract-canvas -m hyperbolic
./target/release/driftwm-abstract-canvas -m spherical
./target/release/driftwm-abstract-canvas -m solid
./target/release/driftwm-abstract-canvas -m image -c path/to/image.toml

# Capture a screenshot (requires grim/swaygrim or similar)
grim -o <output> screenshot_<module>.png

# Capture a short GIF/MP4 (requires wf-recorder or similar)
wf-recorder --output <output> -f demo_<module>.mp4 --duration 10
```

Place the resulting files under `docs/screenshots/` and reference them:

```markdown
![Hyperbolic module](docs/screenshots/hyperbolic.png)
![Spherical module](docs/screenshots/spherical.png)
```

Recommended capture settings:
- Resolution: 1920×1080 (or your primary output)
- Duration: 8–12 seconds per module to show the animated pan
- Include the palette transition: switch `palette` in config while running to demonstrate smooth color fading

## AI Disclosure

This project was created with the assistance of AI. We also recommend using AI
tools when creating new abstract wallpaper modules for this repository — the
`WallpaperModule` trait and WGSL shaders are well-suited to AI-generated code,
and the `AGENTS.md` file contains guidelines to help AI agents write consistent
modules.

## Related Projects

- [driftwm](https://github.com/roman/driftwm) - The compositing window manager
- [Noctalia](https://github.com/roman/noctalia) - Theme system