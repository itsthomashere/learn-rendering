use harfbuzz_rs::{Feature, Tag, UnicodeBuffer};
use image::{ImageBuffer, Rgba, RgbaImage};
use learn_rendering::renderer::LineBuffer;
use learn_rendering::Color;
use rusttype::{point, GlyphId, Scale};
use std::time::{Duration, Instant};

fn main() {
    let hb_font = harfbuzz_rs::rusttype::create_harfbuzz_rusttype_font(
        *include_bytes!("/home/dacbui308/.local/share/fonts/MapleMono-NF-Italic.ttf"),
        0,
    )
    .unwrap();

    let rt_font = rusttype::Font::try_from_bytes(include_bytes!(
        "/home/dacbui308/.local/share/fonts/MapleMono-NF-Italic.ttf"
    ))
    .unwrap();
    let scale = Scale::uniform(32.0);
    let max_x = 1280;
    let max_y = 960;
    let mut line_buffer = LineBuffer::new(scale, 0, 0, max_x, max_y);

    let color_1 = Color {
        r: 0,
        g: 204,
        b: 120,
        a: 255,
    };

    let color_2 = Color {
        r: 51,
        g: 51,
        b: 255,
        a: 255,
    };

    let color_3 = Color {
        r: 255,
        g: 0,
        b: 127,
        a: 255,
    };

    let instant = Instant::now();
    line_buffer.append_text("This is the first line.\n", color_1.clone());
    line_buffer.append_text("This is the second line\n", color_2.clone());
    line_buffer.append_text("What.\nThis is the third line\n", color_3.clone());
    line_buffer.insert_at(
        1,
        3,
        " More line 12345678909876543212345678909876543211234567890123456",
        color_1.clone(),
    );
    line_buffer.append_text(
        "This is a buch of icons: 󰣇\n",
        Color {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        },
    );
    line_buffer.append_text("More text", color_1);
    println!("append text time: {}", instant.elapsed().as_micros());

    let background_color = Rgba([0, 0, 0, 255]); // Black background

    let mut image: RgbaImage = ImageBuffer::from_fn(max_x, max_y, |_, _| background_color);

    line_buffer.render_all(&hb_font, &rt_font, |x, y, v, color| {
        let pixel = image.get_pixel_mut(x as u32, y as u32);
        let fg = Rgba([color.r, color.g, color.b, (v * color.a as f32) as u8]);
        *pixel = blend_colors(*pixel, fg, v);
    });
    image.save("output.png").expect("could not write image");
}

fn blend_colors(bg: Rgba<u8>, fg: Rgba<u8>, intensity: f32) -> Rgba<u8> {
    let alpha = intensity; // Use glyph intensity as alpha
    let inv_alpha = 1.0 - alpha;

    Rgba([
        (fg[0] as f32 * alpha + bg[0] as f32 * inv_alpha) as u8,
        (fg[1] as f32 * alpha + bg[1] as f32 * inv_alpha) as u8,
        (fg[2] as f32 * alpha + bg[2] as f32 * inv_alpha) as u8,
        255,
    ])
}

//     let hb_font = harfbuzz_rs::rusttype::create_harfbuzz_rusttype_font(
//         *include_bytes!("/home/dacbui308/.local/share/fonts/MapleMono-NF-Italic.ttf"),
//         0,
//     )
//     .unwrap();
//
//     let rt_font = rusttype::Font::try_from_bytes(include_bytes!(
//         "/home/dacbui308/.local/share/fonts/MapleMono-NF-Italic.ttf"
//     ))
//     .unwrap();
//
//     let mut renderer = FontRenderer::new(hb_font, rt_font, 800, 200, 32.0);
//     renderer.add_string("󰣇t.This shit is gonna go out of the image boundary󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇󰣇.Lol Please just le me die,hello");
//
//     let mut image = GrayImage::new(800, 200);
//
//     renderer.render(|x, y, v| {
//         let pixel = image.get_pixel_mut(x as u32, y as u32);
//         *pixel = Luma([(v * 255.0) as u8]);
//     });
//     image.save("output.png").expect("could not write image");
// }

pub struct FontRenderer {
    hb_font: harfbuzz_rs::Owned<harfbuzz_rs::Font<'static>>,
    rt_font: rusttype::Font<'static>,
    max_width: u32,
    max_height: u32,
    scale: Scale,

    current_text: String,
}

// TODO: HANDLE BITMAP RENDERING
// TODO: HANDLE \r, \n, \t
// TODO: HANDLE TEXTURE
impl FontRenderer {
    pub fn new(
        hb_font: harfbuzz_rs::Owned<harfbuzz_rs::Font<'static>>,
        rt_font: rusttype::Font<'static>,
        x: u32,
        y: u32,
        size: f32,
    ) -> Self {
        Self {
            hb_font,
            rt_font,
            max_width: x,
            max_height: y,
            current_text: String::default(),
            scale: Scale::uniform(size),
        }
    }

    pub fn add_string(&mut self, data: impl AsRef<str>) {
        self.current_text.push_str(data.as_ref());
    }

    pub fn render<F>(&mut self, mut f: F)
    where
        F: FnMut(i32, i32, f32),
    {
        let buffer = UnicodeBuffer::new()
            .add_str(self.current_text.as_str())
            .guess_segment_properties();

        let glyph_buffer = harfbuzz_rs::shape(
            &self.hb_font,
            buffer,
            &[
                Feature::new(Tag::new('l', 'i', 'g', 'a'), 1, 0..),
                Feature::new(Tag::new('c', 'a', 'l', 't'), 1, 0..),
            ],
        );

        let baseline_y = self.rt_font.v_metrics(self.scale).ascent.ceil();

        let positions = glyph_buffer.get_glyph_positions();
        let infos = glyph_buffer.get_glyph_infos();
        let max_col = self.max_width / (self.scale.x.round() / 2.0) as u32;

        let mut glyphs_with_infos = positions.iter().zip(infos).peekable();
        let mut cur_col = 0; // current column will only increase by 1, but
        let mut cur_row = 0;

        while let Some((position, info)) = glyphs_with_infos.next() {
            let scale_factor = match glyphs_with_infos.peek() {
                Some((_, next_info)) => next_info.cluster - info.cluster,
                None => 1,
            };
            let x_offset = position.x_offset as f32 / 64.0;
            let y_offset = position.y_offset as f32 / 64.0;
            let glyph_id = GlyphId(info.codepoint as u16);

            if cur_col >= max_col {
                cur_col = 0;
                cur_row += 1;
            }

            let x = cur_col as f32 * self.scale.x / 2.0 + x_offset;

            let y = cur_row as f32 * self.scale.y + y_offset + baseline_y;
            let scale = match scale_factor > 1 {
                true => Scale {
                    x: self.scale.x / 1.5,
                    y: self.scale.y / 1.5,
                },
                false => self.scale,
            };

            let glyph = self
                .rt_font
                .glyph(glyph_id)
                .scaled(scale)
                .positioned(point(x, y));

            if let Some(round_box) = glyph.pixel_bounding_box() {
                glyph.draw(|x, y, v| {
                    let x = x as i32 + round_box.min.x;
                    let y = y as i32 + round_box.min.y;

                    if x >= 0 && x < self.max_width as i32 && y >= 0 && y < self.max_height as i32 {
                        f(x, y, v)
                    }
                });
            }

            cur_col += 1;
        }
    }
}

// #[derive(Default)]
// pub struct Screen<'a> {
//     state: Option<State<'a>>,
// }
//
// impl ApplicationHandler for Screen<'_> {
//     fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
//         if self.state.is_none() {
//             self.state = Some(
//                 State::new(Arc::new(
//                     event_loop
//                         .create_window(Window::default_attributes())
//                         .unwrap(),
//                 ))
//                 .unwrap(),
//             );
//         }
//     }
//
//     fn window_event(
//         &mut self,
//         event_loop: &winit::event_loop::ActiveEventLoop,
//         window_id: winit::window::WindowId,
//         event: WindowEvent,
//     ) {
//         let state = self.state.as_mut().expect("is initialzed");
//         if window_id != state.window.id() {
//             log::warn!("wrong window id");
//             event_loop.exit();
//         }
//
//         match event {
//             WindowEvent::Destroyed | WindowEvent::CloseRequested => {
//                 log::info!("exiting");
//                 event_loop.exit();
//             }
//
//             WindowEvent::KeyboardInput { event, .. } => {
//                 if event.physical_key == PhysicalKey::Code(KeyCode::Escape) {
//                     log::info!("exiting with escape key");
//                     event_loop.exit();
//                 }
//
//                 log::info!("key: {:?}", event.physical_key);
//             }
//
//             WindowEvent::Resized(new_size) => {
//                 state.resize(new_size);
//             }
//
//             WindowEvent::RedrawRequested => match state.render() {
//                 Ok(_) => {}
//                 Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
//                     state.resize(state.size);
//                 }
//                 Err(wgpu::SurfaceError::Timeout) => {
//                     log::warn!("timeout")
//                 }
//                 Err(wgpu::SurfaceError::OutOfMemory) => {
//                     log::error!("out of memory");
//                     event_loop.exit();
//                 }
//             },
//             _ => {
//                 log::info!("unhandled event")
//             }
//         }
//     }
// }
//
// pub struct State<'a> {
//     config: wgpu::SurfaceConfiguration,
//     surface: wgpu::Surface<'a>,
//     device: wgpu::Device,
//     queue: wgpu::Queue,
//     size: PhysicalSize<u32>,
//     render: wgpu::RenderPipeline,
//     vertex_buffer: wgpu::Buffer,
//     num_vertices: u32,
//     window: Arc<Window>,
// }
//
// impl State<'_> {
//     pub fn new(window: Arc<Window>) -> Result<Self, Box<dyn Error>> {
//         let size = window.inner_size();
//         let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
//             backends: wgpu::Backends::PRIMARY,
//             ..Default::default()
//         });
//
//         let surface = instance.create_surface(window.clone())?;
//
//         let rt = Runtime::new()?;
//
//         let adapter = rt.block_on(async {
//             instance
//                 .request_adapter(&wgpu::RequestAdapterOptions {
//                     power_preference: wgpu::PowerPreference::HighPerformance,
//                     force_fallback_adapter: false,
//                     compatible_surface: Default::default(),
//                 })
//                 .await
//                 .unwrap()
//         });
//
//         let (device, queue) = rt.block_on(async {
//             adapter
//                 .request_device(
//                     &wgpu::DeviceDescriptor {
//                         label: Some("device"),
//                         required_features: wgpu::Features::empty(),
//                         required_limits: wgpu::Limits::default(),
//                         memory_hints: wgpu::MemoryHints::Performance,
//                     },
//                     None,
//                 )
//                 .await
//                 .unwrap()
//         });
//
//         let capabilities = surface.get_capabilities(&adapter);
//
//         let format = capabilities
//             .formats
//             .iter()
//             .find(|format| format.is_srgb())
//             .copied()
//             .unwrap_or(capabilities.formats[0]);
//
//         let config = wgpu::SurfaceConfiguration {
//             usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
//             format,
//             width: size.width,
//             height: size.height,
//             present_mode: capabilities.present_modes[0],
//             desired_maximum_frame_latency: 2,
//             alpha_mode: capabilities.alpha_modes[0],
//             view_formats: vec![],
//         };
//
//         let shader = device.create_shader_module(include_wgsl!("shader.wgsl"));
//         let render_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
//             label: Some("pipeline layout"),
//             bind_group_layouts: &[],
//             push_constant_ranges: &[],
//         });
//         let render = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
//             label: Some("render pipeline"),
//             layout: Some(&render_layout),
//             vertex: wgpu::VertexState {
//                 module: &shader,
//                 entry_point: "vs_main".into(),
//                 compilation_options: wgpu::PipelineCompilationOptions::default(),
//                 buffers: &[Vertex::desc()],
//             },
//             primitive: wgpu::PrimitiveState {
//                 topology: wgpu::PrimitiveTopology::TriangleList,
//                 strip_index_format: None,
//                 front_face: wgpu::FrontFace::Ccw,
//                 cull_mode: Some(wgpu::Face::Back),
//                 unclipped_depth: false,
//                 polygon_mode: wgpu::PolygonMode::Fill,
//                 conservative: false,
//             },
//             fragment: Some(wgpu::FragmentState {
//                 module: &shader,
//                 entry_point: "fs_main".into(),
//                 compilation_options: wgpu::PipelineCompilationOptions::default(),
//                 targets: &[Some(wgpu::ColorTargetState {
//                     format: config.format,
//                     blend: Some(wgpu::BlendState::REPLACE),
//                     write_mask: wgpu::ColorWrites::ALL,
//                 })],
//             }),
//             depth_stencil: None,
//             multisample: wgpu::MultisampleState {
//                 count: 1,
//                 mask: !0,
//                 alpha_to_coverage_enabled: false,
//             },
//             multiview: None,
//             cache: None,
//         });
//
//         let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
//             label: Some("vertex buffer"),
//             contents: bytemuck::cast_slice(VERTICES),
//             usage: wgpu::BufferUsages::VERTEX,
//         });
//
//         let num_vertices = VERTICES.len() as u32;
//
//         Ok(Self {
//             config,
//             surface,
//             device,
//             queue,
//             size,
//             window,
//             vertex_buffer,
//             render,
//             num_vertices,
//         })
//     }
//
//     pub fn input(&mut self, _event: &WindowEvent) -> bool {
//         false
//     }
//
//     pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
//         if new_size.width > 0 && new_size.height > 0 {
//             self.size = new_size;
//             self.config.width = new_size.width;
//             self.config.height = new_size.height;
//
//             self.surface.configure(&self.device, &self.config);
//         }
//     }
//
//     pub fn update(&mut self) {}
//
//     pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
//         let output = self.surface.get_current_texture()?;
//
//         let view = output
//             .texture
//             .create_view(&wgpu::TextureViewDescriptor::default());
//
//         let mut encoder = self
//             .device
//             .create_command_encoder(&wgpu::CommandEncoderDescriptor {
//                 label: Some("command encoder"),
//             });
//
//         {
//             let mut _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
//                 label: Some("Render Pass"),
//                 color_attachments: &[Some(wgpu::RenderPassColorAttachment {
//                     view: &view,
//                     resolve_target: None,
//                     ops: wgpu::Operations {
//                         load: wgpu::LoadOp::Clear(wgpu::Color {
//                             r: 0.0,
//                             g: 0.0,
//                             b: 0.0,
//                             a: 1.0,
//                         }),
//                         store: wgpu::StoreOp::Store,
//                     },
//                 })],
//                 depth_stencil_attachment: None,
//                 occlusion_query_set: None,
//                 timestamp_writes: None,
//             });
//             _render_pass.set_pipeline(&self.render);
//             _render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
//             _render_pass.draw(0..self.num_vertices, 0..1);
//         }
//
//         // submit will accept anything that implements IntoIter
//         self.queue.submit(std::iter::once(encoder.finish()));
//         output.present();
//
//         Ok(())
//     }
// }
//
// #[repr(C)]
// #[derive(Debug, Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
// pub struct Vertex {
//     position: [f32; 3],
//     color: [f32; 3],
// }
// const VERTICES: &[Vertex] = &[
//     Vertex {
//         position: [0.0, 0.5, 0.0],
//         color: [1.0, 0.0, 0.0],
//     },
//     Vertex {
//         position: [-0.5, -0.5, 0.0],
//         color: [0.0, 1.0, 0.0],
//     },
//     Vertex {
//         position: [0.5, -0.5, 0.0],
//         color: [0.0, 0.0, 1.0],
//     },
// ];
// impl Vertex {
//     fn desc() -> wgpu::VertexBufferLayout<'static> {
//         wgpu::VertexBufferLayout {
//             array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
//             step_mode: wgpu::VertexStepMode::Vertex,
//             attributes: &[
//                 wgpu::VertexAttribute {
//                     offset: 0,
//                     shader_location: 0,
//                     format: wgpu::VertexFormat::Float32x3,
//                 },
//                 wgpu::VertexAttribute {
//                     offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
//                     shader_location: 1,
//                     format: wgpu::VertexFormat::Float32x3,
//                 },
//             ],
//         }
//     }
// }
