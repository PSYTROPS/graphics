use nalgebra as na;
use nalgebra::geometry as na_geo;

#[repr(C)]
#[derive(Copy, Clone, Default)]
pub struct Vertex {
    pub pos: na::Vector3<f32>,
    pub normal: na::Vector3<f32>,
    pub tex: na::Vector2<f32>
}

#[derive(Clone)]
pub struct Primitive {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
    pub material: u32
}

#[derive(Clone)]
pub struct Mesh {
    pub primitives: Vec<Primitive>
}

#[derive(Clone)]
pub struct Node {
    pub mesh: Option<u32>,
    pub children: Vec<u32>,
    pub translation: na_geo::Translation3<f32>,
    pub rotation: na_geo::Rotation3<f32>,
    pub scale: na_geo::Scale3<f32>
}

#[repr(C, align(16))]
#[derive(Copy, Clone, Default)]
pub struct Material {
    pub color: na::Vector4<f32>,
    pub color_texture: u32,
    pub metal_rough_texture: u32,
    pub metal_factor: f32,
    pub rough_factor: f32
}

#[repr(C, align(16))]
#[derive(Copy, Clone, Default)]
pub struct PointLight {
    pub pos: na::Vector4<f32>,
    pub color: na::Vector4<f32>,
    pub intensity: f32,
    pub range: f32
}

#[derive(Clone)]
pub struct Scene {
    pub nodes: Vec<Node>,
    pub meshes: Vec<Mesh>,
    pub materials: Vec<Material>,
    pub textures: Vec<image::RgbaImage> //TODO: Custom image format
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
    pub fn transformations(&self) -> Vec<na_geo::Affine3<f32>> {
        //Find root nodes
        let mut root_mask = vec![true; self.nodes.len()];
        for node in &self.nodes {
            for child in &node.children {
                root_mask[*child as usize] = false;
            }
        }
        //Traverse scene graph
        let mut stack: Vec<(usize, na_geo::Affine3<f32>)> = root_mask.iter().enumerate().filter_map(
            |(i, b)| if *b {
                Some((i, self.nodes[i].matrix()))
            } else {None}
        ).collect();
        let mut result = vec![na_geo::Affine3::<f32>::identity(); self.nodes.len()];
        while !stack.is_empty() {
            let (node, t) = stack.pop().unwrap();
            result[node] = t;
            for child in &self.nodes[node].children {
                stack.push((
                    *child as usize,
                    t * self.nodes[*child as usize].matrix()
                ));
            }
        }
        result
    }

    pub fn load_gltf<P: AsRef<std::path::Path>>(path: P) -> gltf::Result<Self> {
        let (document, buffers, images) = gltf::import(path)?;
        //Nodes
        let nodes: Vec<Node> = document.nodes().map(|node| {
            let (translation, rotation, scale) = node.transform().decomposed();
            Node {
                mesh: match node.mesh() {
                    Some(m) => Some(m.index() as u32),
                    None => None
                },
                children: node.children().map(|c| c.index() as u32).collect(),
                translation: translation.into(),
                rotation: na::UnitQuaternion::from_quaternion(
                    na::Quaternion::<f32>::from(rotation)
                ).into(),
                scale: scale.into()
            }
        }).collect();
        //Meshes
        let meshes: Vec<Mesh> = document.meshes().map(|mesh| {
            let primitives: Vec<Primitive> = mesh.primitives().map(|primitive| {
                let mut indices = Vec::<u16>::new();
                //Indices
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
                        let value = match accessor.size() {
                            size @ 2 => u16::from_le_bytes(
                                data[offset..offset + size].try_into().unwrap()
                            ),
                            size @ 4 => u32::from_le_bytes(
                                data[offset..offset + size].try_into().unwrap()
                            ) as u16,
                            _ => panic!("Unknown index type size")
                        };
                        indices.push(value);
                    }
                }
                //Vertex attributes
                //TODO: Additional texture coordinates
                //TODO: Fill missing attributes
                let mut positions = Vec::<na::Vector3<f32>>::new();
                let mut normals = Vec::<na::Vector3<f32>>::new();
                let mut texcoords = Vec::<na::Vector2<f32>>::new();
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
                                positions.push(element);
                            }
                        },
                        gltf::Semantic::Normals => {
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
                                normals.push(element);
                            }
                        },
                        gltf::Semantic::TexCoords(_) => {
                            let view = accessor.view().unwrap();
                            let buffer = view.buffer();
                            let data = &buffers[buffer.index()];
                            let offset = view.offset() + accessor.offset();
                            let stride = match view.stride() {
                                Some(s) => s,
                                None => accessor.size()
                            };
                            for i in 0..accessor.count() {
                                let mut element = na::Vector2::<f32>::zeros();
                                let offset = offset + i * stride;
                                for j in 0..std::cmp::max(accessor.dimensions().multiplicity(), 2) {
                                    let scalar_size = accessor.data_type().size();
                                    let offset = offset + j * scalar_size;
                                    element[j] = f32::from_le_bytes(
                                        data[offset..offset + scalar_size].try_into().unwrap()
                                    );
                                }
                                texcoords.push(element);
                            }
                        }
                        _ => ()
                    }
                }
                //Fill missing attributes
                if normals.len() < positions.len() {
                    normals = std::iter::repeat(na::Vector3::<f32>::zeros()).take(positions.len()).collect();
                }
                if texcoords.len() < positions.len() {
                    texcoords = std::iter::repeat(na::Vector2::<f32>::zeros()).take(positions.len()).collect();
                }
                //Create vertices
                let vertices: Vec<Vertex> = (0..positions.len()).map(
                    |i| Vertex {
                        pos: positions[i],
                        normal: normals[i],
                        tex: texcoords[i],
                    }
                ).collect();
                //Material
                let material = match primitive.material().index() {
                    Some(x) => x as u32 + 1,
                    None => 0
                };
                Primitive {vertices, indices, material}
            }).collect();
            Mesh {primitives}
        }).collect();
        //Materials
        let default_material = Material {
            color: na::Vector4::<f32>::new(1.0, 0.0, 0.0, 1.0),
            color_texture: 0,
            metal_rough_texture: 0,
            metal_factor: 0.0,
            rough_factor: 0.0
        };
        let mut materials = vec![default_material];
        materials.append(&mut document.materials().map(|material| {
            let pbr = material.pbr_metallic_roughness();
            Material {
                color: pbr.base_color_factor().into(),
                color_texture: match pbr.base_color_texture() {
                    Some(info) => info.texture().index() + 1,
                    None => 0
                } as u32,
                metal_rough_texture: match pbr.metallic_roughness_texture() {
                    Some(info) => info.texture().index() + 1,
                    None => 0
                } as u32,
                metal_factor: pbr.metallic_factor(),
                rough_factor: pbr.roughness_factor()
            }
        }).collect());
        //Textures
        let default_texture = image::RgbaImage::from_pixel(
            1, 1, image::Rgba([255, 255, 255, 255])
        );
        let mut textures = vec![default_texture];
        textures.append(&mut document.textures().map(|texture| {
            /*
                GLTF imports texture images using the `image` library;
                the reported image format maps directly to the original `image` format types.
            */
            let image = &images[texture.source().index()];
            match image.format {
                gltf::image::Format::R8G8B8 => image::DynamicImage::ImageRgb8(
                    image::RgbImage::from_raw(
                        image.width,
                        image.height,
                        image.pixels.clone()
                    ).unwrap()
                ).into_rgba8(),
                gltf::image::Format::R8G8B8A8 => image::RgbaImage::from_raw(
                    image.width,
                    image.height,
                    image.pixels.clone()
                ).unwrap(),
                _ => panic!("Unsupported image format")
            }
        }).collect());
        Ok(Self {nodes, meshes, materials, textures})
    }
}
