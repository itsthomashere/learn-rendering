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
use wgpu::util::{BufferInitDescriptor, DeviceExt, RenderEncoder};
use wgpu::{include_wgsl, Origin2d, Origin3d, TextureAspect};
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
    shader_uniform_bind_group_layout: wgpu::BindGroupLayout,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    texture_nearest_sampler: wgpu::Sampler,
    texture_linear_sampler: wgpu::Sampler,
    buffer: wgpu::Buffer,
    num_vertices: usize,
}

impl DisplayState {
    pub fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
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
                        required_features: wgpu::Features::FLOAT32_FILTERABLE,
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
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &config);

        let main_shader = include_wgsl!("./alt_shader.wgsl");
        let vs = device.create_shader_module(main_shader);

        let shader_uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
                label: Some("ShaderUniform bind group layout"),
            });

        let texture_nearest_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let texture_linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
                label: Some("texture bind group layout"),
            });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[
                    // &shader_uniform_bind_group_layout,
                    // &texture_bind_group_layout,
                    &texture_bind_group_layout,
                ],
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
                module: &vs,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),

            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                polygon_mode: wgpu::PolygonMode::Fill,
                cull_mode: None,
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
            shader_uniform_bind_group_layout,
            texture_bind_group_layout,
            texture_nearest_sampler,
            texture_linear_sampler,
        }
    }

    pub fn rerender_state(&mut self, glyph: usize, buffer: Vec<GlyphVertex>) {
        self.num_vertices = glyph;
        self.buffer = self.device.create_buffer_init(&BufferInitDescriptor {
            label: Some("screen buffer"),
            contents: bytemuck::cast_slice(&buffer),
            usage: wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::UNIFORM
                | wgpu::BufferUsages::VERTEX,
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
            // let uniform_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            //     label: Some("uniform group"),
            //     layout: &self.shader_uniform_bind_group_layout,
            //     entries: &[wgpu::BindGroupEntry {
            //         binding: 0,
            //         resource: self.buffer.as_entire_binding(),
            //     }],
            // });

            let bytes_per_pixel = 16; // RGBA32Float
            let row_size = self.size.width * bytes_per_pixel;
            let padded_row_size = ((row_size + 255) / 256) * 256;
            let raw_data = vec![128u8; self.size.width as usize * self.size.height as usize];

            let buffer_size = padded_row_size * self.size.height;
            let mut padded_data = vec![0u8; buffer_size as usize];

            for row in 0..self.size.height {
                let src_start = (row * row_size) as usize;
                let src_end = src_start + row_size as usize;

                let dst_start = (row * padded_row_size) as usize;
                let dst_end = dst_start + row_size as usize;

                let src_bytes = bytemuck::cast_slice(&raw_data[src_start..src_end]);
                padded_data[dst_start..dst_end].copy_from_slice(src_bytes);
            }

            let texture_size = wgpu::Extent3d {
                width: self.size.width,
                height: self.size.height,
                depth_or_array_layers: 1,
            };

            for row in 0..self.size.height {
                let src_start = (row * row_size) as usize;
                let src_end = src_start + row_size as usize;

                let dst_start = (row * padded_row_size) as usize;
                let dst_end = dst_start + row_size as usize;

                padded_data[dst_start..dst_end].copy_from_slice(&raw_data[src_start..src_end]);
            }
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("texture"),
                size: texture_size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });

            self.queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: &texture,
                    mip_level: 0,
                    origin: Origin3d::ZERO,
                    aspect: TextureAspect::All,
                },
                &padded_data,
                wgpu::ImageDataLayout {
                    offset: 1,
                    bytes_per_row: Some(self.size.width),
                    rows_per_image: Some(self.size.width * self.size.height),
                },
                texture_size,
            );

            // self.queue.write_texture(
            //     wgpu::ImageCopyTexture {
            //         texture: &texture,
            //         mip_level: 0,
            //         origin: wgpu::Origin3d::ZERO,
            //         aspect: wgpu::TextureAspect::All,
            //     },
            //     &vec![128u8; (self.size.width * self.size.height) as usize],
            //     wgpu::ImageDataLayout {
            //         offset: 0,
            //         bytes_per_row: Some(self.size.width),
            //         rows_per_image: None,
            //     },
            //     texture_size,
            // );

            let texture_view = texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("texture view for atlas"),
                format: Some(wgpu::TextureFormat::Rgba32Float),
                dimension: Some(wgpu::TextureViewDimension::D2),
                ..Default::default()
            });

            let atlas_linear = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("al bind group"),
                layout: &self.texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.texture_linear_sampler),
                    },
                ],
            });

            // let atlas_nearest = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            //     label: Some("an bind group"),
            //     layout: &self.texture_bind_group_layout,
            //     entries: &[
            //         wgpu::BindGroupEntry {
            //             binding: 0,
            //             resource: wgpu::BindingResource::TextureView(&texture_view),
            //         },
            //         wgpu::BindGroupEntry {
            //             binding: 1,
            //             resource: wgpu::BindingResource::Sampler(&self.texture_nearest_sampler),
            //         },
            //     ],
            // });
            //
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 1.0,
                            g: 1.0,
                            b: 1.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            let buffer = vec![
                GlyphVertex {
                    position: [-1.0, 0.96183205],
                    tex_coords: [0.001953125, 0.1171875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-1.0, 0.99236643],
                    tex_coords: [0.001953125, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.97280335, 0.99236643],
                    tex_coords: [0.02734375, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.97280335, 0.99236643],
                    tex_coords: [0.02734375, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.97280335, 0.96183205],
                    tex_coords: [0.02734375, 0.1171875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-1.0, 0.96183205],
                    tex_coords: [0.001953125, 0.1171875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.96443516, 0.95229006],
                    tex_coords: [0.001953125, 0.04296875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.96443516, 0.99236643],
                    tex_coords: [0.001953125, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.9539749, 0.99236643],
                    tex_coords: [0.01171875, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.9539749, 0.99236643],
                    tex_coords: [0.01171875, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.9539749, 0.95229006],
                    tex_coords: [0.01171875, 0.04296875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.96443516, 0.95229006],
                    tex_coords: [0.001953125, 0.04296875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.9497908, 0.96183205],
                    tex_coords: [0.25, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.9497908, 0.98664117],
                    tex_coords: [0.25, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.9309623, 0.98664117],
                    tex_coords: [0.26757813, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.9309623, 0.98664117],
                    tex_coords: [0.26757813, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.9309623, 0.96183205],
                    tex_coords: [0.26757813, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.9497908, 0.96183205],
                    tex_coords: [0.25, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.92677826, 0.95229006],
                    tex_coords: [0.43359375, 0.08203125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.92677826, 0.98664117],
                    tex_coords: [0.43359375, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.90585774, 0.98664117],
                    tex_coords: [0.453125, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.90585774, 0.98664117],
                    tex_coords: [0.453125, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.90585774, 0.95229006],
                    tex_coords: [0.453125, 0.08203125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.92677826, 0.95229006],
                    tex_coords: [0.43359375, 0.08203125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.9037657, 0.96183205],
                    tex_coords: [0.5859375, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.9037657, 0.98664117],
                    tex_coords: [0.5859375, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.88284516, 0.98664117],
                    tex_coords: [0.60546875, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.88284516, 0.98664117],
                    tex_coords: [0.60546875, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.88284516, 0.96183205],
                    tex_coords: [0.60546875, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.9037657, 0.96183205],
                    tex_coords: [0.5859375, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8786611, 0.96183205],
                    tex_coords: [0.7167969, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8786611, 0.98664117],
                    tex_coords: [0.7167969, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8577406, 0.98664117],
                    tex_coords: [0.7363281, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8577406, 0.98664117],
                    tex_coords: [0.7363281, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8577406, 0.96183205],
                    tex_coords: [0.7363281, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8786611, 0.96183205],
                    tex_coords: [0.7167969, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8556485, 0.96183205],
                    tex_coords: [0.31640625, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8556485, 0.98664117],
                    tex_coords: [0.31640625, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.834728, 0.98664117],
                    tex_coords: [0.3359375, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.834728, 0.98664117],
                    tex_coords: [0.3359375, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.834728, 0.96183205],
                    tex_coords: [0.3359375, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8556485, 0.96183205],
                    tex_coords: [0.31640625, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.832636, 0.96183205],
                    tex_coords: [0.609375, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.832636, 0.98664117],
                    tex_coords: [0.609375, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.81380755, 0.98664117],
                    tex_coords: [0.6269531, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.81380755, 0.98664117],
                    tex_coords: [0.6269531, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.81380755, 0.96183205],
                    tex_coords: [0.6269531, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.832636, 0.96183205],
                    tex_coords: [0.609375, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8117155, 0.96183205],
                    tex_coords: [0.22851563, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8117155, 0.98664117],
                    tex_coords: [0.22851563, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.79288703, 0.98664117],
                    tex_coords: [0.24609375, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.79288703, 0.98664117],
                    tex_coords: [0.24609375, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.79288703, 0.96183205],
                    tex_coords: [0.24609375, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8117155, 0.96183205],
                    tex_coords: [0.22851563, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.77615064, 0.95229006],
                    tex_coords: [0.115234375, 0.08203125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.77615064, 0.98664117],
                    tex_coords: [0.115234375, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.7552301, 0.98664117],
                    tex_coords: [0.13476563, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.7552301, 0.98664117],
                    tex_coords: [0.13476563, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.7552301, 0.95229006],
                    tex_coords: [0.13476563, 0.08203125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.77615064, 0.95229006],
                    tex_coords: [0.115234375, 0.08203125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.75313807, 0.96183205],
                    tex_coords: [0.33984375, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.75313807, 0.98664117],
                    tex_coords: [0.33984375, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.73221755, 0.98664117],
                    tex_coords: [0.359375, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.73221755, 0.98664117],
                    tex_coords: [0.359375, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.73221755, 0.96183205],
                    tex_coords: [0.359375, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.75313807, 0.96183205],
                    tex_coords: [0.33984375, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.7280335, 0.96183205],
                    tex_coords: [0.29492188, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.7280335, 0.98664117],
                    tex_coords: [0.29492188, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.70920503, 0.98664117],
                    tex_coords: [0.3125, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.70920503, 0.98664117],
                    tex_coords: [0.3125, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.70920503, 0.96183205],
                    tex_coords: [0.3125, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.7280335, 0.96183205],
                    tex_coords: [0.29492188, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.7050209, 0.96183205],
                    tex_coords: [0.42773438, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.7050209, 0.98664117],
                    tex_coords: [0.42773438, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.6715481, 0.98664117],
                    tex_coords: [0.45898438, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.6715481, 0.98664117],
                    tex_coords: [0.45898438, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.6715481, 0.96183205],
                    tex_coords: [0.45898438, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.7050209, 0.96183205],
                    tex_coords: [0.42773438, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.667364, 0.96183205],
                    tex_coords: [0.52734375, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.667364, 0.98664117],
                    tex_coords: [0.52734375, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.6589958, 0.98664117],
                    tex_coords: [0.53515625, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.6589958, 0.98664117],
                    tex_coords: [0.53515625, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.6589958, 0.96183205],
                    tex_coords: [0.53515625, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.667364, 0.96183205],
                    tex_coords: [0.52734375, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.99790794, 0.8683206],
                    tex_coords: [0.6699219, 0.0390625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.99790794, 0.9045801],
                    tex_coords: [0.6699219, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.95815897, 0.9045801],
                    tex_coords: [0.70703125, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.95815897, 0.9045801],
                    tex_coords: [0.70703125, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.95815897, 0.8683206],
                    tex_coords: [0.70703125, 0.0390625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.99790794, 0.8683206],
                    tex_coords: [0.6699219, 0.0390625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.95188284, 0.8683206],
                    tex_coords: [0.86328125, 0.037109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.95188284, 0.9026718],
                    tex_coords: [0.86328125, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.91422594, 0.9026718],
                    tex_coords: [0.8984375, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.91422594, 0.9026718],
                    tex_coords: [0.8984375, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.91422594, 0.8683206],
                    tex_coords: [0.8984375, 0.037109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.95188284, 0.8683206],
                    tex_coords: [0.86328125, 0.037109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.91422594, 0.86641216],
                    tex_coords: [0.09765625, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.91422594, 0.9045801],
                    tex_coords: [0.09765625, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.87447697, 0.9045801],
                    tex_coords: [0.13476563, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.87447697, 0.9045801],
                    tex_coords: [0.13476563, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.87447697, 0.86641216],
                    tex_coords: [0.13476563, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.91422594, 0.86641216],
                    tex_coords: [0.09765625, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8702929, 0.8721374],
                    tex_coords: [0.94921875, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8702929, 0.89694655],
                    tex_coords: [0.94921875, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.83054394, 0.89694655],
                    tex_coords: [0.9863281, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.83054394, 0.89694655],
                    tex_coords: [0.9863281, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.83054394, 0.8721374],
                    tex_coords: [0.9863281, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8702929, 0.8721374],
                    tex_coords: [0.94921875, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.82217574, 0.870229],
                    tex_coords: [0.0390625, 0.08203125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.82217574, 0.9045801],
                    tex_coords: [0.0390625, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.78870296, 0.9045801],
                    tex_coords: [0.0703125, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.78870296, 0.9045801],
                    tex_coords: [0.0703125, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.78870296, 0.870229],
                    tex_coords: [0.0703125, 0.08203125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.82217574, 0.870229],
                    tex_coords: [0.0390625, 0.08203125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.78451884, 0.86641216],
                    tex_coords: [0.015625, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.78451884, 0.9045801],
                    tex_coords: [0.015625, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.7447699, 0.9045801],
                    tex_coords: [0.052734375, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.7447699, 0.9045801],
                    tex_coords: [0.052734375, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.7447699, 0.86641216],
                    tex_coords: [0.052734375, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.78451884, 0.86641216],
                    tex_coords: [0.015625, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.7364017, 0.8683206],
                    tex_coords: [0.17773438, 0.08203125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.7364017, 0.9026718],
                    tex_coords: [0.17773438, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.70711297, 0.9026718],
                    tex_coords: [0.20507813, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.70711297, 0.9026718],
                    tex_coords: [0.20507813, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.70711297, 0.8683206],
                    tex_coords: [0.20507813, 0.08203125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.7364017, 0.8683206],
                    tex_coords: [0.17773438, 0.08203125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.6966527, 0.8683206],
                    tex_coords: [0.20898438, 0.08203125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.6966527, 0.9026718],
                    tex_coords: [0.20898438, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.65690374, 0.9026718],
                    tex_coords: [0.24609375, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.65690374, 0.9026718],
                    tex_coords: [0.24609375, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.65690374, 0.8683206],
                    tex_coords: [0.24609375, 0.08203125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.6966527, 0.8683206],
                    tex_coords: [0.20898438, 0.08203125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.6527197, 0.8683206],
                    tex_coords: [0.94140625, 0.037109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.6527197, 0.9026718],
                    tex_coords: [0.94140625, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.61924684, 0.9026718],
                    tex_coords: [0.97265625, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.61924684, 0.9026718],
                    tex_coords: [0.97265625, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.61924684, 0.8683206],
                    tex_coords: [0.97265625, 0.037109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.6527197, 0.8683206],
                    tex_coords: [0.94140625, 0.037109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.61087865, 0.8683206],
                    tex_coords: [0.001953125, 0.08203125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.61087865, 0.9026718],
                    tex_coords: [0.001953125, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.5753138, 0.9026718],
                    tex_coords: [0.03515625, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.5753138, 0.9026718],
                    tex_coords: [0.03515625, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.5753138, 0.8683206],
                    tex_coords: [0.03515625, 0.08203125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.61087865, 0.8683206],
                    tex_coords: [0.001953125, 0.08203125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.5711297, 0.86641216],
                    tex_coords: [0.13867188, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.5711297, 0.9045801],
                    tex_coords: [0.13867188, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.5292887, 0.9045801],
                    tex_coords: [0.17773438, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.5292887, 0.9045801],
                    tex_coords: [0.17773438, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.5292887, 0.86641216],
                    tex_coords: [0.17773438, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.5711297, 0.86641216],
                    tex_coords: [0.13867188, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.52510464, 0.8683206],
                    tex_coords: [0.7109375, 0.0390625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.52510464, 0.9045801],
                    tex_coords: [0.7109375, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.48535568, 0.9045801],
                    tex_coords: [0.7480469, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.48535568, 0.9045801],
                    tex_coords: [0.7480469, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.48535568, 0.8683206],
                    tex_coords: [0.7480469, 0.0390625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.52510464, 0.8683206],
                    tex_coords: [0.7109375, 0.0390625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.48535568, 0.870229],
                    tex_coords: [0.07421875, 0.08203125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.48535568, 0.9045801],
                    tex_coords: [0.07421875, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.4456067, 0.9045801],
                    tex_coords: [0.111328125, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.4456067, 0.9045801],
                    tex_coords: [0.111328125, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.4456067, 0.870229],
                    tex_coords: [0.111328125, 0.08203125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.48535568, 0.870229],
                    tex_coords: [0.07421875, 0.08203125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.44142258, 0.86641216],
                    tex_coords: [0.18164063, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.44142258, 0.9045801],
                    tex_coords: [0.18164063, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.40167361, 0.9045801],
                    tex_coords: [0.21875, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.40167361, 0.9045801],
                    tex_coords: [0.21875, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.40167361, 0.86641216],
                    tex_coords: [0.21875, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.44142258, 0.86641216],
                    tex_coords: [0.18164063, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.39748955, 0.8683206],
                    tex_coords: [0.22265625, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.39748955, 0.90648854],
                    tex_coords: [0.22265625, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.35774058, 0.90648854],
                    tex_coords: [0.25976563, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.35774058, 0.90648854],
                    tex_coords: [0.25976563, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.35774058, 0.8683206],
                    tex_coords: [0.25976563, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.39748955, 0.8683206],
                    tex_coords: [0.22265625, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.35564852, 0.86641216],
                    tex_coords: [0.26367188, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.35564852, 0.9045801],
                    tex_coords: [0.26367188, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.3179916, 0.9045801],
                    tex_coords: [0.29882813, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.3179916, 0.9045801],
                    tex_coords: [0.29882813, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.3179916, 0.86641216],
                    tex_coords: [0.29882813, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.35564852, 0.86641216],
                    tex_coords: [0.26367188, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.31171548, 0.8683206],
                    tex_coords: [0.28320313, 0.08203125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.31171548, 0.9026718],
                    tex_coords: [0.28320313, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.27405858, 0.9026718],
                    tex_coords: [0.31835938, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.27405858, 0.9026718],
                    tex_coords: [0.31835938, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.27405858, 0.8683206],
                    tex_coords: [0.31835938, 0.08203125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.31171548, 0.8683206],
                    tex_coords: [0.28320313, 0.08203125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.2656904, 0.8683206],
                    tex_coords: [0.40039063, 0.08203125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.2656904, 0.9026718],
                    tex_coords: [0.40039063, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.23430961, 0.9026718],
                    tex_coords: [0.4296875, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.23430961, 0.9026718],
                    tex_coords: [0.4296875, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.23430961, 0.8683206],
                    tex_coords: [0.4296875, 0.08203125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.2656904, 0.8683206],
                    tex_coords: [0.40039063, 0.08203125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.22594142, 0.8683206],
                    tex_coords: [0.90234375, 0.037109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.22594142, 0.9026718],
                    tex_coords: [0.90234375, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.18828452, 0.9026718],
                    tex_coords: [0.9375, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.18828452, 0.9026718],
                    tex_coords: [0.9375, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.18828452, 0.8683206],
                    tex_coords: [0.9375, 0.037109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.22594142, 0.8683206],
                    tex_coords: [0.90234375, 0.037109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.18410039, 0.86641216],
                    tex_coords: [0.30273438, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.18410039, 0.9045801],
                    tex_coords: [0.30273438, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.14644349, 0.9045801],
                    tex_coords: [0.33789063, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.14644349, 0.9045801],
                    tex_coords: [0.33789063, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.14644349, 0.86641216],
                    tex_coords: [0.33789063, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.18410039, 0.86641216],
                    tex_coords: [0.30273438, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.14225942, 0.86641216],
                    tex_coords: [0.34179688, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.14225942, 0.9045801],
                    tex_coords: [0.34179688, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.10251045, 0.9045801],
                    tex_coords: [0.37890625, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.10251045, 0.9045801],
                    tex_coords: [0.37890625, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.10251045, 0.86641216],
                    tex_coords: [0.37890625, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.14225942, 0.86641216],
                    tex_coords: [0.34179688, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.098326385, 0.8683206],
                    tex_coords: [0.7265625, 0.080078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.098326385, 0.9007634],
                    tex_coords: [0.7265625, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.06066948, 0.9007634],
                    tex_coords: [0.76171875, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.06066948, 0.9007634],
                    tex_coords: [0.76171875, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.06066948, 0.8683206],
                    tex_coords: [0.76171875, 0.080078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.098326385, 0.8683206],
                    tex_coords: [0.7265625, 0.080078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.056485355, 0.86641216],
                    tex_coords: [0.3828125, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.056485355, 0.9045801],
                    tex_coords: [0.3828125, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.016736388, 0.9045801],
                    tex_coords: [0.41992188, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.016736388, 0.9045801],
                    tex_coords: [0.41992188, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.016736388, 0.86641216],
                    tex_coords: [0.41992188, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.056485355, 0.86641216],
                    tex_coords: [0.3828125, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.0104602575, 0.86641216],
                    tex_coords: [0.42382813, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.0104602575, 0.9045801],
                    tex_coords: [0.42382813, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.023012519, 0.9045801],
                    tex_coords: [0.45507813, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.023012519, 0.9045801],
                    tex_coords: [0.45507813, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.023012519, 0.86641216],
                    tex_coords: [0.45507813, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.0104602575, 0.86641216],
                    tex_coords: [0.42382813, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.02928865, 0.86641216],
                    tex_coords: [0.45898438, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.02928865, 0.9045801],
                    tex_coords: [0.45898438, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.069037676, 0.9045801],
                    tex_coords: [0.49609375, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.069037676, 0.9045801],
                    tex_coords: [0.49609375, 0.001953125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.069037676, 0.86641216],
                    tex_coords: [0.49609375, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.02928865, 0.86641216],
                    tex_coords: [0.45898438, 0.041015625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.07740581, 0.86641216],
                    tex_coords: [0.29101563, 0.1875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.07740581, 0.9026718],
                    tex_coords: [0.29101563, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.10669458, 0.9026718],
                    tex_coords: [0.31835938, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.10669458, 0.9026718],
                    tex_coords: [0.31835938, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.10669458, 0.86641216],
                    tex_coords: [0.31835938, 0.1875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.07740581, 0.86641216],
                    tex_coords: [0.29101563, 0.1875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.11297071, 0.86641216],
                    tex_coords: [0.001953125, 0.18945313],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.11297071, 0.9045801],
                    tex_coords: [0.001953125, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.15481174, 0.9045801],
                    tex_coords: [0.041015625, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.15481174, 0.9045801],
                    tex_coords: [0.041015625, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.15481174, 0.86641216],
                    tex_coords: [0.041015625, 0.18945313],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.11297071, 0.86641216],
                    tex_coords: [0.001953125, 0.18945313],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.15899587, 0.8683206],
                    tex_coords: [0.32226563, 0.18554688],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.15899587, 0.9026718],
                    tex_coords: [0.32226563, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.19665277, 0.9026718],
                    tex_coords: [0.35742188, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.19665277, 0.9026718],
                    tex_coords: [0.35742188, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.19665277, 0.8683206],
                    tex_coords: [0.35742188, 0.18554688],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.15899587, 0.8683206],
                    tex_coords: [0.32226563, 0.18554688],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.2029289, 0.8683206],
                    tex_coords: [0.5234375, 0.18359375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.2029289, 0.9007634],
                    tex_coords: [0.5234375, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.23849368, 0.9007634],
                    tex_coords: [0.5566406, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.23849368, 0.9007634],
                    tex_coords: [0.5566406, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.23849368, 0.8683206],
                    tex_coords: [0.5566406, 0.18359375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.2029289, 0.8683206],
                    tex_coords: [0.5234375, 0.18359375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.24267781, 0.86641216],
                    tex_coords: [0.044921875, 0.18945313],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.24267781, 0.9045801],
                    tex_coords: [0.044921875, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.28451884, 0.9045801],
                    tex_coords: [0.083984375, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.28451884, 0.9045801],
                    tex_coords: [0.083984375, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.28451884, 0.86641216],
                    tex_coords: [0.083984375, 0.18945313],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.24267781, 0.86641216],
                    tex_coords: [0.044921875, 0.18945313],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.28661084, 0.8683206],
                    tex_coords: [0.2109375, 0.1875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.28661084, 0.9045801],
                    tex_coords: [0.2109375, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.32426775, 0.9045801],
                    tex_coords: [0.24609375, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.32426775, 0.9045801],
                    tex_coords: [0.24609375, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.32426775, 0.8683206],
                    tex_coords: [0.24609375, 0.1875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.28661084, 0.8683206],
                    tex_coords: [0.2109375, 0.1875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.32845187, 0.86641216],
                    tex_coords: [0.087890625, 0.18945313],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.32845187, 0.9045801],
                    tex_coords: [0.087890625, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.36820078, 0.9045801],
                    tex_coords: [0.125, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.36820078, 0.9045801],
                    tex_coords: [0.125, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.36820078, 0.86641216],
                    tex_coords: [0.125, 0.18945313],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.32845187, 0.86641216],
                    tex_coords: [0.087890625, 0.18945313],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.3723849, 0.86641216],
                    tex_coords: [0.25, 0.1875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.3723849, 0.9026718],
                    tex_coords: [0.25, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.41213393, 0.9026718],
                    tex_coords: [0.28710938, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.41213393, 0.9026718],
                    tex_coords: [0.28710938, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.41213393, 0.86641216],
                    tex_coords: [0.28710938, 0.1875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.3723849, 0.86641216],
                    tex_coords: [0.25, 0.1875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.4225942, 0.870229],
                    tex_coords: [0.36132813, 0.18554688],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.4225942, 0.9045801],
                    tex_coords: [0.36132813, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.45397484, 0.9045801],
                    tex_coords: [0.390625, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.45397484, 0.9045801],
                    tex_coords: [0.390625, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.45397484, 0.870229],
                    tex_coords: [0.390625, 0.18554688],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.4225942, 0.870229],
                    tex_coords: [0.36132813, 0.18554688],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.45815897, 0.86641216],
                    tex_coords: [0.12890625, 0.18945313],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.45815897, 0.9045801],
                    tex_coords: [0.12890625, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.497908, 0.9045801],
                    tex_coords: [0.16601563, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.497908, 0.9045801],
                    tex_coords: [0.16601563, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.497908, 0.86641216],
                    tex_coords: [0.16601563, 0.18945313],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.45815897, 0.86641216],
                    tex_coords: [0.12890625, 0.18945313],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.502092, 0.8683206],
                    tex_coords: [0.39453125, 0.18554688],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.502092, 0.9026718],
                    tex_coords: [0.39453125, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.54184103, 0.9026718],
                    tex_coords: [0.43164063, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.54184103, 0.9026718],
                    tex_coords: [0.43164063, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.54184103, 0.8683206],
                    tex_coords: [0.43164063, 0.18554688],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.502092, 0.8683206],
                    tex_coords: [0.39453125, 0.18554688],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.54393303, 0.8683206],
                    tex_coords: [0.43554688, 0.18554688],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.54393303, 0.9026718],
                    tex_coords: [0.43554688, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.58158994, 0.9026718],
                    tex_coords: [0.47070313, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.58158994, 0.9026718],
                    tex_coords: [0.47070313, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.58158994, 0.8683206],
                    tex_coords: [0.47070313, 0.18554688],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.54393303, 0.8683206],
                    tex_coords: [0.43554688, 0.18554688],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.58577406, 0.870229],
                    tex_coords: [0.5605469, 0.18359375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.58577406, 0.9026718],
                    tex_coords: [0.5605469, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.625523, 0.9026718],
                    tex_coords: [0.59765625, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.625523, 0.9026718],
                    tex_coords: [0.59765625, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.625523, 0.870229],
                    tex_coords: [0.59765625, 0.18359375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.58577406, 0.870229],
                    tex_coords: [0.5605469, 0.18359375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.6297071, 0.8683206],
                    tex_coords: [0.16992188, 0.18945313],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.6297071, 0.90648854],
                    tex_coords: [0.16992188, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.6694561, 0.90648854],
                    tex_coords: [0.20703125, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.6694561, 0.90648854],
                    tex_coords: [0.20703125, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.6694561, 0.8683206],
                    tex_coords: [0.20703125, 0.18945313],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.6297071, 0.8683206],
                    tex_coords: [0.16992188, 0.18945313],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.99790794, 0.77862597],
                    tex_coords: [0.9511719, 0.078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.99790794, 0.80916035],
                    tex_coords: [0.9511719, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.9790795, 0.80916035],
                    tex_coords: [0.96875, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.9790795, 0.80916035],
                    tex_coords: [0.96875, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.9790795, 0.77862597],
                    tex_coords: [0.96875, 0.078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.99790794, 0.77862597],
                    tex_coords: [0.9511719, 0.078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.9769874, 0.77862597],
                    tex_coords: [0.29492188, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.9769874, 0.8034351],
                    tex_coords: [0.29492188, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.95815897, 0.8034351],
                    tex_coords: [0.3125, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.95815897, 0.8034351],
                    tex_coords: [0.3125, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.95815897, 0.77862597],
                    tex_coords: [0.3125, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.9769874, 0.77862597],
                    tex_coords: [0.29492188, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.9539749, 0.77862597],
                    tex_coords: [0.22851563, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.9539749, 0.8034351],
                    tex_coords: [0.22851563, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.93514645, 0.8034351],
                    tex_coords: [0.24609375, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.93514645, 0.8034351],
                    tex_coords: [0.24609375, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.93514645, 0.77862597],
                    tex_coords: [0.24609375, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.9539749, 0.77862597],
                    tex_coords: [0.22851563, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.9309623, 0.77862597],
                    tex_coords: [0.52734375, 0.080078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.9309623, 0.81106865],
                    tex_coords: [0.52734375, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.9246862, 0.81106865],
                    tex_coords: [0.5332031, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.9246862, 0.81106865],
                    tex_coords: [0.5332031, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.9246862, 0.77862597],
                    tex_coords: [0.5332031, 0.080078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.9309623, 0.77862597],
                    tex_coords: [0.52734375, 0.080078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.9121339, 0.77862597],
                    tex_coords: [0.5371094, 0.080078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.9121339, 0.81106865],
                    tex_coords: [0.5371094, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8933054, 0.81106865],
                    tex_coords: [0.5546875, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8933054, 0.81106865],
                    tex_coords: [0.5546875, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8933054, 0.77862597],
                    tex_coords: [0.5546875, 0.080078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.9121339, 0.77862597],
                    tex_coords: [0.5371094, 0.080078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8933054, 0.77862597],
                    tex_coords: [0.36328125, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8933054, 0.8034351],
                    tex_coords: [0.36328125, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8786611, 0.8034351],
                    tex_coords: [0.37695313, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8786611, 0.8034351],
                    tex_coords: [0.37695313, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8786611, 0.77862597],
                    tex_coords: [0.37695313, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8933054, 0.77862597],
                    tex_coords: [0.36328125, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8786611, 0.77862597],
                    tex_coords: [0.38085938, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8786611, 0.8034351],
                    tex_coords: [0.38085938, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8577406, 0.8034351],
                    tex_coords: [0.40039063, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8577406, 0.8034351],
                    tex_coords: [0.40039063, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8577406, 0.77862597],
                    tex_coords: [0.40039063, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8786611, 0.77862597],
                    tex_coords: [0.38085938, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8556485, 0.77862597],
                    tex_coords: [0.40429688, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8556485, 0.8034351],
                    tex_coords: [0.40429688, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.834728, 0.8034351],
                    tex_coords: [0.42382813, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.834728, 0.8034351],
                    tex_coords: [0.42382813, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.834728, 0.77862597],
                    tex_coords: [0.42382813, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.8556485, 0.77862597],
                    tex_coords: [0.40429688, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.82217574, 0.77862597],
                    tex_coords: [0.03125, 0.115234375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.82217574, 0.80725193],
                    tex_coords: [0.03125, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.80753136, 0.80725193],
                    tex_coords: [0.044921875, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.80753136, 0.80725193],
                    tex_coords: [0.044921875, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.80753136, 0.77862597],
                    tex_coords: [0.044921875, 0.115234375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.82217574, 0.77862597],
                    tex_coords: [0.03125, 0.115234375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.80753136, 0.77862597],
                    tex_coords: [0.48242188, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.80753136, 0.8034351],
                    tex_coords: [0.48242188, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.78451884, 0.8034351],
                    tex_coords: [0.50390625, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.78451884, 0.8034351],
                    tex_coords: [0.50390625, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.78451884, 0.77862597],
                    tex_coords: [0.50390625, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.80753136, 0.77862597],
                    tex_coords: [0.48242188, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.7719665, 0.77862597],
                    tex_coords: [0.06640625, 0.115234375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.7719665, 0.80725193],
                    tex_coords: [0.06640625, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.7573222, 0.80725193],
                    tex_coords: [0.080078125, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.7573222, 0.80725193],
                    tex_coords: [0.080078125, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.7573222, 0.77862597],
                    tex_coords: [0.080078125, 0.115234375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.7719665, 0.77862597],
                    tex_coords: [0.06640625, 0.115234375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.7594142, 0.769084],
                    tex_coords: [0.55859375, 0.080078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.7594142, 0.80152667],
                    tex_coords: [0.55859375, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.7364017, 0.80152667],
                    tex_coords: [0.5800781, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.7364017, 0.80152667],
                    tex_coords: [0.5800781, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.7364017, 0.769084],
                    tex_coords: [0.5800781, 0.080078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.7594142, 0.769084],
                    tex_coords: [0.55859375, 0.080078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.7343096, 0.769084],
                    tex_coords: [0.45703125, 0.08203125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.7343096, 0.8034351],
                    tex_coords: [0.45703125, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.71338916, 0.8034351],
                    tex_coords: [0.4765625, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.71338916, 0.8034351],
                    tex_coords: [0.4765625, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.71338916, 0.769084],
                    tex_coords: [0.4765625, 0.08203125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.7343096, 0.769084],
                    tex_coords: [0.45703125, 0.08203125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.71129704, 0.77862597],
                    tex_coords: [0.16015625, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.71129704, 0.8034351],
                    tex_coords: [0.16015625, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.6903766, 0.8034351],
                    tex_coords: [0.1796875, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.6903766, 0.8034351],
                    tex_coords: [0.1796875, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.6903766, 0.77862597],
                    tex_coords: [0.1796875, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.71129704, 0.77862597],
                    tex_coords: [0.16015625, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.67573225, 0.77862597],
                    tex_coords: [0.76171875, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.67573225, 0.8034351],
                    tex_coords: [0.76171875, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.65481174, 0.8034351],
                    tex_coords: [0.78125, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.65481174, 0.8034351],
                    tex_coords: [0.78125, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.65481174, 0.77862597],
                    tex_coords: [0.78125, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.67573225, 0.77862597],
                    tex_coords: [0.76171875, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.6506276, 0.77862597],
                    tex_coords: [0.119140625, 0.14453125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.6506276, 0.80152667],
                    tex_coords: [0.119140625, 0.12109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.6297071, 0.80152667],
                    tex_coords: [0.13867188, 0.12109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.6297071, 0.80152667],
                    tex_coords: [0.13867188, 0.12109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.6297071, 0.77862597],
                    tex_coords: [0.13867188, 0.14453125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.6506276, 0.77862597],
                    tex_coords: [0.119140625, 0.14453125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.6276151, 0.77862597],
                    tex_coords: [0.083984375, 0.115234375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.6276151, 0.80725193],
                    tex_coords: [0.083984375, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.6129707, 0.80725193],
                    tex_coords: [0.09765625, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.6129707, 0.80725193],
                    tex_coords: [0.09765625, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.6129707, 0.77862597],
                    tex_coords: [0.09765625, 0.115234375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.6276151, 0.77862597],
                    tex_coords: [0.083984375, 0.115234375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.60041845, 0.77862597],
                    tex_coords: [0.84765625, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.60041845, 0.8034351],
                    tex_coords: [0.84765625, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.583682, 0.8034351],
                    tex_coords: [0.86328125, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.583682, 0.8034351],
                    tex_coords: [0.86328125, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.583682, 0.77862597],
                    tex_coords: [0.86328125, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.60041845, 0.77862597],
                    tex_coords: [0.84765625, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.58158994, 0.77862597],
                    tex_coords: [0.8671875, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.58158994, 0.8034351],
                    tex_coords: [0.8671875, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.5585774, 0.8034351],
                    tex_coords: [0.8886719, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.5585774, 0.8034351],
                    tex_coords: [0.8886719, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.5585774, 0.77862597],
                    tex_coords: [0.8886719, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.58158994, 0.77862597],
                    tex_coords: [0.8671875, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.55648535, 0.77862597],
                    tex_coords: [0.8925781, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.55648535, 0.8034351],
                    tex_coords: [0.8925781, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.5230125, 0.8034351],
                    tex_coords: [0.9238281, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.5230125, 0.8034351],
                    tex_coords: [0.9238281, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.5230125, 0.77862597],
                    tex_coords: [0.9238281, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.55648535, 0.77862597],
                    tex_coords: [0.8925781, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.51882845, 0.77862597],
                    tex_coords: [0.27148438, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.51882845, 0.8034351],
                    tex_coords: [0.27148438, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.49790794, 0.8034351],
                    tex_coords: [0.29101563, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.49790794, 0.8034351],
                    tex_coords: [0.29101563, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.49790794, 0.77862597],
                    tex_coords: [0.29101563, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.51882845, 0.77862597],
                    tex_coords: [0.27148438, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.48535568, 0.77862597],
                    tex_coords: [0.06640625, 0.115234375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.48535568, 0.80725193],
                    tex_coords: [0.06640625, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.4707113, 0.80725193],
                    tex_coords: [0.080078125, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.4707113, 0.80725193],
                    tex_coords: [0.080078125, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.4707113, 0.77862597],
                    tex_coords: [0.080078125, 0.115234375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.48535568, 0.77862597],
                    tex_coords: [0.06640625, 0.115234375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.4707113, 0.77862597],
                    tex_coords: [0.16015625, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.4707113, 0.8034351],
                    tex_coords: [0.16015625, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.44979078, 0.8034351],
                    tex_coords: [0.1796875, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.44979078, 0.8034351],
                    tex_coords: [0.1796875, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.44979078, 0.77862597],
                    tex_coords: [0.1796875, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.4707113, 0.77862597],
                    tex_coords: [0.16015625, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.44769877, 0.77862597],
                    tex_coords: [0.16210938, 0.14453125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.44769877, 0.80152667],
                    tex_coords: [0.16210938, 0.12109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.42677826, 0.80152667],
                    tex_coords: [0.18164063, 0.12109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.42677826, 0.80152667],
                    tex_coords: [0.18164063, 0.12109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.42677826, 0.77862597],
                    tex_coords: [0.18164063, 0.14453125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.44769877, 0.77862597],
                    tex_coords: [0.16210938, 0.14453125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.42677826, 0.77862597],
                    tex_coords: [0.083984375, 0.115234375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.42677826, 0.80725193],
                    tex_coords: [0.083984375, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.41213387, 0.80725193],
                    tex_coords: [0.09765625, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.41213387, 0.80725193],
                    tex_coords: [0.09765625, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.41213387, 0.77862597],
                    tex_coords: [0.09765625, 0.115234375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.42677826, 0.77862597],
                    tex_coords: [0.083984375, 0.115234375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.41213387, 0.7748091],
                    tex_coords: [0.22070313, 0.1328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.41213387, 0.78625953],
                    tex_coords: [0.22070313, 0.12109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.40376568, 0.78625953],
                    tex_coords: [0.22851563, 0.12109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.40376568, 0.78625953],
                    tex_coords: [0.22851563, 0.12109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.40376568, 0.7748091],
                    tex_coords: [0.22851563, 0.1328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.41213387, 0.7748091],
                    tex_coords: [0.22070313, 0.1328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.38912135, 0.77862597],
                    tex_coords: [0.18359375, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.38912135, 0.8034351],
                    tex_coords: [0.18359375, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.3702929, 0.8034351],
                    tex_coords: [0.20117188, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.3702929, 0.8034351],
                    tex_coords: [0.20117188, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.3702929, 0.77862597],
                    tex_coords: [0.20117188, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.38912135, 0.77862597],
                    tex_coords: [0.18359375, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.36610878, 0.77862597],
                    tex_coords: [0.20507813, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.36610878, 0.8034351],
                    tex_coords: [0.20507813, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.34518826, 0.8034351],
                    tex_coords: [0.22460938, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.34518826, 0.8034351],
                    tex_coords: [0.22460938, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.34518826, 0.77862597],
                    tex_coords: [0.22460938, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.36610878, 0.77862597],
                    tex_coords: [0.20507813, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.3410042, 0.77862597],
                    tex_coords: [0.625, 0.080078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.3410042, 0.81106865],
                    tex_coords: [0.625, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.32008368, 0.81106865],
                    tex_coords: [0.64453125, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.32008368, 0.81106865],
                    tex_coords: [0.64453125, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.32008368, 0.77862597],
                    tex_coords: [0.64453125, 0.080078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.3410042, 0.77862597],
                    tex_coords: [0.625, 0.080078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.30543935, 0.77862597],
                    tex_coords: [0.6484375, 0.080078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.30543935, 0.81106865],
                    tex_coords: [0.6484375, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.28451884, 0.81106865],
                    tex_coords: [0.66796875, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.28451884, 0.81106865],
                    tex_coords: [0.66796875, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.28451884, 0.77862597],
                    tex_coords: [0.66796875, 0.080078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.30543935, 0.77862597],
                    tex_coords: [0.6484375, 0.080078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.2803347, 0.77862597],
                    tex_coords: [0.27148438, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.2803347, 0.8034351],
                    tex_coords: [0.27148438, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.25941426, 0.8034351],
                    tex_coords: [0.29101563, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.25941426, 0.8034351],
                    tex_coords: [0.29101563, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.25941426, 0.77862597],
                    tex_coords: [0.29101563, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.2803347, 0.77862597],
                    tex_coords: [0.27148438, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.25523013, 0.77862597],
                    tex_coords: [0.671875, 0.080078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.25523013, 0.81106865],
                    tex_coords: [0.671875, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.248954, 0.81106865],
                    tex_coords: [0.6777344, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.248954, 0.81106865],
                    tex_coords: [0.6777344, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.248954, 0.77862597],
                    tex_coords: [0.6777344, 0.080078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.25523013, 0.77862597],
                    tex_coords: [0.671875, 0.080078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.24686193, 0.77862597],
                    tex_coords: [0.31640625, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.24686193, 0.8034351],
                    tex_coords: [0.31640625, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.22594142, 0.8034351],
                    tex_coords: [0.3359375, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.22594142, 0.8034351],
                    tex_coords: [0.3359375, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.22594142, 0.77862597],
                    tex_coords: [0.3359375, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.24686193, 0.77862597],
                    tex_coords: [0.31640625, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.22384936, 0.77862597],
                    tex_coords: [0.03125, 0.115234375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.22384936, 0.80725193],
                    tex_coords: [0.03125, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.20920503, 0.80725193],
                    tex_coords: [0.044921875, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.20920503, 0.80725193],
                    tex_coords: [0.044921875, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.20920503, 0.77862597],
                    tex_coords: [0.044921875, 0.115234375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.22384936, 0.77862597],
                    tex_coords: [0.03125, 0.115234375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.20920503, 0.77862597],
                    tex_coords: [0.16015625, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.20920503, 0.8034351],
                    tex_coords: [0.16015625, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.18828452, 0.8034351],
                    tex_coords: [0.1796875, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.18828452, 0.8034351],
                    tex_coords: [0.1796875, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.18828452, 0.77862597],
                    tex_coords: [0.1796875, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.20920503, 0.77862597],
                    tex_coords: [0.16015625, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.17364019, 0.77862597],
                    tex_coords: [0.86328125, 0.078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.17364019, 0.80916035],
                    tex_coords: [0.86328125, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.167364, 0.80916035],
                    tex_coords: [0.8691406, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.167364, 0.80916035],
                    tex_coords: [0.8691406, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.167364, 0.77862597],
                    tex_coords: [0.8691406, 0.078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.17364019, 0.77862597],
                    tex_coords: [0.86328125, 0.078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.16527194, 0.77862597],
                    tex_coords: [0.1015625, 0.115234375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.16527194, 0.80725193],
                    tex_coords: [0.1015625, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.15062761, 0.80725193],
                    tex_coords: [0.115234375, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.15062761, 0.80725193],
                    tex_coords: [0.115234375, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.15062761, 0.77862597],
                    tex_coords: [0.115234375, 0.115234375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.16527194, 0.77862597],
                    tex_coords: [0.1015625, 0.115234375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.14016736, 0.77862597],
                    tex_coords: [0.083984375, 0.14453125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.14016736, 0.80152667],
                    tex_coords: [0.083984375, 0.12109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.10669458, 0.80152667],
                    tex_coords: [0.115234375, 0.12109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.10669458, 0.80152667],
                    tex_coords: [0.115234375, 0.12109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.10669458, 0.77862597],
                    tex_coords: [0.115234375, 0.14453125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.14016736, 0.77862597],
                    tex_coords: [0.083984375, 0.14453125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.104602516, 0.77862597],
                    tex_coords: [0.8730469, 0.078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.104602516, 0.80916035],
                    tex_coords: [0.8730469, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.098326385, 0.80916035],
                    tex_coords: [0.87890625, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.098326385, 0.80916035],
                    tex_coords: [0.87890625, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.098326385, 0.77862597],
                    tex_coords: [0.87890625, 0.078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.104602516, 0.77862597],
                    tex_coords: [0.8730469, 0.078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.09623432, 0.77862597],
                    tex_coords: [0.048828125, 0.115234375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.09623432, 0.80725193],
                    tex_coords: [0.048828125, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.08158994, 0.80725193],
                    tex_coords: [0.0625, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.08158994, 0.80725193],
                    tex_coords: [0.0625, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.08158994, 0.77862597],
                    tex_coords: [0.0625, 0.115234375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.09623432, 0.77862597],
                    tex_coords: [0.048828125, 0.115234375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.07949793, 0.77862597],
                    tex_coords: [0.6816406, 0.080078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.07949793, 0.81106865],
                    tex_coords: [0.6816406, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.05857742, 0.81106865],
                    tex_coords: [0.7011719, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.05857742, 0.81106865],
                    tex_coords: [0.7011719, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.05857742, 0.77862597],
                    tex_coords: [0.7011719, 0.080078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.07949793, 0.77862597],
                    tex_coords: [0.6816406, 0.080078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.04184103, 0.77862597],
                    tex_coords: [0.8828125, 0.078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.04184103, 0.80916035],
                    tex_coords: [0.8828125, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.020920515, 0.80916035],
                    tex_coords: [0.90234375, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.020920515, 0.80916035],
                    tex_coords: [0.90234375, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.020920515, 0.77862597],
                    tex_coords: [0.90234375, 0.078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.04184103, 0.77862597],
                    tex_coords: [0.8828125, 0.078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.016736388, 0.77862597],
                    tex_coords: [0.25, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.016736388, 0.8034351],
                    tex_coords: [0.25, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.0020920038, 0.8034351],
                    tex_coords: [0.26757813, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.0020920038, 0.8034351],
                    tex_coords: [0.26757813, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.0020920038, 0.77862597],
                    tex_coords: [0.26757813, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [-0.016736388, 0.77862597],
                    tex_coords: [0.25, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.0062761307, 0.77862597],
                    tex_coords: [0.46289063, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.0062761307, 0.8034351],
                    tex_coords: [0.46289063, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.023012519, 0.8034351],
                    tex_coords: [0.47851563, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.023012519, 0.8034351],
                    tex_coords: [0.47851563, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.023012519, 0.77862597],
                    tex_coords: [0.47851563, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.0062761307, 0.77862597],
                    tex_coords: [0.46289063, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.027196646, 0.77862597],
                    tex_coords: [0.7050781, 0.080078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.027196646, 0.81106865],
                    tex_coords: [0.7050781, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.046025157, 0.81106865],
                    tex_coords: [0.72265625, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.046025157, 0.81106865],
                    tex_coords: [0.72265625, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.046025157, 0.77862597],
                    tex_coords: [0.72265625, 0.080078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.027196646, 0.77862597],
                    tex_coords: [0.7050781, 0.080078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.046025157, 0.77862597],
                    tex_coords: [0.5078125, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.046025157, 0.8034351],
                    tex_coords: [0.5078125, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.062761545, 0.8034351],
                    tex_coords: [0.5234375, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.062761545, 0.8034351],
                    tex_coords: [0.5234375, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.062761545, 0.77862597],
                    tex_coords: [0.5234375, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.046025157, 0.77862597],
                    tex_coords: [0.5078125, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.06694555, 0.769084],
                    tex_coords: [0.47460938, 0.18554688],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.06694555, 0.8034351],
                    tex_coords: [0.47460938, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.08786607, 0.8034351],
                    tex_coords: [0.49414063, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.08786607, 0.8034351],
                    tex_coords: [0.49414063, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.08786607, 0.769084],
                    tex_coords: [0.49414063, 0.18554688],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.06694555, 0.769084],
                    tex_coords: [0.47460938, 0.18554688],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.08995819, 0.77862597],
                    tex_coords: [0.6933594, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.08995819, 0.8034351],
                    tex_coords: [0.6933594, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.110878706, 0.8034351],
                    tex_coords: [0.7128906, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.110878706, 0.8034351],
                    tex_coords: [0.7128906, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.110878706, 0.77862597],
                    tex_coords: [0.7128906, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.08995819, 0.77862597],
                    tex_coords: [0.6933594, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.11297071, 0.77862597],
                    tex_coords: [0.7890625, 0.17578125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.11297071, 0.8034351],
                    tex_coords: [0.7890625, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.13179922, 0.8034351],
                    tex_coords: [0.8066406, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.13179922, 0.8034351],
                    tex_coords: [0.8066406, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.13179922, 0.77862597],
                    tex_coords: [0.8066406, 0.17578125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.11297071, 0.77862597],
                    tex_coords: [0.7890625, 0.17578125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.13389122, 0.77862597],
                    tex_coords: [0.87109375, 0.17578125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.13389122, 0.8034351],
                    tex_coords: [0.87109375, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.15481174, 0.8034351],
                    tex_coords: [0.890625, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.15481174, 0.8034351],
                    tex_coords: [0.890625, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.15481174, 0.77862597],
                    tex_coords: [0.890625, 0.17578125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.13389122, 0.77862597],
                    tex_coords: [0.87109375, 0.17578125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.15690374, 0.77862597],
                    tex_coords: [0.3125, 0.12890625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.15690374, 0.78625953],
                    tex_coords: [0.3125, 0.12109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.165272, 0.78625953],
                    tex_coords: [0.3203125, 0.12109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.165272, 0.78625953],
                    tex_coords: [0.3203125, 0.12109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.165272, 0.77862597],
                    tex_coords: [0.3203125, 0.12890625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.15690374, 0.77862597],
                    tex_coords: [0.3125, 0.12890625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.17573225, 0.77862597],
                    tex_coords: [0.671875, 0.18164063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.17573225, 0.80916035],
                    tex_coords: [0.671875, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.20083678, 0.80916035],
                    tex_coords: [0.6953125, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.20083678, 0.80916035],
                    tex_coords: [0.6953125, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.20083678, 0.77862597],
                    tex_coords: [0.6953125, 0.18164063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.17573225, 0.77862597],
                    tex_coords: [0.671875, 0.18164063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.20083678, 0.77862597],
                    tex_coords: [0.48242188, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.20083678, 0.8034351],
                    tex_coords: [0.48242188, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.22384942, 0.8034351],
                    tex_coords: [0.50390625, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.22384942, 0.8034351],
                    tex_coords: [0.50390625, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.22384942, 0.77862597],
                    tex_coords: [0.50390625, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.20083678, 0.77862597],
                    tex_coords: [0.48242188, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.22803342, 0.77862597],
                    tex_coords: [0.9550781, 0.17382813],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.22803342, 0.80152667],
                    tex_coords: [0.9550781, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.24686193, 0.80152667],
                    tex_coords: [0.97265625, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.24686193, 0.80152667],
                    tex_coords: [0.97265625, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.24686193, 0.77862597],
                    tex_coords: [0.97265625, 0.17382813],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.22803342, 0.77862597],
                    tex_coords: [0.9550781, 0.17382813],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.26359832, 0.77862597],
                    tex_coords: [0.46289063, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.26359832, 0.8034351],
                    tex_coords: [0.46289063, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.2803347, 0.8034351],
                    tex_coords: [0.47851563, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.2803347, 0.8034351],
                    tex_coords: [0.47851563, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.2803347, 0.77862597],
                    tex_coords: [0.47851563, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.26359832, 0.77862597],
                    tex_coords: [0.46289063, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.28242683, 0.77862597],
                    tex_coords: [0.8105469, 0.17578125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.28242683, 0.8034351],
                    tex_coords: [0.8105469, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.30125523, 0.8034351],
                    tex_coords: [0.828125, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.30125523, 0.8034351],
                    tex_coords: [0.828125, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.30125523, 0.77862597],
                    tex_coords: [0.828125, 0.17578125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.28242683, 0.77862597],
                    tex_coords: [0.8105469, 0.17578125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.30543935, 0.77862597],
                    tex_coords: [0.20507813, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.30543935, 0.8034351],
                    tex_coords: [0.20507813, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.32635987, 0.8034351],
                    tex_coords: [0.22460938, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.32635987, 0.8034351],
                    tex_coords: [0.22460938, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.32635987, 0.77862597],
                    tex_coords: [0.22460938, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.30543935, 0.77862597],
                    tex_coords: [0.20507813, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.34100413, 0.77862597],
                    tex_coords: [0.83203125, 0.17578125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.34100413, 0.8034351],
                    tex_coords: [0.83203125, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.35983264, 0.8034351],
                    tex_coords: [0.8496094, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.35983264, 0.8034351],
                    tex_coords: [0.8496094, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.35983264, 0.77862597],
                    tex_coords: [0.8496094, 0.17578125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.34100413, 0.77862597],
                    tex_coords: [0.83203125, 0.17578125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.36610878, 0.77862597],
                    tex_coords: [0.671875, 0.080078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.36610878, 0.81106865],
                    tex_coords: [0.671875, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.3723849, 0.81106865],
                    tex_coords: [0.6777344, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.3723849, 0.81106865],
                    tex_coords: [0.6777344, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.3723849, 0.77862597],
                    tex_coords: [0.6777344, 0.080078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.36610878, 0.77862597],
                    tex_coords: [0.671875, 0.080078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.37447703, 0.77862597],
                    tex_coords: [0.609375, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.37447703, 0.8034351],
                    tex_coords: [0.609375, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.39330542, 0.8034351],
                    tex_coords: [0.6269531, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.39330542, 0.8034351],
                    tex_coords: [0.6269531, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.39330542, 0.77862597],
                    tex_coords: [0.6269531, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.37447703, 0.77862597],
                    tex_coords: [0.609375, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.39539754, 0.77862597],
                    tex_coords: [0.76171875, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.39539754, 0.8034351],
                    tex_coords: [0.76171875, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.41631794, 0.8034351],
                    tex_coords: [0.78125, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.41631794, 0.8034351],
                    tex_coords: [0.78125, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.41631794, 0.77862597],
                    tex_coords: [0.78125, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.39539754, 0.77862597],
                    tex_coords: [0.76171875, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.42887032, 0.77862597],
                    tex_coords: [0.048828125, 0.115234375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.42887032, 0.80725193],
                    tex_coords: [0.048828125, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.4435146, 0.80725193],
                    tex_coords: [0.0625, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.4435146, 0.80725193],
                    tex_coords: [0.0625, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.4435146, 0.77862597],
                    tex_coords: [0.0625, 0.115234375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.42887032, 0.77862597],
                    tex_coords: [0.048828125, 0.115234375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.4456067, 0.77862597],
                    tex_coords: [0.8535156, 0.17578125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.4456067, 0.8034351],
                    tex_coords: [0.8535156, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.4602511, 0.8034351],
                    tex_coords: [0.8671875, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.4602511, 0.8034351],
                    tex_coords: [0.8671875, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.4602511, 0.77862597],
                    tex_coords: [0.8671875, 0.17578125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.4456067, 0.77862597],
                    tex_coords: [0.8535156, 0.17578125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.4602511, 0.769084],
                    tex_coords: [0.6015625, 0.18359375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.4602511, 0.80152667],
                    tex_coords: [0.6015625, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.4832636, 0.80152667],
                    tex_coords: [0.6230469, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.4832636, 0.80152667],
                    tex_coords: [0.6230469, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.4832636, 0.769084],
                    tex_coords: [0.6230469, 0.18359375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.4602511, 0.769084],
                    tex_coords: [0.6015625, 0.18359375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.49581587, 0.77862597],
                    tex_coords: [0.91796875, 0.17578125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.49581587, 0.8034351],
                    tex_coords: [0.91796875, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.51046026, 0.8034351],
                    tex_coords: [0.9316406, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.51046026, 0.8034351],
                    tex_coords: [0.9316406, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.51046026, 0.77862597],
                    tex_coords: [0.9316406, 0.17578125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.49581587, 0.77862597],
                    tex_coords: [0.91796875, 0.17578125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.51046026, 0.77862597],
                    tex_coords: [0.16015625, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.51046026, 0.8034351],
                    tex_coords: [0.16015625, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.5313808, 0.8034351],
                    tex_coords: [0.1796875, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.5313808, 0.8034351],
                    tex_coords: [0.1796875, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.5313808, 0.77862597],
                    tex_coords: [0.1796875, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.51046026, 0.77862597],
                    tex_coords: [0.16015625, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.5334728, 0.77862597],
                    tex_coords: [0.76953125, 0.17578125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.5334728, 0.8034351],
                    tex_coords: [0.76953125, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.55020916, 0.8034351],
                    tex_coords: [0.78515625, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.55020916, 0.8034351],
                    tex_coords: [0.78515625, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.55020916, 0.77862597],
                    tex_coords: [0.78515625, 0.17578125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.5334728, 0.77862597],
                    tex_coords: [0.76953125, 0.17578125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.5543933, 0.77862597],
                    tex_coords: [0.86328125, 0.078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.5543933, 0.80916035],
                    tex_coords: [0.86328125, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.5606694, 0.80916035],
                    tex_coords: [0.8691406, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.5606694, 0.80916035],
                    tex_coords: [0.8691406, 0.046875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.5606694, 0.77862597],
                    tex_coords: [0.8691406, 0.078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.5543933, 0.77862597],
                    tex_coords: [0.86328125, 0.078125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.56485355, 0.77862597],
                    tex_coords: [0.9355469, 0.17382813],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.56485355, 0.80152667],
                    tex_coords: [0.9355469, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.58158994, 0.80152667],
                    tex_coords: [0.9511719, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.58158994, 0.80152667],
                    tex_coords: [0.9511719, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.58158994, 0.77862597],
                    tex_coords: [0.9511719, 0.17382813],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.56485355, 0.77862597],
                    tex_coords: [0.9355469, 0.17382813],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.58368206, 0.77862597],
                    tex_coords: [0.69921875, 0.18164063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.58368206, 0.80916035],
                    tex_coords: [0.69921875, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.5899582, 0.80916035],
                    tex_coords: [0.7050781, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.5899582, 0.80916035],
                    tex_coords: [0.7050781, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.5899582, 0.77862597],
                    tex_coords: [0.7050781, 0.18164063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.58368206, 0.77862597],
                    tex_coords: [0.69921875, 0.18164063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.5962343, 0.77862597],
                    tex_coords: [0.7480469, 0.17578125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.5962343, 0.8034351],
                    tex_coords: [0.7480469, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.6150627, 0.8034351],
                    tex_coords: [0.765625, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.6150627, 0.8034351],
                    tex_coords: [0.765625, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.6150627, 0.77862597],
                    tex_coords: [0.765625, 0.17578125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.5962343, 0.77862597],
                    tex_coords: [0.7480469, 0.17578125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.61715484, 0.769084],
                    tex_coords: [0.49804688, 0.18554688],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.61715484, 0.8034351],
                    tex_coords: [0.49804688, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.64016736, 0.8034351],
                    tex_coords: [0.51953125, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.64016736, 0.8034351],
                    tex_coords: [0.51953125, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.64016736, 0.769084],
                    tex_coords: [0.51953125, 0.18554688],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.61715484, 0.769084],
                    tex_coords: [0.49804688, 0.18554688],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.6506276, 0.77862597],
                    tex_coords: [0.7285156, 0.1796875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.6506276, 0.80725193],
                    tex_coords: [0.7285156, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.667364, 0.80725193],
                    tex_coords: [0.7441406, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.667364, 0.80725193],
                    tex_coords: [0.7441406, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.667364, 0.77862597],
                    tex_coords: [0.7441406, 0.1796875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.6506276, 0.77862597],
                    tex_coords: [0.7285156, 0.1796875],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.6694561, 0.77862597],
                    tex_coords: [0.6269531, 0.18359375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.6694561, 0.81106865],
                    tex_coords: [0.6269531, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.6882845, 0.81106865],
                    tex_coords: [0.64453125, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.6882845, 0.81106865],
                    tex_coords: [0.64453125, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.6882845, 0.77862597],
                    tex_coords: [0.64453125, 0.18359375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.6694561, 0.77862597],
                    tex_coords: [0.6269531, 0.18359375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.69456065, 0.77862597],
                    tex_coords: [0.7089844, 0.18164063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.69456065, 0.80916035],
                    tex_coords: [0.7089844, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.7008368, 0.80916035],
                    tex_coords: [0.71484375, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.7008368, 0.80916035],
                    tex_coords: [0.71484375, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.7008368, 0.77862597],
                    tex_coords: [0.71484375, 0.18164063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.69456065, 0.77862597],
                    tex_coords: [0.7089844, 0.18164063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.7029289, 0.77862597],
                    tex_coords: [0.609375, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.7029289, 0.8034351],
                    tex_coords: [0.609375, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.7217573, 0.8034351],
                    tex_coords: [0.6269531, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.7217573, 0.8034351],
                    tex_coords: [0.6269531, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.7217573, 0.77862597],
                    tex_coords: [0.6269531, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.7029289, 0.77862597],
                    tex_coords: [0.609375, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.73221755, 0.77862597],
                    tex_coords: [0.24414063, 0.14453125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.73221755, 0.80152667],
                    tex_coords: [0.24414063, 0.12109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.7656903, 0.80152667],
                    tex_coords: [0.27539063, 0.12109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.7656903, 0.80152667],
                    tex_coords: [0.27539063, 0.12109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.7656903, 0.77862597],
                    tex_coords: [0.27539063, 0.14453125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.73221755, 0.77862597],
                    tex_coords: [0.24414063, 0.14453125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.76778245, 0.77862597],
                    tex_coords: [0.71875, 0.18164063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.76778245, 0.80916035],
                    tex_coords: [0.71875, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.7740586, 0.80916035],
                    tex_coords: [0.7246094, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.7740586, 0.80916035],
                    tex_coords: [0.7246094, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.7740586, 0.77862597],
                    tex_coords: [0.7246094, 0.18164063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.76778245, 0.77862597],
                    tex_coords: [0.71875, 0.18164063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.7782427, 0.77862597],
                    tex_coords: [0.89453125, 0.17578125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.7782427, 0.8034351],
                    tex_coords: [0.89453125, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.7991632, 0.8034351],
                    tex_coords: [0.9140625, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.7991632, 0.8034351],
                    tex_coords: [0.9140625, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.7991632, 0.77862597],
                    tex_coords: [0.9140625, 0.17578125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.7782427, 0.77862597],
                    tex_coords: [0.89453125, 0.17578125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.80334723, 0.77862597],
                    tex_coords: [0.6484375, 0.18359375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.80334723, 0.81106865],
                    tex_coords: [0.6484375, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.82426775, 0.81106865],
                    tex_coords: [0.66796875, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.82426775, 0.81106865],
                    tex_coords: [0.66796875, 0.15039063],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.82426775, 0.77862597],
                    tex_coords: [0.66796875, 0.18359375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.80334723, 0.77862597],
                    tex_coords: [0.6484375, 0.18359375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.8284519, 0.77862597],
                    tex_coords: [0.76171875, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.8284519, 0.8034351],
                    tex_coords: [0.76171875, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.8493724, 0.8034351],
                    tex_coords: [0.78125, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.8493724, 0.8034351],
                    tex_coords: [0.78125, 0.0859375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.8493724, 0.77862597],
                    tex_coords: [0.78125, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.8284519, 0.77862597],
                    tex_coords: [0.76171875, 0.111328125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.8514644, 0.77862597],
                    tex_coords: [0.27929688, 0.14453125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.8514644, 0.80152667],
                    tex_coords: [0.27929688, 0.12109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.88284516, 0.80152667],
                    tex_coords: [0.30859375, 0.12109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.88284516, 0.80152667],
                    tex_coords: [0.30859375, 0.12109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.88284516, 0.77862597],
                    tex_coords: [0.30859375, 0.14453125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.8514644, 0.77862597],
                    tex_coords: [0.27929688, 0.14453125],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.8849373, 0.77862597],
                    tex_coords: [0.3125, 0.12890625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.8849373, 0.78625953],
                    tex_coords: [0.3125, 0.12109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.8933054, 0.78625953],
                    tex_coords: [0.3203125, 0.12109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.8933054, 0.78625953],
                    tex_coords: [0.3203125, 0.12109375],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.8933054, 0.77862597],
                    tex_coords: [0.3203125, 0.12890625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
                GlyphVertex {
                    position: [0.8849373, 0.77862597],
                    tex_coords: [0.3125, 0.12890625],
                    fg: [0.0, 0.0, 0.0, 1.0],
                    bg: [0.0, 0.0, 0.0, 1.0],
                },
            ];

            let len = buffer.len();
            let buffer = self.device.create_buffer_init(&BufferInitDescriptor {
                label: Some("render buffer"),
                contents: bytemuck::cast_slice(&buffer),
                usage: wgpu::BufferUsages::VERTEX,
            });
            render_pass.set_pipeline(&self.pipe_line);
            render_pass.set_bind_group(0, &atlas_linear, &[]);
            // render_pass.set_bind_group(1, &atlas_linear, &[]);
            // render_pass.set_bind_group(2, &atlas_nearest, &[]);
            render_pass.set_vertex_buffer(0, buffer.slice(..));
            render_pass.draw(0..len as u32, 0..1);
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
        println!("{:?}", buffer);
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
                    // println!("rendered");
                }
                Err(e) => {
                    // println!("error: {e}");
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
