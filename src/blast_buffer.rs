// reupload that buffer every single frame
pub struct BlastBuffer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    usage: wgpu::BufferUsages,
    staging: Vec<u8>,
    buffer: wgpu::Buffer,
}

impl BlastBuffer {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue, usage: wgpu::BufferUsages) -> Self {
        let usage = usage | wgpu::BufferUsages::COPY_DST;

        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 4096, // totally random
            usage,
            mapped_at_creation: false,
        });

        Self {
            staging: Vec::new(),
            buffer,
            usage,
            device,
            queue,
        }
    }

    pub fn extend(&mut self, bytes: &[u8]) {
        self.staging.extend(bytes);
    }

    pub fn upload(&mut self) -> wgpu::Buffer {
        let buffer = self.maybe_resize();
        self.queue.write_buffer(&buffer, 0, &self.staging);
        self.staging.clear();

        buffer
    }

    fn maybe_resize(&mut self) -> wgpu::Buffer {
        if (self.staging.len() as u64) < self.buffer.size() {
            return self.buffer.clone();
        }

        let new_size = powify(self.staging.len() as u64);

        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: new_size,
            usage: self.usage,
            mapped_at_creation: false,
        });

        self.buffer = buffer.clone();
        buffer
    }
}

fn powify(value: u64) -> u64 {
    if value.is_power_of_two() {
        value
    } else {
        value.next_power_of_two()
    }
}
