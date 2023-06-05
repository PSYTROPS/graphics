use nalgebra as na;
use nalgebra::geometry as na_geo;

/*
    Coordinate systems:
    * Worldspace: Right-handed with Z-axis up
    * Viewspace: Right-handed with Y-axis down, looking into +Z axis
    * Clipspace: Defined by Vulkan.
        X & Y axes have range [-1, 1] where (-1, -1) is the upper-left corner of the screen.
        Depth buffer (Z-axis) has range [0, 1].
*/

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Vertex {
    pub pos: na::Vector3<f32>,
    pub color: na::Vector3<f32>
}

pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>
}

pub struct Node {
    pub mesh: u32,
    pub children: Vec<u32>,
    pub translation: na_geo::Translation3<f32>,
    pub rotation: na_geo::Rotation3<f32>,
    pub scale: na_geo::Scale3<f32>
}

pub struct Scene {
    pub nodes: Vec<Node>,
    pub meshes: Vec<Mesh>
}

impl Node {
    pub fn matrix(&self) -> na_geo::Affine3<f32> {
        na_geo::Affine3::<f32>::from_matrix_unchecked(
            self.translation.to_homogeneous()
            * self.rotation.to_homogeneous()
            * self.scale.to_homogeneous()
        )
    }
}

impl Scene {
    pub fn load_gltf<P: AsRef<std::path::Path>>(path: P) -> gltf::Result<Self> {
        let (document, buffers, _images) = gltf::import(path)?;
        //Load nodes
        let mut nodes = Vec::<Node>::new();
        for node in document.nodes() {
            if let Some(mesh) = node.mesh() {
                let children = node.children().map(|c| c.index() as u32).collect();
                let (translation, rotation, scale) = node.transform().decomposed();
                let rotation = na::Rotation3::from(na::UnitQuaternion::from_quaternion(rotation.into()));
                let euler = rotation.euler_angles();
                nodes.push(Node {
                    mesh: mesh.index() as u32,
                    children,
                    translation: na::Translation3::<f32>::from([
                        translation[0],
                        translation[2],
                        translation[1]
                    ]),
                    rotation: na::Rotation3::<f32>::from_euler_angles(
                        euler.0,
                        euler.2,
                        euler.1
                    ),
                    scale: na::Scale3::<f32>::from([
                        scale[0],
                        scale[2],
                        scale[1]
                    ])
                });
            }
        }
        //Load meshes
        let mut meshes = Vec::<Mesh>::new();
        for mesh in document.meshes() {
            let mut vertices = Vec::<Vertex>::new();
            let mut indices = Vec::<u16>::new();
            for primitive in mesh.primitives() {
                //Read indices
                let mut local_indices = Vec::<u16>::new();
                if let Some(accessor) = primitive.indices() {
                    let view = accessor.view().unwrap();
                    let buffer = view.buffer();
                    let data = &buffers[buffer.index()];
                    let offset = view.offset() + accessor.offset();
                    let stride = match view.stride() {
                        Some(s) => s,
                        None => accessor.size()
                    };
                    for i in 0..accessor.count() {
                        let offset = offset + i * stride;
                        let value = u16::from_le_bytes(
                            data[offset..offset + accessor.size()].try_into().unwrap()
                        );
                        local_indices.push(value + vertices.len() as u16);
                    }
                }
                indices.append(&mut local_indices);
                //Read vertex attributes
                let mut positions = Vec::<na::Vector3<f32>>::new();
                for (semantic, accessor) in primitive.attributes() {
                    match semantic {
                        gltf::Semantic::Positions => {
                            let view = accessor.view().unwrap();
                            let buffer = view.buffer();
                            let data = &buffers[buffer.index()];
                            let offset = view.offset() + accessor.offset();
                            let stride = match view.stride() {
                                Some(s) => s,
                                None => accessor.size()
                            };
                            for i in 0..accessor.count() {
                                let mut element = na::Vector3::<f32>::zeros();
                                let offset = offset + i * stride;
                                for j in 0..std::cmp::max(accessor.dimensions().multiplicity(), 3) {
                                    let scalar_size = accessor.data_type().size();
                                    let offset = offset + j * scalar_size;
                                    element[j] = f32::from_le_bytes(
                                        data[offset..offset + scalar_size].try_into().unwrap()
                                    );
                                }
                                positions.push(element.xzy());
                            }
                        }
                        _ => ()
                    }
                }
                //Create vertices
                for position in positions {
                    vertices.push(Vertex {
                        pos: position,
                        color: na::Vector3::<f32>::x()
                    })
                }
            }
            meshes.push(Mesh {vertices, indices});
        }
        Ok(Self {nodes, meshes})
    }
}
