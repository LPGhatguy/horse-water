#![expect(unused)]

use std::marker::PhantomData;
use std::num::NonZero;

use bytemuck::Pod;

/// bump buffer
pub struct Bumpfer<T> {
    device: wgpu::Device,
    queue: wgpu::Queue,
    usage: wgpu::BufferUsages,
    occupied: u64,
    buffer: Option<wgpu::Buffer>,
    _marker: PhantomData<*const T>,
}

impl<T> Bumpfer<T>
where
    T: Pod,
{
    pub fn new(device: wgpu::Device, queue: wgpu::Queue, usage: wgpu::BufferUsages) -> Self {
        Self {
            device,
            queue,
            usage: usage | wgpu::BufferUsages::COPY_DST,
            occupied: 0,
            buffer: None,
            _marker: PhantomData,
        }
    }

    pub fn extend(&mut self, values: &[T]) {
        let bytes: &[u8] = bytemuck::cast_slice(values);
        assert!(!bytes.is_empty());
        let len = NonZero::new(bytes.len() as u64).unwrap();

        let new_size = self.occupied + bytes.len() as u64;

        let buffer = self.maybe_resize(new_size);
        let mut view = self
            .queue
            .write_buffer_with(&buffer, self.occupied, len)
            .unwrap();

        view.copy_from_slice(bytes);
    }

    pub fn buffer(&self) -> Option<&wgpu::Buffer> {
        self.buffer.as_ref()
    }

    fn maybe_resize(&mut self, new_size: u64) -> wgpu::Buffer {
        if let Some(buffer) = &self.buffer
            && buffer.size() >= new_size
        {
            return buffer.clone();
        }

        let new_size = powify(new_size);

        let new_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: new_size,
            usage: self.usage,
            mapped_at_creation: false,
        });

        if let Some(old_buffer) = self.buffer.take() {
            let mut commands = self.device.create_command_encoder(&Default::default());
            commands.copy_buffer_to_buffer(&old_buffer, 0, &new_buffer, 0, old_buffer.size());

            self.queue.submit([commands.finish()]);
        }

        self.buffer.insert(new_buffer).clone()
    }
}

fn powify(value: u64) -> u64 {
    if value.is_power_of_two() {
        value
    } else {
        value.next_power_of_two()
    }
}
