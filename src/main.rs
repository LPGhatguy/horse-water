use std::borrow::Cow;
use std::f32::consts::TAU;
use std::num::NonZeroU64;
use std::time::Instant;

use anyhow::Context;
use bytemuck::{Pod, Zeroable, bytes_of, cast_slice};
use glam::{EulerRot, Mat4, Quat, UVec2, Vec3};
use hecs::World;
use rand::RngExt;
use sdl3::event::Event;
use sdl3::keyboard::Keycode;
use wgpu::rwh::{HasDisplayHandle, HasWindowHandle};
use wgpu::util::DeviceExt;

use self::asset::Model;
use self::blast_buffer::BlastBuffer;
use self::shader::Shaders;

const SHADER_DEBUGGING: bool = true;

const TICK_RATE: f32 = 1.0 / 60.0;

const MODELS: &[&str] = &["monkey", "text"];
#[expect(unused)]
const MODEL_MONKEY: usize = pos(MODELS, "monkey");
#[expect(unused)]
const MODEL_TEXT: usize = pos(MODELS, "text");

mod asset;
mod blast_buffer;
mod bumpfer;
mod shader;

const KB: u64 = 1024;
const MB: u64 = 1024 * KB;

#[derive(Clone, Copy, Zeroable, Pod)]
#[repr(C)]
struct Vertex {
    pos: Vec3,
    normal: Vec3,
}

impl Vertex {
    fn new(pos: Vec3, normal: Vec3) -> Self {
        Self { pos, normal }
    }
}

#[derive(Default)]
struct ModelInfo {
    nodes: Vec<ModelNodeInfo>,
}

struct ModelNodeInfo {
    first_index: u32,
    index_count: u32,
    base_vertex: i32,
    model_from_node: Mat4,
}

#[derive(Clone, Copy, Zeroable, Pod)]
#[repr(C)]
struct GpuModelNode {
    world_from_node: Mat4,
}

#[derive(Clone, Copy, Zeroable, Pod)]
#[repr(C)]
struct GpuPerView {
    world_to_ndc: Mat4,
    camera_in_world: Vec3,
    _pad0: f32,
}

// component
struct Transform {
    pos: Vec3,
    rot: Quat,
}

// component
struct ModelFromFile {
    id: usize,
}

fn per_view(_t: f32) -> GpuPerView {
    let camera_in_world = Vec3::new(-10.0, 2.0, -10.0);

    let camera_from_world =
        glam::camera::rh::view::look_at_mat4(camera_in_world, Vec3::ZERO, Vec3::Y);
    let ndc_from_camera =
        glam::camera::rh::proj::directx::perspective_infinite_reverse(70.0, 800.0 / 600.0, 0.01);

    let world_to_ndc = ndc_from_camera * camera_from_world;

    GpuPerView {
        world_to_ndc,
        camera_in_world,
        _pad0: f32::MAX,
    }
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
        | wgpu::Features::INDIRECT_FIRST_INSTANCE
        | wgpu::Features::PASSTHROUGH_SHADERS;

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
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::all(),
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(size_of::<GpuPerView>() as u64),
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
    let module = if SHADER_DEBUGGING {
        unsafe {
            device.create_shader_module_passthrough(wgpu::ShaderModuleDescriptorPassthrough {
                label: Some("main"),
                entry_points: Cow::Borrowed(&[
                    wgpu::PassthroughShaderEntryPoint {
                        name: "vs_main".into(),
                        workgroup_size: (1, 1, 1),
                    },
                    wgpu::PassthroughShaderEntryPoint {
                        name: "fs_main".into(),
                        workgroup_size: (1, 1, 1),
                    },
                ]),
                spirv: Some(Cow::Owned(main_source)),
                ..Default::default()
            })
        }
    } else {
        unsafe {
            device.create_shader_module_trusted(
                wgpu::ShaderModuleDescriptor {
                    label: Some("main!?"),
                    source: wgpu::ShaderSource::SpirV(Cow::Owned(main_source)),
                },
                wgpu::ShaderRuntimeChecks::unchecked(),
            )
        }
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
            depth_compare: Some(wgpu::CompareFunction::Greater),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });

    let mut all_the_vertices = Vec::new();
    let mut all_the_indices = Vec::new();
    let mut draw_calls = Vec::new();
    let mut gpu_model_nodes = Vec::new();
    let mut model_infos = Vec::new();

    for model_name in MODELS {
        let model = Model::load(model_name)?;

        let mut model_info = ModelInfo::default();

        for node in model.nodes {
            let base_vertex = all_the_vertices.len() as i32;
            // let vertex_len = node.vertices.len() as u32;
            all_the_vertices.extend_from_slice(&node.vertices);

            let first_index = all_the_indices.len() as u32;
            let index_count = node.indices.len() as u32;
            all_the_indices.extend_from_slice(&node.indices);

            let first_instance = gpu_model_nodes.len() as u32;

            model_info.nodes.push(ModelNodeInfo {
                first_index,
                index_count,
                base_vertex,
                model_from_node: node.model_from_node,
            });

            draw_calls.push(wgpu::util::DrawIndexedIndirectArgs {
                first_index,
                index_count,
                base_vertex,
                first_instance,
                instance_count: 1,
            });

            gpu_model_nodes.push(GpuModelNode {
                world_from_node: node.model_from_node,
            });
        }

        model_infos.push(model_info);
    }

    let mut rng = rand::rng();
    let mut world = World::new();

    for x in 0..64 {
        for y in 0..64 {
            for z in -8..8 {
                let x = x as f32;
                let y = y as f32;
                let z = z as f32;

                let model_id = rng.random_range(0..MODELS.len());

                world.spawn((
                    Transform {
                        pos: Vec3::new(x * 2.5, z * 2.5, y * 2.5),
                        rot: Quat::from_euler(EulerRot::XYZ, x / (5.0 * TAU), y / (5.0 * TAU), 0.0),
                    },
                    ModelFromFile { id: model_id },
                ));
            }
        }
    }

    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("vertices"),
        contents: cast_slice(all_the_vertices.as_slice()),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    });

    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("indices"),
        contents: cast_slice(all_the_indices.as_slice()),
        usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
    });

    let per_view_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("per view"),
        contents: bytes_of(&per_view(0.0)),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let mut model_node_buffer =
        BlastBuffer::new(device.clone(), queue.clone(), wgpu::BufferUsages::STORAGE);

    let mut indirect_buffer =
        BlastBuffer::new(device.clone(), queue.clone(), wgpu::BufferUsages::INDIRECT);

    let mut count_buffer =
        BlastBuffer::new(device.clone(), queue.clone(), wgpu::BufferUsages::INDIRECT);

    let mut event_pump = sdl.event_pump()?;
    let start = Instant::now();
    let mut last = start;
    let mut accum = 0.0;

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

                _ => {}
            }
        }

        let now = Instant::now();
        let t = now.duration_since(start).as_secs_f32();
        accum += now.duration_since(last).as_secs_f32();
        last = now;

        while accum >= TICK_RATE {
            accum -= TICK_RATE;
            systems(&mut world, TICK_RATE);
        }

        let mut num_calls = 0;

        {
            profiling::scope!("gather draw calls");

            let mut query = world.query::<(&Transform, &ModelFromFile)>();
            for (world_from_model, model) in query.iter() {
                let world_from_model =
                    Mat4::from_rotation_translation(world_from_model.rot, world_from_model.pos);

                let model_info = &model_infos[model.id];
                for node in &model_info.nodes {
                    indirect_buffer.extend(bytes_of(&wgpu::util::DrawIndexedIndirectArgs {
                        index_count: node.index_count,
                        instance_count: 1,
                        first_index: node.first_index,
                        base_vertex: node.base_vertex,
                        first_instance: num_calls,
                    }));

                    let world_from_node = world_from_model * node.model_from_node;

                    model_node_buffer.extend(bytes_of(&GpuModelNode { world_from_node }));

                    num_calls += 1;
                }
            }
        }

        indirect_buffer.extend(cast_slice(&draw_calls));
        let indirect_buffer = indirect_buffer.upload();

        count_buffer.extend(&u32::to_ne_bytes(num_calls));
        let count_buffer = count_buffer.upload();

        let model_node_buffer = model_node_buffer.upload();

        queue.write_buffer(&per_view_buffer, 0, bytes_of(&per_view(t)));

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
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: per_view_buffer.as_entire_binding(),
                },
            ],
        });

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
                        load: wgpu::LoadOp::Clear(0.0),
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
            pass.multi_draw_indexed_indirect_count(
                &indirect_buffer,
                0,
                &count_buffer,
                0,
                num_calls,
            );
        }

        queue.submit([encoder.finish()]);
        queue.present(surface_texture);
        profiling::finish_frame!();

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

#[profiling::function]
fn systems(world: &mut World, dt: f32) {
    let mut query = world.query::<&mut Transform>();

    for transform in query.iter() {
        let scale = transform.pos.length() / 20.0;
        transform.rot *= Quat::from_rotation_y(dt * scale);
    }
}

const fn pos(slice: &[&str], needle: &str) -> usize {
    let mut i = 0;
    while i < slice.len() {
        if slice[i].eq_ignore_ascii_case(needle) {
            return i;
        }
        i += 1;
    }

    panic!("not found");
}
