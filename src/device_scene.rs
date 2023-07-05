use ash::vk;
use nalgebra as na;

use super::{FRAME_COUNT, MAX_TEXTURES};
use super::base::Base;
use super::scene::{Vertex, Material, Scene};
use super::transfer::transaction::Transaction;
use std::rc::Rc;

//Device-local structures must obey GLSL std430 layout alignment rules

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DevicePrimitive {
    pub material: u32
}

#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct DeviceMesh {
    pub lower_bounds: na::Vector4<f32>,
    pub upper_bounds: na::Vector4<f32>,
    pub primitive_offset: u32,
    pub primitive_count: u32
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
    //Buffers
    /*
        Buffers:
        0. Vertices
        1. Indices
        2. Primitives
        3. Meshes
        4. Materials
        5. Primitive draw commands
        6. Nodes (duplicated)
        7. Draw commands (duplicated)
        8. Draw extras [node, primitive] (duplicated)
        9. Draw command count (duplicated)
    */
    pub buffers: [vk::Buffer; 10],
    pub buffer_alloc: vk::DeviceMemory,
    pub buffer_sizes: [usize; 10],
    pub buffer_descriptors: [vk::DescriptorBufferInfo; 4 + 4 * FRAME_COUNT],
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
        //Primitives & meshes
        let mut vertices = Vec::<Vertex>::new();
        let mut indices = Vec::<u16>::new();
        let mut primitives = Vec::<DevicePrimitive>::new();
        let mut primitive_commands = Vec::<vk::DrawIndexedIndirectCommand>::new();
        let mut meshes = Vec::<DeviceMesh>::new();
        for mesh in &scene.meshes {
            let primitive_offset = primitives.len() as u32;
            let primitive_count = mesh.primitives.len() as u32;
            let mut lower_bounds = na::Point3::<f32>::new(f32::MAX, f32::MAX, f32::MAX);
            let mut upper_bounds = na::Point3::<f32>::new(f32::MIN, f32::MIN, f32::MIN);
            for primitive in &mesh.primitives {
                primitives.push(DevicePrimitive {
                    material: primitive.material
                });
                primitive_commands.push(*vk::DrawIndexedIndirectCommand::builder()
                    .index_count(primitive.indices.len() as u32)
                    .instance_count(1)
                    .first_index(indices.len() as u32)
                    .vertex_offset(vertices.len() as i32)
                    .first_instance(0)
                );
                vertices.extend_from_slice(&primitive.vertices);
                indices.extend_from_slice(&primitive.indices);
                for vertex in &primitive.vertices {
                    lower_bounds.x = lower_bounds.x.min(vertex.pos.x);
                    lower_bounds.y = lower_bounds.y.min(vertex.pos.y);
                    lower_bounds.z = lower_bounds.z.min(vertex.pos.z);
                    upper_bounds.x = upper_bounds.x.max(vertex.pos.x);
                    upper_bounds.y = upper_bounds.y.max(vertex.pos.y);
                    upper_bounds.z = upper_bounds.z.max(vertex.pos.z);
                }
            }
            meshes.push(DeviceMesh {
                lower_bounds: lower_bounds.into(),
                upper_bounds: upper_bounds.into(),
                primitive_offset,
                primitive_count
            });
        }
        //Nodes
        let nodes: Vec<_> = std::iter::zip(&scene.nodes, scene.transformations())
            .filter_map(|(node, transform)| {
                if let Some(mesh) = node.mesh {
                    Some(DeviceNode {
                        transform: transform.to_homogeneous(),
                        inverse_transform: transform.inverse().to_homogeneous(),
                        mesh,
                        flags: 1
                    })
                } else {None}
            }).collect();
        let nodes: Vec<_> = nodes.iter().cycle().take(FRAME_COUNT * nodes.len())
            .copied().collect();
        //Draw commands
        let mut draw_commands = Vec::<vk::DrawIndexedIndirectCommand>::new();
        let mut draw_commands_extra = Vec::<[u32; 2]>::new();
        for (i, node) in nodes.iter().enumerate() {
            let mesh = meshes[node.mesh as usize];
            let start = mesh.primitive_offset as usize;
            let end = (mesh.primitive_offset + mesh.primitive_count) as usize;
            draw_commands.extend_from_slice(&primitive_commands[start..end]);
            for j in start..end {
                draw_commands_extra.push([i as u32, j as u32]);
            }
        }
        //Create device-local buffers
        let buffer_sizes = [
            vertices.len() * std::mem::size_of::<Vertex>(),
            indices.len() * std::mem::size_of::<u16>(),
            primitives.len() * std::mem::size_of::<DevicePrimitive>(),
            meshes.len() * std::mem::size_of::<DeviceMesh>(),
            scene.materials.len() * std::mem::size_of::<Material>(),
            primitive_commands.len() * std::mem::size_of::<vk::DrawIndexedIndirectCommand>(),
            nodes.len() * std::mem::size_of::<DeviceNode>(),
            draw_commands.len() * std::mem::size_of::<vk::DrawIndexedIndirectCommand>(),
            draw_commands_extra.len() * std::mem::size_of::<[u32; 2]>(),
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
            //Primitives
            *vk::BufferCreateInfo::builder()
                .size(buffer_sizes[2] as u64)
                .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE),
            //Meshes
            *vk::BufferCreateInfo::builder()
                .size(buffer_sizes[3] as u64)
                .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE),
            //Materials
            *vk::BufferCreateInfo::builder()
                .size(buffer_sizes[4] as u64)
                .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE),
            //Primitive draw commands
            *vk::BufferCreateInfo::builder()
                .size(buffer_sizes[5] as u64)
                .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE),
            *vk::BufferCreateInfo::builder()
            //Nodes
                .size((FRAME_COUNT * buffer_sizes[6]) as u64)
                .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE),
            //Draw commands
            *vk::BufferCreateInfo::builder()
                .size((FRAME_COUNT * buffer_sizes[7]) as u64)
                .usage(
                    vk::BufferUsageFlags::INDIRECT_BUFFER
                    | vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::TRANSFER_DST
                ).sharing_mode(vk::SharingMode::EXCLUSIVE),
            //Draw extras
            *vk::BufferCreateInfo::builder()
                .size((FRAME_COUNT * buffer_sizes[8]) as u64)
                .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE),
            //Draw count
            *vk::BufferCreateInfo::builder()
                .size((FRAME_COUNT * buffer_sizes[9]) as u64)
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
        //Duplication
        let draw_commands: Vec<_> = draw_commands.iter().cycle()
            .take(FRAME_COUNT * draw_commands.len()).copied().collect();
        let draw_commands_extra: Vec<_> = draw_commands_extra.iter().cycle()
            .take(FRAME_COUNT * draw_commands_extra.len()).copied().collect();
        debug_assert!(draw_commands.len() == draw_commands_extra.len());
        //Write to buffers
        transaction.buffer_write(&vertices, buffers[0], 0);
        transaction.buffer_write(&indices, buffers[1], 0);
        transaction.buffer_write(&primitives, buffers[2], 0);
        transaction.buffer_write(&meshes, buffers[3], 0);
        transaction.buffer_write(&scene.materials, buffers[4], 0);
        transaction.buffer_write(&primitive_commands, buffers[5], 0);
        transaction.buffer_write(&nodes, buffers[6], 0);
        //transaction.buffer_write(&draw_commands, buffers[7], 0);
        //transaction.buffer_write(&draw_commands_extra, buffers[8], 0);

        //Buffer descriptors
        //Static descriptors
        let mut buffer_descriptors = vec![
            //Primitives
            *vk::DescriptorBufferInfo::builder()
                .buffer(buffers[2])
                .offset(0)
                .range(vk::WHOLE_SIZE),
            //Meshes
            *vk::DescriptorBufferInfo::builder()
                .buffer(buffers[3])
                .offset(0)
                .range(vk::WHOLE_SIZE),
            //Materials
            *vk::DescriptorBufferInfo::builder()
                .buffer(buffers[4])
                .offset(0)
                .range(vk::WHOLE_SIZE),
            //Primitive draw commands
            *vk::DescriptorBufferInfo::builder()
                .buffer(buffers[5])
                .offset(0)
                .range(vk::WHOLE_SIZE),
        ];
        //Dynamic descriptors
        let framed_buffer_descriptor = |i: usize, frame: usize| -> vk::DescriptorBufferInfo {
            debug_assert!(frame < FRAME_COUNT);
            let size = buffer_sizes[i];
            *vk::DescriptorBufferInfo::builder()
                .buffer(buffers[i])
                .offset((frame * size) as u64)
                .range(size as u64)
        };
        buffer_descriptors.extend((0..FRAME_COUNT).map(
            |frame| framed_buffer_descriptor(6, frame)
        )); //Nodes
        buffer_descriptors.extend((0..FRAME_COUNT).map(
            |frame| framed_buffer_descriptor(8, frame)
        )); //Draw command extras
        buffer_descriptors.extend((0..FRAME_COUNT).map(
            |frame| framed_buffer_descriptor(9, frame)
        )); //Draw command count
        buffer_descriptors.extend((0..FRAME_COUNT).map(
            |frame| framed_buffer_descriptor(7, frame)
        )); //Draw commands

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
        let nodes: Vec<_> = std::iter::zip(&scene.nodes, scene.transformations())
            .filter_map(|(node, transform)| {
                if let Some(mesh) = node.mesh {
                    Some(DeviceNode {
                        transform: transform.to_homogeneous(),
                        inverse_transform: transform.inverse().to_homogeneous(),
                        mesh,
                        flags: 0
                    })
                } else {None}
            }).collect();
        assert!(nodes.len() == self.nodes.len());
        self.nodes = nodes;
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
