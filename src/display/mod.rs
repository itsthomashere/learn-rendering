use crate::renderer::Terminal;
use std::sync::Arc;
use term::data::{Color, RGBA};
use tokio::runtime::Runtime;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::window::Window;

#[derive(Debug)]
pub struct Display<'t> {
    text_width: u32,
    line_height: u32,
    colorscheme: Option<&'t [RGBA; 16]>,
    pub(crate) term: Option<Terminal<'t>>,
    pub(crate) view_state: Option<ViewState>,
}

impl<'t> Display<'t> {
    pub fn new(text_width: u32, line_height: u32, colorscheme: &'t [RGBA; 16]) -> Self {
        Self {
            colorscheme: Some(colorscheme),
            term: None,
            view_state: None,
            text_width,
            line_height,
        }
    }
}

impl ApplicationHandler for Display<'_> {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let window = event_loop
            .create_window(Window::default_attributes())
            .unwrap();

        let size = window.inner_size();
        if self.view_state.is_none() {
            self.view_state = Some(ViewState::new(window.into()));
            self.term = Some(Terminal::new(
                size,
                self.text_width,
                self.line_height,
                self.colorscheme.unwrap(),
            ));
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        tracing::info!("Window event !");
    }
}

#[derive(Debug)]
pub struct ViewState {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: PhysicalSize<u32>,
    window: Arc<Window>,
}

impl ViewState {
    pub fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            flags: wgpu::InstanceFlags::from_build_config(),
            ..Default::default()
        });

        let surface = instance.create_surface(Arc::clone(&window)).unwrap();

        let rt = Runtime::new().unwrap();

        let (adapter, (device, queue)) = rt.block_on(async {
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    force_fallback_adapter: false,
                    compatible_surface: Some(&surface),
                })
                .await
                .unwrap();

            let (device, queue) = adapter
                .request_device(
                    &wgpu::DeviceDescriptor {
                        label: None,
                        required_features: wgpu::Features::VERTEX_WRITABLE_STORAGE,
                        required_limits: wgpu::Limits::default(),
                        memory_hints: wgpu::MemoryHints::MemoryUsage,
                    },
                    None,
                )
                .await
                .unwrap();
            (adapter, (device, queue))
        });

        let caps = surface.get_capabilities(&adapter);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            format: caps
                .formats
                .iter()
                .find(|f| f.is_srgb())
                .copied()
                .unwrap_or(caps.formats[0]),
            width: size.width,
            height: size.height,
            present_mode: caps.present_modes[0],
            desired_maximum_frame_latency: 8,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
        };
        Self {
            surface,
            device,
            queue,
            config,
            size,
            window,
        }
    }
}
