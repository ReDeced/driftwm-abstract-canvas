//! Wayland layer-shell integration with SHM presentation for the wallpaper

use crate::config::LayerConfig;
use anyhow::{Result, Context};
use calloop::{EventLoop};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_registry,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryHandler, RegistryState},
    registry_handlers,
    seat::{SeatHandler, SeatState},
    shell::wlr_layer::{LayerShell as SctkLayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure, Anchor, KeyboardInteractivity, Layer},
    shell::WaylandSurface,
    shm::{Shm, ShmHandler},
    shm::slot::{Slot, SlotPool},
};
use tracing::{debug, info, warn};
use wayland_client::{Connection, QueueHandle, Proxy};
use wayland_client::protocol::wl_output::WlOutput;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::protocol::wl_output::Transform;
use wayland_client::protocol::wl_seat::{WlSeat, Capability};
use wayland_client::protocol::wl_shm;
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::ZwlrLayerSurfaceV1;

/// Events from the layer shell
#[derive(Debug, Clone)]
pub enum LayerEvent {
    RedrawRequested { width: u32, height: u32 },
    Configure { width: u32, height: u32 },
    Close,
}

/// Layer shell surface wrapper with SHM presentation
pub struct LayerShell {
    connection: Connection,
    event_queue: wayland_client::EventQueue<LayerShellData>,
    layer_surface: Option<LayerSurface>,
    config: LayerConfig,
    width: u32,
    height: u32,
    data: LayerShellData,
    event_rx: crossbeam::channel::Receiver<LayerEvent>,
    event_tx: crossbeam::channel::Sender<LayerEvent>,
    // SHM pool for buffer allocation
    shm_pool: Option<SlotPool>,
    // Current buffer slot
    current_slot: Option<Slot>,
    // Pending frame data
    pending_frame: Option<Vec<u8>>,
    // Frame dimensions
    frame_width: u32,
    frame_height: u32,
}

impl LayerShell {
    /// Create a new layer shell surface with SHM presentation
    pub async fn new(config: &LayerConfig) -> Result<Self> {
        let (event_tx, event_rx) = crossbeam::channel::unbounded::<LayerEvent>();

        // Connect to Wayland
        let connection = Connection::connect_to_env()?;
        let (globals, mut event_queue) = wayland_client::globals::registry_queue_init::<LayerShellData>(&connection)?;

        let qh = event_queue.handle();
        let compositor = CompositorState::bind(&globals, &qh)?;
        let layer_shell = smithay_client_toolkit::shell::wlr_layer::LayerShell::bind(&globals, &qh)?;
        let shm = Shm::bind(&globals, &qh)?;
        let output_state = OutputState::new(&globals, &qh);
        let seat_state = SeatState::new(&globals, &qh);

        let event_tx_clone = event_tx.clone();
        let mut data = LayerShellData {
            connection: connection.clone(),
            compositor,
            layer_shell,
            shm,
            output_state,
            seat_state,
            registry_state: RegistryState::new(&globals),
            queue_handle: qh.clone(),
            config: config.clone(),
            layer_surface: None,
            width: 0,
            height: 0,
            closed: false,
            event_tx,
        };

        // Roundtrip to ensure globals are bound
        event_queue.blocking_dispatch(&mut data)?;
        connection.flush()?;

        // Wait for outputs
        let mut attempts = 0;
        while data.output_state.outputs().count() == 0 && attempts < 100 {
            event_queue.blocking_dispatch(&mut data)?;
            std::thread::sleep(std::time::Duration::from_millis(10));
            attempts += 1;
        }

        if data.output_state.outputs().count() == 0 {
            anyhow::bail!("No outputs found");
        }

        // Create layer surface for the first output
        let outputs = data.output_state.outputs().collect::<Vec<_>>();
        let output = &outputs[0];

        let layer = Layer::Background;
        let namespace = Some(config.namespace.clone());

        let surface = data.compositor.create_surface(&qh);
        let layer_surface = data.layer_shell.create_layer_surface(
            &qh,
            surface,
            layer,
            namespace,
            Some(output),
        );

        let anchors = config.anchors.iter()
            .filter_map(|a| match a.as_str() {
                "top" => Some(Anchor::TOP),
                "bottom" => Some(Anchor::BOTTOM),
                "left" => Some(Anchor::LEFT),
                "right" => Some(Anchor::RIGHT),
                _ => None,
            })
            .fold(Anchor::empty(), |acc, a| acc | a);

        layer_surface.set_anchor(anchors);
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer_surface.set_exclusive_zone(config.exclusive_zone);

        data.layer_surface = Some(layer_surface.clone());

        // Initialize SHM pool
        let shm_pool = SlotPool::new(1920 * 1080 * 4, &data.shm)?;

        // Initial commit so the compositor sends us a configure event
        if let Some(ref ls) = data.layer_surface {
            ls.wl_surface().commit();
        }

        Ok(Self {
            connection,
            event_queue,
            layer_surface: Some(layer_surface),
            config: config.clone(),
            width: 0,
            height: 0,
            data,
            event_tx: event_tx_clone,
            event_rx,
            shm_pool: Some(shm_pool),
            current_slot: None,
            pending_frame: None,
            frame_width: 0,
            frame_height: 0,
        })
    }

    /// Get the next event
    pub fn try_next_event(&mut self) -> Option<LayerEvent> {
        self.event_rx.try_recv().ok()
    }

    /// Run one iteration of the event loop (non-blocking)
    pub fn dispatch_pending(&mut self) -> Result<()> {
        self.event_queue.dispatch_pending(&mut self.data)?;
        Ok(())
    }

    /// Request a redraw
    pub fn request_redraw(&self) {
        if let Some(surface) = &self.layer_surface {
            let _callback = surface.wl_surface().frame(&self.event_queue.handle(), 
                smithay_client_toolkit::compositor::FrameCallbackData(surface.wl_surface().clone()));
            surface.wl_surface().commit();
        }
    }

    /// Acknowledge configure
    pub fn ack_configure(&self) {
        if let Some(surface) = &self.layer_surface {
            surface.wl_surface().commit();
        }
    }

    /// Present a frame (RGBA pixels) via SHM
    pub fn present_shm(&mut self, pixels: Vec<u8>, width: u32, height: u32) -> Result<()> {
        if pixels.len() != (width * height * 4) as usize {
            anyhow::bail!("Invalid pixel data size: expected {}, got {}", width * height * 4, pixels.len());
        }

        let stride = (width * 4) as i32;

        // Ensure SHM pool is large enough
        let pool_size = (width * height * 4) as usize;
        if self.shm_pool.as_ref().map(|p| p.len() < pool_size).unwrap_or(true) {
            let pool = SlotPool::new(pool_size, &self.data.shm)?;
            self.shm_pool = Some(pool);
        }

        // Create buffer and get canvas to write to
        let pool = self.shm_pool.as_mut().unwrap();
        let (buffer, canvas) = pool.create_buffer(
            width as i32,
            height as i32,
            stride,
            wl_shm::Format::Argb8888,
        )?;

        // Write pixel data to canvas
        canvas[..pixels.len()].copy_from_slice(&pixels);
        
        // Attach buffer to surface
        if let Some(surface) = &self.layer_surface {
            buffer.attach_to(&surface.wl_surface())?;
            surface.wl_surface().damage_buffer(0, 0, width as i32, height as i32);
            surface.wl_surface().commit();
        }

        self.width = width;
        self.height = height;

        Ok(())
    }

    /// Get the event receiver
    pub fn get_event_receiver(&self) -> &crossbeam::channel::Receiver<LayerEvent> {
        &self.event_rx
    }
}

/// Temporary data for initial layer surface creation
struct TempLayerData {
    compositor: CompositorState,
    layer_shell: smithay_client_toolkit::shell::wlr_layer::LayerShell,
    shm: Shm,
    output_state: OutputState,
    seat_state: SeatState,
    registry_state: RegistryState,
    queue_handle: QueueHandle<LayerShellData>,
}

impl ProvidesRegistryState for TempLayerData {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    
    fn runtime_add_global(&mut self, _: &Connection, _: &QueueHandle<Self>, _: u32, _: &str, _: u32) {}
    fn runtime_remove_global(&mut self, _: &Connection, _: &QueueHandle<Self>, _: u32, _: &str) {}
}

impl CompositorHandler for TempLayerData {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _new_factor: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _new_transform: Transform,
    ) {
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _output: &WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _output: &WlOutput,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _time: u32,
    ) {
    }
}

impl ShmHandler for TempLayerData {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl OutputHandler for TempLayerData {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: WlOutput,
    ) {
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: WlOutput,
    ) {
    }
}

impl SeatHandler for TempLayerData {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _: WlSeat) {}
    fn new_capability(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _: WlSeat,
        _: smithay_client_toolkit::seat::Capability,
    ) {
    }
    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _: WlSeat,
        _: smithay_client_toolkit::seat::Capability,
    ) {
    }
    fn remove_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _: WlSeat) {}
}

/// Internal data for the layer shell
struct LayerShellData {
    connection: Connection,
    compositor: CompositorState,
    layer_shell: smithay_client_toolkit::shell::wlr_layer::LayerShell,
    shm: Shm,
    output_state: OutputState,
    seat_state: SeatState,
    registry_state: RegistryState,
    queue_handle: QueueHandle<LayerShellData>,
    config: LayerConfig,
    layer_surface: Option<LayerSurface>,
    width: u32,
    height: u32,
    closed: bool,
    event_tx: crossbeam::channel::Sender<LayerEvent>,
}

impl LayerShellData {
    fn create_surface(&mut self, output: &WlOutput) {
        let layer = Layer::Background;
        let namespace = Some(self.config.namespace.clone());

        let surface = self.compositor.create_surface(&self.queue_handle);
        let layer_surface = self.layer_shell.create_layer_surface(
            &self.queue_handle,
            surface,
            layer,
            namespace,
            Some(output),
        );

        let anchors = self.config.anchors.iter()
            .filter_map(|a| match a.as_str() {
                "top" => Some(Anchor::TOP),
                "bottom" => Some(Anchor::BOTTOM),
                "left" => Some(Anchor::LEFT),
                "right" => Some(Anchor::RIGHT),
                _ => None,
            })
            .fold(Anchor::empty(), |acc, a| acc | a);

        layer_surface.set_anchor(anchors);
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer_surface.set_exclusive_zone(self.config.exclusive_zone);
        
        self.layer_surface = Some(layer_surface);
    }
}

impl ProvidesRegistryState for LayerShellData {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    
    registry_handlers![OutputState, SeatState];
}

impl CompositorHandler for LayerShellData {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _new_factor: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _new_transform: Transform,
    ) {
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _output: &WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _output: &WlOutput,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _time: u32,
    ) {
        let _ = self.event_tx.send(LayerEvent::RedrawRequested {
            width: self.width,
            height: self.height,
        });
    }
}

impl ShmHandler for LayerShellData {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl SeatHandler for LayerShellData {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _: WlSeat) {}
    fn new_capability(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _: WlSeat,
        _: smithay_client_toolkit::seat::Capability,
    ) {
    }
    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _: WlSeat,
        _: smithay_client_toolkit::seat::Capability,
    ) {
    }
    fn remove_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _: WlSeat) {}
}

impl LayerShellHandler for LayerShellData {
    fn closed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
    ) {
        self.closed = true;
        let _ = self.event_tx.send(LayerEvent::Close);
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        self.width = if configure.new_size.0 == 0 { self.width } else { configure.new_size.0 };
        self.height = if configure.new_size.1 == 0 { self.height } else { configure.new_size.1 };
        let _ = self.event_tx.send(LayerEvent::Configure {
            width: self.width,
            height: self.height,
        });
    }
}

impl OutputHandler for LayerShellData {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: WlOutput,
    ) {
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: WlOutput,
    ) {
    }
}

// Delegate dispatch for layer shell
smithay_client_toolkit::delegate_dispatch2!(LayerShellData);

// Delegate registry
smithay_client_toolkit::delegate_registry!(LayerShellData);