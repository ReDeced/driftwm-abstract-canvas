//! Wallpaper management and rendering

use crate::config::{AppConfig, HyperbolicConfig, ModuleConfigs, PaletteConfig, SphericalConfig};
use crate::ipc::StateInfo;
use crate::renderer::{Renderer, FrameData, OutputState, WindowState};
use crate::layer_shell;
use anyhow::{Context as _, Result};
use std::sync::Arc;
use tracing::{info, debug};

/// Uniforms for hyperbolic shader
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct HyperbolicUniforms {
    resolution: [f32; 2],
    camera: [f32; 2],
    zoom: f32,
    time: f32,
    pan_angle: f32,
    pan_rate: f32,
    rmax: f32,
    center_count: u32,
}

/// Number of tiling cells uploaded to the hyperbolic shader
const HYPERBOLIC_MAX_CELLS: usize = 256;

/// Uniforms for solid color shader
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct SolidUniforms {
    color: [f32; 4],
}

/// Manages wallpaper modules and rendering
pub struct WallpaperManager {
    config: Arc<AppConfig>,
    current_module: Box<dyn WallpaperModule>,
    renderer: Renderer,
    state: WallpaperState,
}

struct WallpaperState {
    camera: (f64, f64),
    zoom: f64,
    outputs: Vec<OutputState>,
    windows: Vec<WindowState>,
    time: f64,
    frame_count: u64,
}

impl WallpaperManager {
    /// Create a new wallpaper manager
    pub async fn new(config: AppConfig, module_override: Option<String>) -> Result<Self> {
        let config = Arc::new(config);
        let module_name = module_override.unwrap_or_else(|| config.module.clone());

        // Create renderer (headless, renders to offscreen texture)
        let renderer = Renderer::new(&config.render).await?;
        let current_module = Self::create_module(&module_name, &config.modules, &config.layer)?;

        info!("Loaded wallpaper module: {}", module_name);

        Ok(Self {
            config,
            current_module,
            renderer,
            state: WallpaperState {
                camera: (0.0, 0.0),
                zoom: 1.0,
                outputs: Vec::new(),
                windows: Vec::new(),
                time: 0.0,
                frame_count: 0,
            },
        })
    }

    /// Get the configuration
    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    /// Create a wallpaper module by name
    fn create_module(
        name: &str,
        modules: &ModuleConfigs,
        _layer: &crate::config::LayerConfig,
    ) -> Result<Box<dyn WallpaperModule>> {
        match name {
            "hyperbolic" => {
                let module = <HyperbolicModule as WallpaperModule>::new(modules)?;
                Ok(Box::new(module))
            }
            "solid" => {
                let module = <SolidModule as WallpaperModule>::new(modules)?;
                Ok(Box::new(module))
            }
            "image" => {
                let module = <ImageModule as WallpaperModule>::new(modules)?;
                Ok(Box::new(module))
            }
            "spherical" => {
                let module = <SphericalModule as WallpaperModule>::new(modules)?;
                Ok(Box::new(module))
            }
            _ => anyhow::bail!("Unknown wallpaper module: {}", name),
        }
    }

    /// Update state from driftwm IPC
    pub fn update_state(&mut self, state: StateInfo) {
        // Use the active output's camera/zoom, or the global one
        let active_output = state.outputs.iter().find(|o| o.active);
        
        if let Some(output) = active_output {
            self.state.camera = output.camera;
            self.state.zoom = output.zoom;
        } else {
            self.state.camera = state.camera;
            self.state.zoom = state.zoom;
        }

        self.state.outputs = state.outputs.into_iter().map(|o| OutputState {
            name: o.name,
            camera: o.camera,
            zoom: o.zoom,
            size: o.size,
            active: o.active,
        }).collect();

        self.state.windows = state.windows.into_iter().map(|w| WindowState {
            id: w.id,
            app_id: w.app_id,
            position: w.position,
            size: w.size,
            is_focused: w.is_focused,
        }).collect();

        debug!("Updated state: camera={:?}, zoom={}, outputs={}, windows={}", 
               self.state.camera, self.state.zoom, 
               self.state.outputs.len(), self.state.windows.len());
    }

    /// Resize the rendering surface
    pub fn resize(&mut self, width: u32, height: u32) {
        self.renderer.resize(width, height);
        self.current_module.resize(width, height);
    }

    /// Render a frame and present via SHM
    pub async fn render_frame(&mut self, layer: &mut layer_shell::LayerShell, width: u32, height: u32) -> Result<()> {
        tracing::info!("render_frame: {}x{}, frame_count={}", width, height, self.state.frame_count);
        self.state.frame_count += 1;
        self.state.time = self.state.frame_count as f64 / 60.0; // Approximate

        // Prepare frame data
        let frame_data = FrameData {
            camera: self.state.camera,
            zoom: self.state.zoom,
            outputs: self.state.outputs.clone(),
            windows: self.state.windows.clone(),
            time: self.state.time,
            frame_count: self.state.frame_count,
            width,
            height,
        };

        // Let the module update its internal state
        self.current_module.update(&frame_data);

        // Ensure pipeline exists (created lazily on first render or after resize)
        self.current_module.ensure_pipeline(&self.renderer)?;

        // Upload per-frame uniforms before drawing
        self.current_module.update_uniforms(&self.renderer, &frame_data)?;

        // Get pipeline and bind group from module
        let pipeline = self.current_module.pipeline();
        let bind_group = self.current_module.bind_group();

        // Render to offscreen texture
        self.renderer.begin_render_pass(pipeline, bind_group)?;

        // Copy to CPU
        let pixels = self.renderer.copy_to_cpu()?;

        // Present via SHM
        layer.present_shm(pixels, width, height)?;

        Ok(())
    }

    /// Reload configuration
    pub async fn reload_config(&mut self) -> Result<()> {
        let new_config = AppConfig::load(None)?;
        let module_name = new_config.module.clone();
        
        // If module changed, recreate it
        if module_name != self.config.module {
            self.current_module = Self::create_module(&module_name, &new_config.modules, &new_config.layer)?;
            info!("Switched to wallpaper module: {}", module_name);
        }

        self.config = Arc::new(new_config);
        self.current_module.reload_config(&self.config.modules)?;
        
        Ok(())
    }
}

/// Trait for wallpaper modules
pub trait WallpaperModule: Send + Sync {
    /// Initialize the module from module configs
    fn new(modules: &ModuleConfigs) -> Result<Self> where Self: Sized;

    /// Update internal state from frame data
    fn update(&mut self, frame: &FrameData);

    /// Ensure the render pipeline exists (create if needed, e.g. after resize)
    fn ensure_pipeline(&mut self, renderer: &Renderer) -> Result<()>;

    /// Write per-frame uniform data (called after ensure_pipeline, before draw)
    fn update_uniforms(&mut self, renderer: &Renderer, frame: &FrameData) -> Result<()>;

    /// Get the render pipeline
    fn pipeline(&self) -> &wgpu::RenderPipeline;

    /// Get the bind group
    fn bind_group(&self) -> &wgpu::BindGroup;

    /// Handle resize
    fn resize(&mut self, width: u32, height: u32);

    /// Reload configuration
    fn reload_config(&mut self, modules: &ModuleConfigs) -> Result<()>;

    /// Get module name
    fn name(&self) -> &str;
}

/// Hyperbolic {5,4} pentagon tiling module
pub struct HyperbolicModule {
    config: HyperbolicConfig,
    pipeline: Option<wgpu::RenderPipeline>,
    bind_group: Option<wgpu::BindGroup>,
    uniform_buffer: Option<wgpu::Buffer>,
    centers_buffer: Option<wgpu::Buffer>,
    /// Per-cell data: [cx, cy, color_index, unused] in Poincaré disk coords
    centers: Vec<[f32; 4]>,
    width: u32,
    height: u32,
    pan_angle: f64,
    palette: Vec<[f32; 4]>,
    target_palette: Vec<[f32; 4]>,
    transition: f32,
    transition_speed: f32,
}

impl HyperbolicModule {
    pub fn new(config: &HyperbolicConfig) -> Result<Self> {
        // Parse palette
        let palette = Self::parse_palette(&config.palette)?;

        // Generate {5,4} tiling cell centers via BFS reflections
        let centers = Self::generate_pentagon_centers(HYPERBOLIC_MAX_CELLS);
        debug!("Generated {} hyperbolic pentagon cells", centers.len());

        Ok(Self {
            config: config.clone(),
            pipeline: None,
            bind_group: None,
            uniform_buffer: None,
            centers_buffer: None,
            centers,
            width: 0,
            height: 0,
            pan_angle: 0.0,
            palette: palette.clone(),
            target_palette: palette,
            transition: 1.0,
            transition_speed: 0.5,
        })
    }

    /// Möbius translation: hyperbolic isometry sending origin to `c`
    fn mobius(z: [f64; 2], c: [f64; 2]) -> [f64; 2] {
        let den = 1.0 + c[0] * z[0] + c[1] * z[1];
        [(z[0] + c[0]) / den, (z[1] + c[1]) / den]
    }

    /// Reflect point `z` across the hyperbolic geodesic through `a` and `b`
    /// (a circle orthogonal to the unit circle)
    fn reflect_across(z: [f64; 2], a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
        let va = (a[0] * a[0] + a[1] * a[1] - 1.0) * 0.5;
        let vb = (b[0] * b[0] + b[1] * b[1] - 1.0) * 0.5;
        let det = a[0] * b[1] - a[1] * b[0];
        if det.abs() < 1e-9 {
            return z;
        }
        let cx = (va * b[1] - vb * a[1]) / det;
        let cy = (a[0] * vb - b[0] * va) / det;
        let rr = cx * cx + cy * cy - 1.0; // squared orthogonal-circle radius
        let wx = z[0] - cx;
        let wy = z[1] - cy;
        let w2 = wx * wx + wy * wy;
        if w2 < 1e-12 {
            return z;
        }
        [cx + rr * wx / w2, cy + rr * wy / w2]
    }

    /// Generate {5,4} tiling centers: start with a central pentagon, then
    /// reflect its center across every geodesic side (BFS), deduplicating.
    /// Colors alternate against the parent cell so neighbors never match.
    fn generate_pentagon_centers(max_cells: usize) -> Vec<[f32; 4]> {
        const P: usize = 5;
        // Euclidean vertex radius of the central pentagon in the Poincaré
        // disk for a {5,4} tiling: tanh(acosh(cos(pi/4)/sin(pi/5)) / 2)
        const VERT_R: f64 = 0.30796;
        const MIN_SEP: f64 = 0.25; // neighbor centers sit ~0.48 apart

        let base_verts: Vec<[f64; 2]> = (0..P)
            .map(|k| {
                let a = std::f64::consts::TAU * k as f64 / P as f64;
                [VERT_R * a.cos(), VERT_R * a.sin()]
            })
            .collect();

        let mut centers: Vec<[f64; 2]> = vec![[0.0, 0.0]];
        let mut colors: Vec<f32> = vec![0.0];
        let mut verts: Vec<Vec<[f64; 2]>> = vec![base_verts.clone()];

        let mut qi = 0usize;
        while qi < centers.len() && centers.len() < max_cells {
            let c = centers[qi];
            let vs = verts[qi].clone();
            let pcol = colors[qi] as i32;

            for k in 0..P {
                let a = vs[k];
                let b = vs[(k + 1) % P];
                let nb = Self::reflect_across(c, a, b);

                // Deduplicate against every known center (euclidean is enough)
                let dup = centers.iter().any(|o| {
                    let dx = o[0] - nb[0];
                    let dy = o[1] - nb[1];
                    dx * dx + dy * dy < MIN_SEP * MIN_SEP
                });
                if dup {
                    continue;
                }

                // Color must differ from the parent cell
                let ncol = ((pcol + 1 + k as i32 % 4) % 5) as f32;

                let nverts: Vec<[f64; 2]> =
                    vs.iter().map(|v| Self::mobius(*v, nb)).collect();

                centers.push(nb);
                colors.push(ncol);
                verts.push(nverts);
            }
            qi += 1;
        }

        centers
            .iter()
            .zip(colors.iter())
            .map(|(c, col)| [c[0] as f32, c[1] as f32, *col, 0.0])
            .collect()
    }

    fn parse_palette(palette_config: &PaletteConfig) -> Result<Vec<[f32; 4]>> {
        match palette_config {
            PaletteConfig::Named(name) => {
                // Built-in palettes
                match name.as_str() {
                    "default" | "noctalia" => Ok(vec![
                        [0.05, 0.05, 0.15, 1.0],  // Deep background
                        [0.15, 0.1, 0.3, 1.0],    // Dark accent
                        [0.3, 0.2, 0.5, 1.0],     // Mid tone
                        [0.6, 0.4, 0.8, 1.0],     // Light accent
                        [0.9, 0.7, 1.0, 1.0],     // Highlight
                    ]),
                    "monochrome" => Ok(vec![
                        [0.05, 0.05, 0.05, 1.0],
                        [0.15, 0.15, 0.15, 1.0],
                        [0.3, 0.3, 0.3, 1.0],
                        [0.6, 0.6, 0.6, 1.0],
                        [0.9, 0.9, 0.9, 1.0],
                    ]),
                    "solarized" => Ok(vec![
                        [0.0, 0.17, 0.21, 1.0],   // base03
                        [0.03, 0.21, 0.26, 1.0],  // base02
                        [0.35, 0.43, 0.45, 1.0],  // base01
                        [0.58, 0.63, 0.63, 1.0],  // base00
                        [0.83, 0.85, 0.82, 1.0],  // base0
                    ]),
                    _ => Ok(vec![
                        [0.05, 0.05, 0.15, 1.0],
                        [0.15, 0.1, 0.3, 1.0],
                        [0.3, 0.2, 0.5, 1.0],
                        [0.6, 0.4, 0.8, 1.0],
                        [0.9, 0.7, 1.0, 1.0],
                    ]),
                }
            }
            PaletteConfig::Custom(colors) => {
                let mut palette = Vec::new();
                for color in colors {
                    let c = Self::parse_hex_color(color)?;
                    palette.push(c);
                }
                if palette.is_empty() {
                    anyhow::bail!("Custom palette must have at least one color");
                }
                Ok(palette)
            }
        }
    }

    fn parse_hex_color(hex: &str) -> Result<[f32; 4]> {
        let hex = hex.trim_start_matches('#');
        let (r, g, b, a) = match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16)?;
                let g = u8::from_str_radix(&hex[2..4], 16)?;
                let b = u8::from_str_radix(&hex[4..6], 16)?;
                (r, g, b, 255)
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16)?;
                let g = u8::from_str_radix(&hex[2..4], 16)?;
                let b = u8::from_str_radix(&hex[4..6], 16)?;
                let a = u8::from_str_radix(&hex[6..8], 16)?;
                (r, g, b, a)
            }
            _ => anyhow::bail!("Invalid hex color: {}", hex),
        };
        Ok([
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        ])
    }
}

impl WallpaperModule for HyperbolicModule {
    fn new(modules: &ModuleConfigs) -> Result<Self> {
        Self::new(&modules.hyperbolic)
    }

    fn update(&mut self, frame: &FrameData) {
        // Update pan angle based on camera movement and time
        self.pan_angle += self.config.pan_rate * (1.0 / 60.0); // Approximate dt
        
        // Could also incorporate camera delta for parallax
        let camera_delta = (frame.camera.0 - 0.0).abs() + (frame.camera.1 - 0.0).abs();
        self.pan_angle += camera_delta * 0.001;

        // Smooth palette transition when config changes
        if self.transition < 1.0 {
            self.transition = (self.transition + self.transition_speed * (1.0 / 60.0)).min(1.0);
            let len = self.palette.len().min(self.target_palette.len());
            for i in 0..len {
                self.palette[i] = lerp_color(self.palette[i], self.target_palette[i], self.transition);
            }
        }
    }

    fn pipeline(&self) -> &wgpu::RenderPipeline {
        self.pipeline.as_ref().unwrap()
    }

    fn bind_group(&self) -> &wgpu::BindGroup {
        self.bind_group.as_ref().unwrap()
    }

    fn ensure_pipeline(&mut self, renderer: &Renderer) -> Result<()> {
        if self.pipeline.is_none() {
            self.create_pipeline(renderer.device(), renderer.surface_format(), self.width, self.height)?;
        }
        Ok(())
    }

    fn update_uniforms(&mut self, renderer: &Renderer, frame: &FrameData) -> Result<()> {
        self.write_uniforms(renderer, frame)
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        // Pipeline will be recreated on next render
        self.pipeline = None;
    }

    fn reload_config(&mut self, modules: &ModuleConfigs) -> Result<()> {
        self.config = modules.hyperbolic.clone();
        let new_palette = Self::parse_palette(&self.config.palette)?;
        // Start smooth transition to new palette
        self.target_palette = new_palette;
        self.transition = 0.0;
        self.pipeline = None; // Force recreation
        Ok(())
    }

    fn name(&self) -> &str {
        "hyperbolic"
    }
}

/// Linear interpolation between two RGBA colors
fn lerp_color(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}

impl HyperbolicModule {
    fn create_pipeline(&mut self, device: &wgpu::Device, surface_format: wgpu::TextureFormat, width: u32, height: u32) -> Result<()> {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Hyperbolic Wallpaper Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/hyperbolic.wgsl").into()),
        });

        // Create uniform buffer (shader declares @group(0) @binding(0) uniform)
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Hyperbolic Uniforms"),
            size: 48, // sizeof(Uniforms) = 40, rounded to 16-byte alignment
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create bind group layout matching the shader's uniform declaration
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Hyperbolic Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(std::num::NonZeroU64::new(40).unwrap()),
                    },
                    count: None,
                },
            ],
        });

        // Create bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Hyperbolic Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
            ],
        });

        // Create pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Hyperbolic Pipeline Layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        // Create render pipeline
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Hyperbolic Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        self.pipeline = Some(pipeline);
        self.bind_group = Some(bind_group);
        self.uniform_buffer = Some(uniform_buffer);
        self.width = width;
        self.height = height;

        Ok(())
    }

    /// Write per-frame uniform data into the uniform buffer
    fn write_uniforms(&mut self, renderer: &Renderer, frame: &FrameData) -> Result<()> {
        let uniform_buffer = self.uniform_buffer.as_ref().context("No uniform buffer")?;
        let uniforms = HyperbolicUniforms {
            resolution: [frame.width as f32, frame.height as f32],
            camera: [frame.camera.0 as f32, frame.camera.1 as f32],
            zoom: frame.zoom as f32,
            time: frame.time as f32,
            pan_angle: self.pan_angle as f32,
            pan_rate: self.config.pan_rate as f32,
            rmax: self.config.rmax as f32,
            _padding: [0; 3],
        };
        renderer.queue().write_buffer(uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
        Ok(())
    }

    fn palette_to_array(&self) -> [[f32; 4]; 16] {
        let mut arr = [[0.0, 0.0, 0.0, 1.0]; 16];
        for (i, color) in self.palette.iter().enumerate().take(16) {
            arr[i] = *color;
        }
        arr
    }
}

/// Solid color wallpaper module
pub struct SolidModule {
    color: [f32; 4],
    pipeline: Option<wgpu::RenderPipeline>,
    bind_group: Option<wgpu::BindGroup>,
    uniform_buffer: Option<wgpu::Buffer>,
}

impl SolidModule {
    pub fn new(config: &crate::config::SolidConfig) -> Result<Self> {
        let color = Self::parse_hex_color(&config.color)?;
        Ok(Self {
            color,
            pipeline: None,
            bind_group: None,
            uniform_buffer: None,
        })
    }

    fn parse_hex_color(hex: &str) -> Result<[f32; 4]> {
        let hex = hex.trim_start_matches('#');
        let (r, g, b, a) = match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16)?;
                let g = u8::from_str_radix(&hex[2..4], 16)?;
                let b = u8::from_str_radix(&hex[4..6], 16)?;
                (r, g, b, 255)
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16)?;
                let g = u8::from_str_radix(&hex[2..4], 16)?;
                let b = u8::from_str_radix(&hex[4..6], 16)?;
                let a = u8::from_str_radix(&hex[6..8], 16)?;
                (r, g, b, a)
            }
            _ => anyhow::bail!("Invalid hex color: {}", hex),
        };
        Ok([
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        ])
    }
}

impl WallpaperModule for SolidModule {
    fn new(modules: &ModuleConfigs) -> Result<Self> {
        Self::new(&modules.solid)
    }

    fn update(&mut self, _frame: &FrameData) {}

    fn pipeline(&self) -> &wgpu::RenderPipeline {
        self.pipeline.as_ref().unwrap()
    }

    fn bind_group(&self) -> &wgpu::BindGroup {
        self.bind_group.as_ref().unwrap()
    }

    fn ensure_pipeline(&mut self, renderer: &Renderer) -> Result<()> {
        if self.pipeline.is_none() {
            self.create_pipeline(renderer.device(), renderer.surface_format())?;
        }
        Ok(())
    }

    fn update_uniforms(&mut self, renderer: &Renderer, _frame: &FrameData) -> Result<()> {
        let uniform_buffer = self.uniform_buffer.as_ref().context("No uniform buffer")?;
        let uniforms = SolidUniforms { color: self.color };
        renderer.queue().write_buffer(uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
        Ok(())
    }

    fn resize(&mut self, _width: u32, _height: u32) {
        self.pipeline = None;
    }

    fn reload_config(&mut self, modules: &ModuleConfigs) -> Result<()> {
        self.color = Self::parse_hex_color(&modules.solid.color)?;
        self.pipeline = None;
        Ok(())
    }

    fn name(&self) -> &str {
        "solid"
    }
}

impl SolidModule {
    fn create_pipeline(&mut self, device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Result<()> {
        
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Solid Color Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/solid.wgsl").into()),
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Solid Uniforms"),
            size: std::mem::size_of::<SolidUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Solid Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Solid Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Solid Pipeline Layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Solid Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        self.pipeline = Some(pipeline);
        self.bind_group = Some(bind_group);
        self.uniform_buffer = Some(uniform_buffer);

        Ok(())
    }
}

/// Image wallpaper module
pub struct ImageModule {
    config: crate::config::ImageConfig,
    pipeline: Option<wgpu::RenderPipeline>,
    bind_group: Option<wgpu::BindGroup>,
    texture: Option<wgpu::Texture>,
    sampler: Option<wgpu::Sampler>,
}

impl ImageModule {
    pub fn new(config: &crate::config::ImageConfig) -> Result<Self> {
        Ok(Self {
            config: config.clone(),
            pipeline: None,
            bind_group: None,
            texture: None,
            sampler: None,
        })
    }
}

impl WallpaperModule for ImageModule {
    fn new(modules: &ModuleConfigs) -> Result<Self> {
        Self::new(&modules.image)
    }

    fn update(&mut self, _frame: &FrameData) {}

    fn pipeline(&self) -> &wgpu::RenderPipeline {
        self.pipeline.as_ref().unwrap()
    }

    fn bind_group(&self) -> &wgpu::BindGroup {
        self.bind_group.as_ref().unwrap()
    }

    fn ensure_pipeline(&mut self, renderer: &Renderer) -> Result<()> {
        if self.pipeline.is_none() {
            self.create_pipeline(renderer.device(), renderer.queue(), renderer.surface_format())?;
        }
        Ok(())
    }

    fn update_uniforms(&mut self, _renderer: &Renderer, _frame: &FrameData) -> Result<()> {
        Ok(()) // Image module binds texture+sampler, no uniform buffer
    }

    fn resize(&mut self, _width: u32, _height: u32) {
        self.pipeline = None;
    }

    fn reload_config(&mut self, modules: &ModuleConfigs) -> Result<()> {
        self.config = modules.image.clone();
        self.pipeline = None;
        self.texture = None;
        Ok(())
    }

    fn name(&self) -> &str {
        "image"
    }
}

impl ImageModule {
    fn create_pipeline(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, surface_format: wgpu::TextureFormat) -> Result<()> {
        // Load image if path provided
        if let Some(path) = &self.config.path {
            let image = image::open(path)?.to_rgba8();
            let dimensions = image.dimensions();
            
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Wallpaper Image"),
                size: wgpu::Extent3d {
                    width: dimensions.0,
                    height: dimensions.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &image,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * dimensions.0),
                    rows_per_image: Some(dimensions.1),
                },
                wgpu::Extent3d {
                    width: dimensions.0,
                    height: dimensions.1,
                    depth_or_array_layers: 1,
                },
            );

            let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Nearest,
                ..Default::default()
            });

            let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

            let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Image Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Image Bind Group"),
                layout: &bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&texture_view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
                ],
            });

            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Image Pipeline Layout"),
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            });

            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Image Shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("shaders/image.wgsl").into()),
            });

            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Image Render Pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: surface_format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleStrip,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    unclipped_depth: false,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });

            self.pipeline = Some(pipeline);
            self.bind_group = Some(bind_group);
            self.texture = Some(texture);
            self.sampler = Some(sampler);
        }

        Ok(())
    }
}

/// Uniforms for spherical shader (positive curvature / elliptic geometry)
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct SphericalUniforms {
    resolution: [f32; 2],
    camera: [f32; 2],
    zoom: f32,
    time: f32,
    pan_angle: f32,
    pan_rate: f32,
    radius: f32,
    _padding1: [u32; 3],          // align palette_colors to 16 bytes
    palette_colors: [[f32; 4]; 32],
    palette_size: u32,
    mode: u32,
    _padding2: [u32; 2],          // align struct size to 16 bytes
}

/// Spherical wallpaper module — positive curvature tiling on a sphere
pub struct SphericalModule {
    config: SphericalConfig,
    pipeline: Option<wgpu::RenderPipeline>,
    bind_group: Option<wgpu::BindGroup>,
    uniform_buffer: Option<wgpu::Buffer>,
    width: u32,
    height: u32,
    pan_angle: f32,
    palette: Vec<[f32; 4]>,
    target_palette: Vec<[f32; 4]>,
    transition: f32,
    transition_speed: f32,
}

impl SphericalModule {
    pub fn new(config: &SphericalConfig) -> Result<Self> {
        let palette = Self::parse_palette(&config.palette)?;

        Ok(Self {
            config: config.clone(),
            pipeline: None,
            bind_group: None,
            uniform_buffer: None,
            width: 0,
            height: 0,
            pan_angle: 0.0,
            palette: palette.clone(),
            target_palette: palette,
            transition: 1.0,
            transition_speed: 0.5,
        })
    }

    fn parse_palette(palette_config: &PaletteConfig) -> Result<Vec<[f32; 4]>> {
        match palette_config {
            PaletteConfig::Named(name) => {
                match name.as_str() {
                    "default" | "noctalia" => Ok(vec![
                        [0.05, 0.05, 0.15, 1.0],  // Deep background
                        [0.15, 0.1, 0.3, 1.0],    // Dark accent
                        [0.3, 0.2, 0.5, 1.0],     // Mid tone
                        [0.6, 0.4, 0.8, 1.0],     // Light accent
                        [0.9, 0.7, 1.0, 1.0],     // Highlight
                    ]),
                    "monochrome" => Ok(vec![
                        [0.05, 0.05, 0.05, 1.0],
                        [0.15, 0.15, 0.15, 1.0],
                        [0.3, 0.3, 0.3, 1.0],
                        [0.6, 0.6, 0.6, 1.0],
                        [0.9, 0.9, 0.9, 1.0],
                    ]),
                    "solarized" => Ok(vec![
                        [0.0, 0.17, 0.21, 1.0],   // base03
                        [0.03, 0.21, 0.26, 1.0],  // base02
                        [0.35, 0.43, 0.45, 1.0],  // base01
                        [0.58, 0.63, 0.63, 1.0],  // base00
                        [0.83, 0.85, 0.82, 1.0],  // base0
                    ]),
                    "sphere" => Ok(vec![
                        [0.1, 0.2, 0.4, 1.0],     // Deep ocean
                        [0.2, 0.4, 0.8, 1.0],     // Ocean
                        [0.4, 0.6, 0.9, 1.0],     // Light ocean
                        [0.8, 0.9, 1.0, 1.0],     // Foam
                        [1.0, 0.8, 0.6, 1.0],     // Sand
                        [0.6, 0.4, 0.2, 1.0],     // Earth
                        [0.2, 0.5, 0.2, 1.0],     // Vegetation
                        [0.9, 0.9, 0.7, 1.0],     // Cloud
                    ]),
                    _ => Ok(vec![
                        [0.05, 0.05, 0.15, 1.0],
                        [0.15, 0.1, 0.3, 1.0],
                        [0.3, 0.2, 0.5, 1.0],
                        [0.6, 0.4, 0.8, 1.0],
                        [0.9, 0.7, 1.0, 1.0],
                    ]),
                }
            }
            PaletteConfig::Custom(colors) => {
                let mut palette = Vec::new();
                for color in colors {
                    let c = Self::parse_hex_color(color)?;
                    palette.push(c);
                }
                if palette.is_empty() {
                    anyhow::bail!("Custom palette must have at least one color");
                }
                Ok(palette)
            }
        }
    }

    fn parse_hex_color(hex: &str) -> Result<[f32; 4]> {
        let hex = hex.trim_start_matches('#');
        let (r, g, b, a) = match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16)?;
                let g = u8::from_str_radix(&hex[2..4], 16)?;
                let b = u8::from_str_radix(&hex[4..6], 16)?;
                (r, g, b, 255)
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16)?;
                let g = u8::from_str_radix(&hex[2..4], 16)?;
                let b = u8::from_str_radix(&hex[4..6], 16)?;
                let a = u8::from_str_radix(&hex[6..8], 16)?;
                (r, g, b, a)
            }
            _ => anyhow::bail!("Invalid hex color: {}", hex),
        };
        Ok([
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        ])
    }

    fn create_pipeline(&mut self, device: &wgpu::Device, surface_format: wgpu::TextureFormat, width: u32, height: u32) -> Result<()> {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Spherical Wallpaper Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/spherical.wgsl").into()),
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Spherical Uniforms"),
            size: std::mem::size_of::<SphericalUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Spherical Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Spherical Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Spherical Pipeline Layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Spherical Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        self.pipeline = Some(pipeline);
        self.bind_group = Some(bind_group);
        self.uniform_buffer = Some(uniform_buffer);
        self.width = width;
        self.height = height;

        Ok(())
    }

    fn palette_to_array(&self) -> [[f32; 4]; 32] {
        let mut arr = [[0.0, 0.0, 0.0, 1.0]; 32];
        for (i, color) in self.palette.iter().enumerate().take(32) {
            arr[i] = *color;
        }
        arr
    }
}

impl WallpaperModule for SphericalModule {
    fn new(modules: &ModuleConfigs) -> Result<Self> {
        Self::new(&modules.spherical)
    }

    fn update(&mut self, frame: &FrameData) {
        self.pan_angle += self.config.pan_rate * (1.0 / 60.0);
        let camera_delta = (frame.camera.0.abs() + frame.camera.1.abs()) as f32 * 0.001;
        self.pan_angle += camera_delta;

        // Smooth palette transition when config changes
        if self.transition < 1.0 {
            self.transition = (self.transition + self.transition_speed * (1.0 / 60.0)).min(1.0);
            let len = self.palette.len().min(self.target_palette.len());
            for i in 0..len {
                self.palette[i] = lerp_color(self.palette[i], self.target_palette[i], self.transition);
            }
        }
    }

    fn pipeline(&self) -> &wgpu::RenderPipeline {
        self.pipeline.as_ref().unwrap()
    }

    fn bind_group(&self) -> &wgpu::BindGroup {
        self.bind_group.as_ref().unwrap()
    }

    fn ensure_pipeline(&mut self, renderer: &Renderer) -> Result<()> {
        if self.pipeline.is_none() {
            self.create_pipeline(renderer.device(), renderer.surface_format(), self.width, self.height)?;
        }
        Ok(())
    }

    fn update_uniforms(&mut self, renderer: &Renderer, frame: &FrameData) -> Result<()> {
        let uniform_buffer = self.uniform_buffer.as_ref().context("No uniform buffer")?;
        let uniforms = SphericalUniforms {
            resolution: [frame.width as f32, frame.height as f32],
            camera: [frame.camera.0 as f32, frame.camera.1 as f32],
            zoom: frame.zoom as f32,
            time: frame.time as f32,
            pan_angle: self.pan_angle,
            pan_rate: self.config.pan_rate,
            radius: self.config.radius,
            _padding1: [0; 3],
            palette_colors: self.palette_to_array(),
            palette_size: self.palette.len() as u32,
            mode: 0,
            _padding2: [0; 2],
        };
        renderer.queue().write_buffer(uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
        Ok(())
    }

    fn resize(&mut self, _width: u32, _height: u32) {
        self.pipeline = None;
    }

    fn reload_config(&mut self, modules: &ModuleConfigs) -> Result<()> {
        self.config = modules.spherical.clone();
        let new_palette = Self::parse_palette(&self.config.palette)?;
        self.target_palette = new_palette;
        self.transition = 0.0;
        self.pipeline = None;
        Ok(())
    }

    fn name(&self) -> &str {
        "spherical"
    }
}