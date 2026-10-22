//! wgpu renderer for wallpaper - renders to offscreen texture, presents via SHM

use anyhow::{Result, Context};
use wgpu::{Device, Queue, TextureFormat, Texture, TextureView, TextureDescriptor, TextureDimension, TextureUsages, Extent3d, CommandEncoder, RenderPass, RenderPassDescriptor, RenderPassColorAttachment, LoadOp, StoreOp, Color, TextureViewDescriptor, BindGroup, RenderPipeline, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, BufferBindingType, ShaderStages, BindGroupDescriptor, BindGroupEntry, BindingResource, BufferSize, VertexState, FragmentState, ColorTargetState, BlendState, ColorWrites, PrimitiveState, PrimitiveTopology, FrontFace, PolygonMode, MultisampleState, DepthStencilState, TextureAspect, Origin3d, TexelCopyTextureInfo, TexelCopyBufferLayout, Extent3d as Extent3dCopy, TextureViewDimension, TextureSampleType, SamplerBindingType, FilterMode, AddressMode, SamplerDescriptor, Sampler, MipmapFilterMode, ShaderModule, ShaderModuleDescriptor};
use std::sync::{Arc, OnceLock};

/// Frame data passed to wallpaper modules for rendering
#[derive(Debug, Clone)]
pub struct FrameData {
    pub camera: (f64, f64),
    pub zoom: f64,
    pub outputs: Vec<OutputState>,
    pub windows: Vec<WindowState>,
    pub time: f64,
    pub frame_count: u64,
    pub width: u32,
    pub height: u32,
}

/// Output state for rendering
#[derive(Debug, Clone)]
pub struct OutputState {
    pub name: String,
    pub camera: (f64, f64),
    pub zoom: f64,
    pub size: [i32; 2],
    pub active: bool,
}

/// Window state for rendering
#[derive(Debug, Clone)]
pub struct WindowState {
    pub id: u64,
    pub app_id: String,
    pub position: [i32; 2],
    pub size: [i32; 2],
    pub is_focused: bool,
}

pub struct Renderer {
    device: Arc<Device>,
    queue: Arc<Queue>,
    surface_format: TextureFormat,
    width: u32,
    height: u32,
    // Offscreen render texture
    render_texture: Option<Texture>,
    render_texture_view: Option<TextureView>,
    // Staging buffer for CPU readback
    staging_buffer: Option<wgpu::Buffer>,
    // Shader modules cache
    hyperbolic_shader: OnceLock<ShaderModule>,
    solid_shader: OnceLock<ShaderModule>,
    image_shader: OnceLock<ShaderModule>,
    // Sampler for image module
    sampler: OnceLock<wgpu::Sampler>,
}

impl Renderer {
    /// Create a new renderer (headless, no surface)
    pub async fn new(config: &crate::config::RenderConfig) -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            flags: wgpu::InstanceFlags::default(),
            backend_options: wgpu::BackendOptions::default(),
            display: None,
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: match config.power_preference.as_str() {
                    "low" => wgpu::PowerPreference::LowPower,
                    _ => wgpu::PowerPreference::HighPerformance,
                },
                force_fallback_adapter: false,
                compatible_surface: None,
                apply_limit_buckets: false,
            })
            .await
            .context("Failed to find GPU adapter")?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Wallpaper Renderer"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::default(),
                    experimental_features: wgpu::ExperimentalFeatures::disabled(),
                    trace: wgpu::Trace::Off,
                },
            )
            .await?;

        // Use a standard format for offscreen rendering
        let surface_format = TextureFormat::Rgba8UnormSrgb;

        Ok(Self {
            device: Arc::new(device),
            queue: Arc::new(queue),
            surface_format,
            width: 0,
            height: 0,
            render_texture: None,
            render_texture_view: None,
            staging_buffer: None,
            hyperbolic_shader: OnceLock::new(),
            solid_shader: OnceLock::new(),
            image_shader: OnceLock::new(),
            sampler: OnceLock::new(),
        })
    }

    pub fn device(&self) -> &Device {
        &self.device
    }

    pub fn queue(&self) -> &Queue {
        &self.queue
    }

    pub fn surface_format(&self) -> TextureFormat {
        self.surface_format
    }

    /// Resize the renderer
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == self.width && height == self.height {
            return;
        }
        self.width = width;
        self.height = height;
        self.render_texture = None;
        self.render_texture_view = None;
        self.staging_buffer = None;
    }

    /// Get or create the render texture
    fn get_render_texture(&mut self) -> &TextureView {
        if self.render_texture_view.is_none() || 
           self.render_texture.as_ref().map(|t| t.width() != self.width || t.height() != self.height).unwrap_or(true) {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Wallpaper Render Target"),
                size: wgpu::Extent3d {
                    width: self.width,
                    height: self.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.surface_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });

            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.render_texture = Some(texture);
            self.render_texture_view = Some(view);
        }

        self.render_texture_view.as_ref().unwrap()
    }

    /// Get or create staging buffer for CPU readback
    fn get_staging_buffer(&mut self) -> &wgpu::Buffer {
        let buffer_size = (self.width * self.height * 4) as u64; // BGRA8 = 4 bytes per pixel
        
        if self.staging_buffer.is_none() || 
           self.staging_buffer.as_ref().unwrap().size() < buffer_size {
            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Wallpaper Staging Buffer"),
                size: buffer_size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            self.staging_buffer = Some(buffer);
        }

        self.staging_buffer.as_ref().unwrap()
    }

    /// Begin a render pass to the offscreen texture
    pub fn begin_render_pass(&mut self, pipeline: &wgpu::RenderPipeline, bind_group: &wgpu::BindGroup) -> Result<()> {
        let texture_view = self.get_render_texture().clone();

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Wallpaper Render Encoder"),
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Wallpaper Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &texture_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            render_pass.set_pipeline(pipeline);
            render_pass.set_bind_group(0, bind_group, &[]);
            render_pass.draw(0..4, 0..1); // Full-screen quad (triangle strip)
        }

        self.queue.submit(Some(encoder.finish()));
        Ok(())
    }

    /// Copy render texture to staging buffer and map for CPU access
    pub fn copy_to_cpu(&mut self) -> Result<Vec<u8>> {
        // Ensure staging buffer exists (mutable borrow)
        let _staging_buffer = self.get_staging_buffer();
        
        // Now get the texture (immutable borrow)
        let texture = self.render_texture.as_ref().context("No render texture")?;

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Wallpaper Copy Encoder"),
        });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.staging_buffer.as_ref().unwrap(),
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * self.width),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );

        self.queue.submit(Some(encoder.finish()));

        // Map buffer and read data
        let buffer_slice = self.staging_buffer.as_ref().unwrap().slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap();
        });
        let _ = self.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None });
        receiver.recv().unwrap().context("Failed to map staging buffer")?;

        let data = buffer_slice.get_mapped_range().context("Failed to get mapped range")?;
        let result = data.to_vec();
        drop(data);
        self.staging_buffer.as_ref().unwrap().unmap();

        Ok(result)
    }

    /// Get or create hyperbolic shader
    pub fn get_hyperbolic_shader(&self) -> &ShaderModule {
        self.hyperbolic_shader.get_or_init(|| {
            self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Hyperbolic Shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("shaders/hyperbolic.wgsl").into()),
            })
        })
    }

    /// Get or create solid color shader
    pub fn get_solid_shader(&self) -> &ShaderModule {
        self.solid_shader.get_or_init(|| {
            self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Solid Shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("shaders/solid.wgsl").into()),
            })
        })
    }

    /// Get or create image shader
    pub fn get_image_shader(&self) -> &ShaderModule {
        self.image_shader.get_or_init(|| {
            self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Image Shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("shaders/image.wgsl").into()),
            })
        })
    }

    /// Get or create sampler
    pub fn get_sampler(&self) -> &wgpu::Sampler {
        self.sampler.get_or_init(|| {
            self.device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Nearest,
                ..Default::default()
            })
        })
    }

    /// Create hyperbolic pipeline
    pub fn create_hyperbolic_pipeline(&mut self, bind_group_layout: &wgpu::BindGroupLayout) -> wgpu::RenderPipeline {
        let surface_format = self.surface_format;
        let shader = self.get_hyperbolic_shader();
        let pipeline_layout = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Hyperbolic Pipeline Layout"),
            bind_group_layouts: &[Some(bind_group_layout)],
            immediate_size: 0,
        });

        self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Hyperbolic Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
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
        })
    }

    /// Create solid pipeline
    pub fn create_solid_pipeline(&mut self, bind_group_layout: &wgpu::BindGroupLayout) -> wgpu::RenderPipeline {
        let surface_format = self.surface_format;
        let shader = self.get_solid_shader();
        let pipeline_layout = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Solid Pipeline Layout"),
            bind_group_layouts: &[Some(bind_group_layout)],
            immediate_size: 0,
        });

        self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Solid Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
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
        })
    }

    /// Create image pipeline
    pub fn create_image_pipeline(&mut self, bind_group_layout: &wgpu::BindGroupLayout) -> wgpu::RenderPipeline {
        let surface_format = self.surface_format;
        let shader = self.get_image_shader();
        let pipeline_layout = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Image Pipeline Layout"),
            bind_group_layouts: &[Some(bind_group_layout)],
            immediate_size: 0,
        });

        self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Image Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
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
        })
    }
}