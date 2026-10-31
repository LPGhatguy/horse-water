use std::process::Stdio;
use std::ptr;

use anyhow::{Context, bail};
use tokio::process::Command;

pub struct Shaders {}

impl Shaders {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn get(&mut self, name: &str) -> anyhow::Result<Vec<u32>> {
        let base_dir = "shaders";
        let input_path = format!("{base_dir}/{name}.hlsl");
        let dir = tempfile::tempdir()?;
        let output_path = dir.path().join("output.spv");

        let mut command = Command::new("dxc");

        let base_args = ["-T", "lib_6_5", "-spirv"];

        command
            .arg("-I")
            .arg(base_dir)
            .arg(input_path)
            .args(base_args);

        // for define in defines {
        //     command.arg("-D").arg(define.as_ref());
        // }

        command.arg("-Fo").arg(&output_path);

        // if emit_debug_info {
        //     command.args(["-Zi", "-fspv-debug=vulkan-with-source"]);
        // }

        command
            .kill_on_drop(true)
            .stdout(Stdio::piped())
            .stdin(Stdio::null())
            .stderr(Stdio::piped());

        let child = command.spawn().context("failed to run the 'dxc' command")?;

        let output = child.wait_with_output().await?;

        let stderr = String::from_utf8_lossy(&output.stderr);

        if !output.status.success() {
            bail!("failed to compile DXC shader: {stderr}");
        }

        let data = tokio::fs::read(&output_path).await?;

        // https://docs.rs/wgpu/22.1.0/src/wgpu/util/mod.rs.html#60-64
        let mut words = vec![0u32; data.len() / size_of::<u32>()];
        unsafe {
            ptr::copy_nonoverlapping(data.as_ptr(), words.as_mut_ptr() as *mut u8, data.len());
        }

        Ok(words)
    }
}
