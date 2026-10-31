use anyhow::Context;
use sdl3::event::Event;
use sdl3::keyboard::Keycode;
use wgpu::rwh::{HasDisplayHandle, HasWindowHandle};

mod shader;

const KB: u64 = 1024;
const MB: u64 = 1024 * KB;

async fn go() -> anyhow::Result<()> {
    let sdl = sdl3::init()?;
    let video = sdl.video()?;

    let window = video
        .window("rust-sdl3 demo", 800, 600)
        .position_centered()
        .build()
        .unwrap();

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN | wgpu::Backends::METAL | wgpu::Backends::DX12,
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

    surface.configure(
        &device,
        &wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            color_space: wgpu::SurfaceColorSpace::Srgb,
            width: window.size_in_pixels().0,
            height: window.size_in_pixels().1,
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: Vec::new(),
        },
    );

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

                _ => {}
            }
        }

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
