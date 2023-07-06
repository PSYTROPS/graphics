use ash::vk;
use nalgebra as na;

use super::{FRAME_COUNT, MAX_TEXTURES};
use super::base::Base;
use super::scene::{Vertex, Material, Scene};
use super::transfer::transaction::Transaction;
use std::rc::Rc;

//Device-local structures must obey GLSL std430 layout alignment rules

#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct DeviceMesh {
    pub lower_bounds: na::Vector4<f32>,
    pub upper_bounds: na::Vector4<f32>,
    pub material: u32,
}

#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct DeviceNode {
    pub transform: na::Matrix4<f32>,
    pub inverse_transform: na::Matrix4<f32>,
    pub mesh: u32,
    pub flags: u32 //LSB is visibility
}

pub struct DeviceScene {
    base: Rc<Base>,
    //Dynamic data
    pub nodes: Vec<DeviceNode>,
    pub mesh_offsets: Vec<usize>,
    //Buffers
    /*
        Buffers:
        0. Vertices
        1. Indices
        2. Meshes
        3. Materials
        4. Mesh draw commands
        5. Nodes (duplicated)
        6. Draw commands (duplicated)
        7. Draw extras [node, primitive] (duplicated)
        8. Draw command count (duplicated)
    */
    pub buffers: [vk::Buffer; 9],
    pub buffer_alloc: vk::DeviceMemory,
    pub buffer_sizes: [usize; 9],
    pub buffer_descriptors: [vk::DescriptorBufferInfo; 3 + 4 * FRAME_COUNT],
    //Images
    pub images: Vec<vk::Image>,
    pub image_views: Vec<vk::ImageView>,
    pub image_alloc: vk::DeviceMemory,
    pub image_descriptors: [vk::DescriptorImageInfo; MAX_TEXTURES]
}

impl DeviceScene {
    pub fn new(
        base: Rc<Base>,
        transaction: &mut Transaction,
        scene: &Scene
    ) -> Result<Self, vk::Result> {
        //Meshes
        let mut vertices = Vec::<Vertex>::new();
        let mut indices = Vec::<u16>::new();
        let mut meshes = Vec::<DeviceMesh>::new();
        let mut mesh_commands = Vec::<vk::DrawIndexedIndirectCommand>::new();
        let mut mesh_offsets = Vec::<usize>::new();
        for mesh in &scene.meshes {
            mesh_offsets.push(meshes.len());
            for primitive in &mesh.primitives {
                //Geometry
                mesh_commands.push(*vk::DrawIndexedIndirectCommand::builder()
                    .index_count(primitive.indices.len() as u32)
                    .instance_count(1)
                    .first_index(indices.len() as u32)
                    .vertex_offset(vertices.len() as i32)
                    .first_instance(0)
                );
                vertices.extend_from_slice(&primitive.vertices);
                indices.extend_from_slice(&primitive.indices);
                //Bounds
                let mut lower_bounds = na::Point3::<f32>::new(f32::MAX, f32::MAX, f32::MAX);
                let mut upper_bounds = na::Point3::<f32>::new(f32::MIN, f32::MIN, f32::MIN);
                for vertex in &primitive.vertices {
                    lower_bounds.x = lower_bounds.x.min(vertex.pos.x);
                    lower_bounds.y = lower_bounds.y.min(vertex.pos.y);
                    lower_bounds.z = lower_bounds.z.min(vertex.pos.z);
                    upper_bounds.x = upper_bounds.x.max(vertex.pos.x);
                    upper_bounds.y = upper_bounds.y.max(vertex.pos.y);
                    upper_bounds.z = upper_bounds.z.max(vertex.pos.z);
                }
                //Device mesh
                meshes.push(DeviceMesh {
                    upper_bounds: upper_bounds.into(),
                    lower_bounds: lower_bounds.into(),
                    material: primitive.material
                });
            }
        }
        //Nodes
        let mut nodes = Vec::<DeviceNode>::new();
        for (node, transform) in std::iter::zip(&scene.nodes, scene.transformations()) {
            if let Some(mesh) = node.mesh {
                for i in 0..(scene.meshes[mesh as usize].primitives.len()) {
                    nodes.push(DeviceNode {
                        transform: transform.to_homogeneous(),
                        inverse_transform: transform.inverse().to_homogeneous(),
                        mesh: (mesh_offsets[mesh as usize] + i) as u32,
                        flags: 1
                    });
                }
            }
        }
        //Create device-local buffers
        let buffer_sizes = [
            vertices.len() * std::mem::size_of::<Vertex>(),
            indices.len() * std::mem::size_of::<u16>(),
            meshes.len() * std::mem::size_of::<DeviceMesh>(),
            scene.materials.len() * std::mem::size_of::<Material>(),
            mesh_commands.len() * std::mem::size_of::<vk::DrawIndexedIndirectCommand>(),
            nodes.len() * std::mem::size_of::<DeviceNode>(),
            nodes.len() * std::mem::size_of::<vk::DrawIndexedIndirectCommand>(),
            nodes.len() * std::mem::size_of::<[u32; 2]>(),
            std::mem::size_of::<u32>()
        ];
        let create_infos = [
            //Vertices
            *vk::BufferCreateInfo::builder()
                .size(buffer_sizes[0] as u64)
                .usage(vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE),
            //Indices
            *vk::BufferCreateInfo::builder()
                .size(buffer_sizes[1] as u64)
                .usage(vk::BufferUsageFlags::INDEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE),
            //Meshes
            *vk::BufferCreateInfo::builder()
                .size(buffer_sizes[2] as u64)
                .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE),
            //Materials
            *vk::BufferCreateInfo::builder()
                .size(buffer_sizes[3] as u64)
                .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE),
            //Primitive draw commands
            *vk::BufferCreateInfo::builder()
                .size(buffer_sizes[4] as u64)
                .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE),
            //Nodes
            *vk::BufferCreateInfo::builder()
                .size((FRAME_COUNT * buffer_sizes[5]) as u64)
                .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE),
            //Draw commands
            *vk::BufferCreateInfo::builder()
                .size((FRAME_COUNT * buffer_sizes[6]) as u64)
                .usage(
                    vk::BufferUsageFlags::INDIRECT_BUFFER
                    | vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::TRANSFER_DST
                ).sharing_mode(vk::SharingMode::EXCLUSIVE),
            //Draw extras
            *vk::BufferCreateInfo::builder()
                .size((FRAME_COUNT * buffer_sizes[7]) as u64)
                .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE),
            //Draw count
            *vk::BufferCreateInfo::builder()
                .size((FRAME_COUNT * buffer_sizes[8]) as u64)
                .usage(
                    vk::BufferUsageFlags::INDIRECT_BUFFER
                    | vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::TRANSFER_DST
                ).sharing_mode(vk::SharingMode::EXCLUSIVE)
        ];
        let (buffers, buffer_alloc) = base.create_buffers(
            &create_infos,
            vk::MemoryPropertyFlags::DEVICE_LOCAL
        )?;
        //Write to buffers
        transaction.buffer_write(&vertices, buffers[0], 0);
        transaction.buffer_write(&indices, buffers[1], 0);
        transaction.buffer_write(&meshes, buffers[2], 0);
        transaction.buffer_write(&scene.materials, buffers[3], 0);
        transaction.buffer_write(&mesh_commands, buffers[4], 0);

        //Buffer descriptors
        let mut buffer_descriptors = Vec::<vk::DescriptorBufferInfo>::new();
        //Static descriptors
        for i in 2..=4 {
            buffer_descriptors.push(*vk::DescriptorBufferInfo::builder()
                .buffer(buffers[i])
                .offset(0)
                .range(vk::WHOLE_SIZE)
            );
        }
        //Dynamic descriptors
        for i in 5..=8 {
            let size = buffer_sizes[i];
            for j in 0..FRAME_COUNT {
                buffer_descriptors.push(*vk::DescriptorBufferInfo::builder()
                    .buffer(buffers[i])
                    .offset((j * size) as u64)
                    .range(size as u64)
                );
            }
        }

        //Textures
        let format = vk::Format::R8G8B8A8_SRGB;
        //Create images
        let create_infos: Vec<_> = scene.textures.iter().map(|asset| {
            let extent = vk::Extent3D::builder()
                .width(asset.width())
                .height(asset.height())
                .depth(1);
            *vk::ImageCreateInfo::builder()
                .image_type(vk::ImageType::TYPE_2D)
                .format(format)
                .extent(*extent)
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
        }).collect();
        let (images, image_alloc) = base.create_images(
            &create_infos,
            vk::MemoryPropertyFlags::DEVICE_LOCAL
        )?;
        //Write to images
        for (asset, image) in std::iter::zip(&scene.textures, &images) {
            let subresource_range = vk::ImageSubresourceRange::builder()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .base_mip_level(0)
                .level_count(1)
                .base_array_layer(0)
                .layer_count(1);
            let subresource = vk::ImageSubresourceLayers::builder()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .mip_level(0)
                .base_array_layer(0)
                .layer_count(1);
            let extent = vk::Extent3D::builder()
                .width(asset.width())
                .height(asset.height())
                .depth(1);
            let region = vk::BufferImageCopy2::builder()
                .buffer_offset(0)
                .image_subresource(*subresource)
                .image_offset(vk::Offset3D::default())
                .image_extent(*extent);
            transaction.image_write(
                asset.as_raw(),
                *image,
                *subresource_range,
                std::slice::from_ref(&region),
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
            );
        }
        //Create image views
        let image_views: Vec<_> = images.iter().map(|image| {
            let component_mapping = vk::ComponentMapping::builder()
                .r(vk::ComponentSwizzle::IDENTITY)
                .g(vk::ComponentSwizzle::IDENTITY)
                .b(vk::ComponentSwizzle::IDENTITY)
                .a(vk::ComponentSwizzle::IDENTITY);
            let subresource_range = vk::ImageSubresourceRange::builder()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .base_mip_level(0)
                .level_count(1)
                .base_array_layer(0)
                .layer_count(1);
            let create_info = vk::ImageViewCreateInfo::builder()
                .image(*image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format)
                .components(*component_mapping)
                .subresource_range(*subresource_range);
            unsafe {
                base.device.create_image_view(&create_info, None).unwrap()
            }
        }).collect();

        //Image descriptors
        let mut image_descriptors = [
            *vk::DescriptorImageInfo::builder()
                .image_view(image_views[0])
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
            MAX_TEXTURES
        ];
        for i in 0..image_views.len() {
            image_descriptors[i] = *vk::DescriptorImageInfo::builder()
                .image_view(image_views[i])
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        }

        //Result
        Ok(Self {
            base,
            nodes,
            mesh_offsets,
            buffers: buffers.try_into().unwrap(),
            buffer_alloc,
            buffer_sizes,
            buffer_descriptors: buffer_descriptors.try_into().unwrap(),
            images,
            image_views,
            image_alloc,
            image_descriptors
        })
    }

    pub fn update(&mut self, scene: &Scene) {
        let old_len = self.nodes.len();
        self.nodes.clear();
        for (node, transform) in std::iter::zip(&scene.nodes, scene.transformations()) {
            if let Some(mesh) = node.mesh {
                for i in 0..(scene.meshes[mesh as usize].primitives.len()) {
                    self.nodes.push(DeviceNode {
                        transform: transform.to_homogeneous(),
                        inverse_transform: transform.inverse().to_homogeneous(),
                        mesh: (self.mesh_offsets[mesh as usize] + i) as u32,
                        flags: 1
                    });
                }
            }
        }
        assert!(self.nodes.len() == old_len);
    }
}

impl Drop for DeviceScene {
    fn drop(&mut self) {
        unsafe {
            for buffer in self.buffers {
                self.base.device.destroy_buffer(buffer, None);
            }
            self.base.device.free_memory(self.buffer_alloc, None);
            for image_view in &self.image_views {
                self.base.device.destroy_image_view(*image_view, None);
            }
            for image in &self.images {
                self.base.device.destroy_image(*image, None);
            }
            self.base.device.free_memory(self.image_alloc, None);
        }
    }
}
