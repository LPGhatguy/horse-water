use std::borrow::Cow;

use anyhow::Context;
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, UVec2, Vec3};
use sdl3::event::Event;
use sdl3::keyboard::Keycode;
use wgpu::rwh::{HasDisplayHandle, HasWindowHandle};
use wgpu::util::DeviceExt;

use self::asset::Model;
use self::shader::Shaders;

mod asset;
mod shader;

const KB: u64 = 1024;
const MB: u64 = 1024 * KB;

#[derive(Clone, Copy, Zeroable, Pod)]
#[repr(C)]
struct Vertex {
    pos: Vec3,
    _pad0: f32,
}

impl Vertex {
    fn new(x: f32, y: f32, z: f32) -> Self {
        Self {
            pos: Vec3::new(x, y, z),
            _pad0: f32::MAX,
        }
    }
}

#[derive(Clone, Copy, Zeroable, Pod)]
#[repr(C)]
struct GpuModelNode {
    node_to_world: Mat4,
}

async fn go() -> anyhow::Result<()> {
    let sdl = sdl3::init()?;
    let video = sdl.video()?;

    let window = video
        .window("playground", 800, 600)
        .position_centered()
        .build()
        .unwrap();

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        flags: wgpu::InstanceFlags::DEBUG,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });

    let surface = unsafe {
        instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(window.display_handle().unwrap().as_raw()),
            raw_window_handle: window.window_handle().unwrap().as_raw(),
        })?
    };

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
            apply_limit_buckets: false,
        })
        .await?;

    let required_features = wgpu::Features::ADDRESS_MODE_CLAMP_TO_BORDER
        | wgpu::Features::TEXTURE_BINDING_ARRAY
        | wgpu::Features::IMMEDIATES
        | wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING
        | wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES
        | wgpu::Features::TEXTURE_COMPRESSION_BC
        | wgpu::Features::DEPTH_CLIP_CONTROL
        | wgpu::Features::PARTIALLY_BOUND_BINDING_ARRAY
        | wgpu::Features::FLOAT32_FILTERABLE
        | wgpu::Features::MULTI_DRAW_INDIRECT_COUNT
        | wgpu::Features::INDIRECT_FIRST_INSTANCE;

    let required_limits = wgpu::Limits {
        max_sampled_textures_per_shader_stage: 32,
        max_immediate_size: 128,
        max_buffer_size: 512 * MB,
        max_storage_buffer_binding_size: 512 * MB,
        max_binding_array_elements_per_shader_stage: 4096,
        max_binding_array_sampler_elements_per_shader_stage: 32,
        ..Default::default()
    };

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("Main Device"),
            required_features,
            required_limits,
            ..Default::default()
        })
        .await
        .context("failed to create wgpu device")?;

    let mut shaders = Shaders::new();

    let size = UVec2::from(window.size_in_pixels());

    surface.configure(
        &device,
        &wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            color_space: wgpu::SurfaceColorSpace::Srgb,
            width: size.x,
            height: size.y,
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: Vec::new(),
        },
    );

    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Depth"),
        size: wgpu::Extent3d {
            width: size.x,
            height: size.y,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });

    let depth_view = depth.create_view(&Default::default());

    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("maaaaaiiinnn"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::all(),
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::all(),
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });

    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("...main"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });

    let main_source = shaders.get("main").await?;
    let module = unsafe {
        device.create_shader_module_trusted(
            wgpu::ShaderModuleDescriptor {
                label: Some("main!?"),
                source: wgpu::ShaderSource::SpirV(Cow::Owned(main_source)),
            },
            wgpu::ShaderRuntimeChecks::unchecked(),
        )
    };

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("main!"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Bgra8UnormSrgb,
                blend: None,
                write_mask: wgpu::ColorWrites::all(),
            })],
        }),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });

    let monkey = Model::load("monkey")?;

    let mut all_the_vertices = Vec::new();
    let mut all_the_indices = Vec::new();
    let mut draw_calls = Vec::new();
    let mut gpu_model_nodes = Vec::new();

    for node in monkey.nodes {
        let vertex_offset = all_the_vertices.len() as i32;
        // let vertex_len = node.vertices.len() as u32;
        all_the_vertices.extend_from_slice(&node.vertices);

        let index_offset = all_the_indices.len() as u32;
        let index_len = node.indices.len() as u32;
        all_the_indices.extend_from_slice(&node.indices);

        let first_instance = gpu_model_nodes.len() as u32;

        draw_calls.push(wgpu::util::DrawIndexedIndirectArgs {
            first_index: index_offset,
            index_count: index_len,
            base_vertex: vertex_offset,
            first_instance,
            instance_count: 1,
        });

        gpu_model_nodes.push(GpuModelNode {
            node_to_world: node.node_to_model,
        });
    }

    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("vertices"),
        contents: bytemuck::cast_slice(all_the_vertices.as_slice()),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    });

    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("indices"),
        contents: bytemuck::cast_slice(all_the_indices.as_slice()),
        usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
    });

    let model_node_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("nodes"),
        contents: bytemuck::cast_slice(gpu_model_nodes.as_slice()),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("...brains"),
        layout: &bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: vertex_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: model_node_buffer.as_entire_binding(),
            },
        ],
    });

    let indirect_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("indirect"),
        contents: bytemuck::cast_slice(draw_calls.as_slice()),
        usage: wgpu::BufferUsages::INDIRECT | wgpu::BufferUsages::COPY_DST,
    });

    let count_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("indirect count"),
        contents: bytemuck::bytes_of(&[draw_calls.len() as u32]),
        usage: wgpu::BufferUsages::INDIRECT | wgpu::BufferUsages::COPY_DST,
    });

    let mut write_count: u32 = 1;

    let mut event_pump = sdl.event_pump()?;

    'outer: loop {
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => {
                    break 'outer;
                }

                Event::KeyDown { keycode, .. } => {
                    if keycode == Some(Keycode::W) {
                        write_count = 1;
                    } else if keycode == Some(Keycode::S) {
                        write_count = 0;
                    }
                }

                _ => {}
            }
        }

        queue.write_buffer(&count_buffer, 0, bytemuck::bytes_of(&[write_count]));

        let surface_texture = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(surface_texture) => surface_texture,
            wgpu::CurrentSurfaceTexture::Suboptimal(surface_texture) => {
                drop(surface_texture);
                panic!("not done yet");
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                panic!("not done yet");
            }
            _ => continue,
        };

        let surface_view = surface_texture.texture.create_view(&Default::default());

        let mut encoder = device.create_command_encoder(&Default::default());

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("booomain"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &surface_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.multi_draw_indexed_indirect_count(&indirect_buffer, 0, &count_buffer, 0, 1);
        }

        queue.submit([encoder.finish()]);
        queue.present(surface_texture);

        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    if let Err(err) = go().await {
        eprintln!("{err:?}");
        std::process::exit(1);
    }
}
