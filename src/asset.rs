use std::collections::VecDeque;
use std::path::Path;

use anyhow::Context;
use glam::{Mat4, Vec3};

use crate::Vertex;

#[derive(Default)]
pub struct Model {
    pub nodes: Vec<ModelNode>,
}

#[derive(Default)]
pub struct ModelNode {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub model_from_node: Mat4,
}

impl Model {
    pub fn load(name: &str) -> anyhow::Result<Self> {
        let base_path = Path::new("assets");
        let path = base_path.join(format!("{name}.glb"));
        let (document, buffers, _images) = gltf::import(&path)?;

        let scene = document.default_scene().context("no default scene")?;

        let mut model = Model::default();

        let mut to_visit = VecDeque::new();

        for node in scene.nodes() {
            to_visit.push_back((node, Mat4::IDENTITY));
        }

        while let Some((gltf_node, node_to_root)) = to_visit.pop_front() {
            let children_to_node = Mat4::from_cols_array_2d(&gltf_node.transform().matrix());
            let children_to_root = children_to_node * node_to_root;

            if let Some(mesh) = gltf_node.mesh() {
                for primitive in mesh.primitives() {
                    let mut node = ModelNode {
                        model_from_node: children_to_root,
                        ..Default::default()
                    };

                    let reader =
                        primitive.reader(|buffer| buffers.get(buffer.index()).map(|v| &**v));

                    let positions = reader.read_positions().unwrap();
                    let normals = reader.read_normals().unwrap();

                    let vertex_iter = positions
                        .zip(normals)
                        .map(|(pos, normal)| Vertex::new(Vec3::from(pos), Vec3::from(normal)));

                    node.vertices.extend(vertex_iter);

                    let Some(indices) = reader.read_indices() else {
                        continue;
                    };

                    node.indices.extend(indices.into_u32());

                    // let material = primitive.material();

                    model.nodes.push(node);
                }
            }

            for child in gltf_node.children() {
                to_visit.push_back((child, children_to_root));
            }
        }

        Ok(model)
    }
}
