use self::display::Display;
use self::renderer::Renderer;
use self::text::GlyphVertex;
use rusttype::Scale;
use std::io::{ErrorKind, Read};
use std::ops::Range;
use std::sync::Arc;
use term::data::cursor::Cursor;
use term::data::grids::Grid;
use term::data::{Attribute, Cell, Color, Column, Line, RGBA};
use term::pty::PTY;
use tokio::runtime::Runtime;
use vte::VTEParser;
use wgpu::include_wgsl;
use wgpu::util::{BufferInitDescriptor, DeviceExt};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::window::Window;
pub mod display;
pub mod renderer;
pub mod text;

pub struct App<'config> {
    colorscheme: &'config [RGBA; 16],
    scale: Scale,
    display: Option<Display<'config>>,
    pty: PTY,
    parser: VTEParser,

    renderer: Option<Renderer<'config>>,
    state: Option<DisplayState>,
}

pub struct DisplayState {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipe_line: wgpu::RenderPipeline,
    size: PhysicalSize<u32>,
    config: wgpu::SurfaceConfiguration,
    buffer: wgpu::Buffer,
    num_vertices: usize,
}

impl DisplayState {
    pub fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        let surface = instance.create_surface(window.clone()).unwrap();

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
                        required_features: wgpu::Features::empty(),
                        required_limits: wgpu::Limits::default(),
                        memory_hints: wgpu::MemoryHints::Performance,
                    },
                    None,
                )
                .await
                .unwrap();

            (adapter, (device, queue))
        });

        let surface_caps = surface.get_capabilities(&adapter);
        // Shader code in this tutorial assumes an sRGB surface texture. Using a different
        // one will result in all the colors coming out darker. If you want to support non
        // sRGB surfaces, you'll need to account for that when drawing to the frame.
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &config);

        let main_shader = include_wgsl!("./shader.wgsl");
        let fragment_shader = include_wgsl!("./shader.wgsl");

        let vs = device.create_shader_module(main_shader);
        let fs = device.create_shader_module(fragment_shader);

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[],
                push_constant_ranges: &[],
            });

        let pipe_line = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("font render pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vs,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[GlyphVertex::desc()],
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(wgpu::FragmentState {
                module: &fs,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent::REPLACE,
                        alpha: wgpu::BlendComponent::REPLACE,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                polygon_mode: wgpu::PolygonMode::Fill,
                cull_mode: Some(wgpu::Face::Back),
                unclipped_depth: false,
                conservative: false,
            },
            multiview: None,
            cache: None,
        });

        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("screen buffer"),
            size: 100_000_000,
            usage: wgpu::BufferUsages::VERTEX,
            mapped_at_creation: false,
        });

        Self {
            window,
            surface,
            device,
            queue,
            pipe_line,
            size,
            config,
            buffer,
            num_vertices: 0,
        }
    }

    pub fn rerender_state(&mut self, glyph: usize, buffer: Vec<GlyphVertex>) {
        self.num_vertices = glyph;
        self.buffer = self.device.create_buffer_init(&BufferInitDescriptor {
            label: Some("screen buffer"),
            contents: bytemuck::cast_slice(&buffer),
            usage: wgpu::BufferUsages::VERTEX,
        });
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        println!("rendering ");
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rendering encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            render_pass.set_pipeline(&self.pipe_line);
            render_pass.set_vertex_buffer(0, self.buffer.slice(..));
            render_pass.draw(0..self.num_vertices as u32, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }
}

impl<'config> App<'config> {
    pub fn new(colorscheme: &'config [RGBA; 16], scale: Scale, pty: PTY) -> Self {
        Self {
            colorscheme,
            display: None,
            renderer: None,
            scale,
            state: None,
            pty,
            parser: VTEParser::new(),
        }
    }

    pub fn update(&mut self) {
        let mut curr = 0;

        let reader = self.pty.io();

        let mut buff = vec![0; 2048];
        loop {
            match reader.read(&mut buff[curr..]) {
                Ok(n) => {
                    if n == 0 {
                        return;
                    } else {
                        curr += n;
                        if curr > 100 {
                            break;
                        }
                    }
                }
                Err(_e) => break,
            }
        }

        self.parser
            .parse(&buff[..curr], self.display.as_mut().unwrap());

        let render = self.renderer.as_ref().unwrap();
        let buffer = render.prepare_render(self.display.as_ref().unwrap().grid_iter(Line(0)));
        self.state
            .as_mut()
            .unwrap()
            .rerender_state(buffer.len(), buffer);
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        self.state.as_mut().unwrap().resize(new_size);
    }
}

impl ApplicationHandler for App<'_> {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.display.is_none() {
            let window = Arc::new(
                event_loop
                    .create_window(Window::default_attributes())
                    .unwrap(),
            );
            let size = window.inner_size();
            self.state = Some(DisplayState::new(Arc::clone(&window)));

            self.display = Some(Display::new(
                size.width,
                size.height,
                self.scale,
                self.colorscheme,
            ));

            self.renderer = Some(Renderer::new(
                size.width,
                size.height,
                self.scale,
                self.colorscheme,
            ));
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        if window_id != self.state.as_ref().unwrap().window.id() {
            std::process::exit(1)
        }

        self.state.as_mut().unwrap().window.request_redraw();
        self.update();

        let state = self.state.as_mut().unwrap();
        match event {
            winit::event::WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            winit::event::WindowEvent::Destroyed => {
                event_loop.exit();
            }
            winit::event::WindowEvent::Resized(new_size) => self.resize(new_size),
            winit::event::WindowEvent::RedrawRequested => match state.render() {
                Ok(_) => {
                    println!("rendered");
                }
                Err(e) => {
                    println!("error: {e}");
                    match e {
                        wgpu::SurfaceError::Timeout => {}
                        wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost => {
                            state.resize(state.size)
                        }
                        wgpu::SurfaceError::OutOfMemory => {
                            event_loop.exit();
                        }
                    }
                }
            },
            _ => {}
        }
    }
}

#[derive(Debug)]
pub struct Terminal<'config> {
    scheme: &'config [RGBA; 16],

    fg: Color,
    bg: Color,
    attr: Attribute,

    dark_mode: bool,
    pub data: Grid<Cell>,
    pub write_stack: Vec<Cell>,
}

impl<'config> Terminal<'config> {
    pub fn new(max_row: usize, max_col: usize, colorscheme: &'config [RGBA; 16]) -> Self {
        Self {
            scheme: colorscheme,
            fg: Color::IndexBase(7),
            bg: Color::IndexBase(0),
            attr: Attribute::default(),
            dark_mode: false,
            data: Grid::new(max_col, max_row),
            write_stack: Vec::with_capacity(25),
        }
    }

    pub fn resize(&mut self, max_row: usize, max_col: usize) {
        self.data.resize(max_col, max_row, |_| true);
    }

    pub fn input(&mut self, cursor: &mut Cursor, data: Vec<Cell>) {
        self.data.input_insert(data, cursor, |_| true);
    }

    pub fn update(&mut self, cursor: &mut Cursor) {
        self.data
            .input_insert(std::mem::take(&mut self.write_stack), cursor, |_| true);
    }

    pub fn reset_graphic(&mut self) {
        self.fg = Color::IndexBase(7);
        self.bg = Color::IndexBase(0);
        self.attr = Attribute::default();
    }

    fn set_attr(&mut self, val: i64) {}

    pub fn rendition(&mut self, rendition: Vec<i64>) {
        if rendition.len() <= 2 {
            for val in rendition {
                match &val {
                    0 => self.reset_graphic(),
                    1..=27 => self.set_attr(val),
                    30..=37 => {
                        if self.dark_mode {
                            self.fg = Color::IndexBase((val - 30) as usize)
                        } else {
                            self.fg = Color::IndexBase((val - 30 + 8) as usize)
                        }
                    }
                    38 => self.fg = Color::IndexBase(7),
                    40..=47 => {
                        if self.dark_mode {
                            self.bg = Color::IndexBase((val - 30) as usize)
                        } else {
                            self.bg = Color::IndexBase((val - 30 + 8) as usize)
                        }
                    }
                    49 => self.bg = Color::IndexBase(0),
                    _ => {}
                }
            }
            return;
        }

        if rendition.len() > 2 {
            match rendition.as_slice() {
                [pre @ .., 38, 5, index] => {
                    self.rendition(pre.to_vec());
                    self.fg = Color::Index256(*index as usize);
                }
                [pre @ .., 48, 5, index] => {
                    self.rendition(pre.to_vec());
                    self.fg = Color::Index256(*index as usize);
                }
                [38, 2, rgb @ ..] => {
                    self.fg = Color::Rgba(RGBA {
                        r: rgb
                            .first()
                            .map_or_else(|| 0, |r| (*r).try_into().unwrap_or(0)),
                        g: rgb
                            .get(1)
                            .map_or_else(|| 0, |g| (*g).try_into().unwrap_or(0)),
                        b: rgb
                            .get(2)
                            .map_or_else(|| 0, |b| (*b).try_into().unwrap_or(0)),
                        a: rgb
                            .get(3)
                            .map_or_else(|| 255, |a| (*a).try_into().unwrap_or(255)),
                    });
                }
                [48, 2, rgb @ ..] => {
                    self.bg = Color::Rgba(RGBA {
                        r: rgb
                            .first()
                            .map_or_else(|| 0, |r| (*r).try_into().unwrap_or(0)),
                        g: rgb
                            .get(1)
                            .map_or_else(|| 0, |g| (*g).try_into().unwrap_or(0)),
                        b: rgb
                            .get(2)
                            .map_or_else(|| 0, |b| (*b).try_into().unwrap_or(0)),
                        a: rgb
                            .get(3)
                            .map_or_else(|| 255, |a| (*a).try_into().unwrap_or(255)),
                    });
                }
                _ => {}
            }
        }
    }

    pub fn add_new_cell(&mut self, c: char) {
        self.write_stack.push(Cell {
            c,
            fg: self.fg,
            bg: self.bg,
            attr: self.attr.clone(),
            sixel_data: None,
            erasable: true,
            dirty: false,
        });
    }

    pub fn erase_line_range_unchecked(
        &mut self,
        line: Line,
        range: Range<usize>,
        with_filter: impl Fn(&Cell) -> bool,
    ) {
        if self.data.len() < line.0 {
            return;
        }
        let data = &mut self.data[line];

        for i in range {
            if i > data.len() - 1 {
                return;
            }
            if !with_filter(&data[Column(i)]) {
                continue;
            }
            data[Column(i)].c = ' ';
            data[Column(i)].dirty = true;
            data[Column(i)].bg = Color::IndexBase(0);
            data[Column(i)].fg = Color::IndexBase(7);
            data[Column(i)].attr = Attribute::default();
        }
    }

    /// Erase lines in range
    pub fn erase_range_unchecked(
        &mut self,
        range: Range<usize>,
        mut with_filter: impl FnMut(&&mut Cell) -> bool,
    ) {
        for i in range {
            (&mut self.data[Line(i)])
                .into_iter()
                .take_while(&mut with_filter)
                .for_each(|cell| {
                    cell.c = ' ';
                    cell.dirty = true;
                    cell.bg = Color::IndexBase(0);
                    cell.fg = Color::IndexBase(7);
                    cell.attr = Attribute::default();
                });
        }
    }
}
