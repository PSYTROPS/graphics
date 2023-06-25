use ash::vk;
use nalgebra as na;
use super::base::Base;
use super::scene::{Vertex, Material, Scene, PointLight};
use super::textures::Textures;
use super::transfer::transaction::Transaction;
use std::rc::Rc;

pub struct DeviceScene {
    base: Rc<Base>,
    pub mesh_count: u32,
    pub transform_matrices: Vec<na::Matrix4<f32>>,
    pub transforms_size: usize,
    //Device buffers
    pub allocation: vk::DeviceMemory,
    pub vertices: vk::Buffer,
    pub indices: vk::Buffer,
    pub transforms: vk::Buffer,
    pub materials: vk::Buffer,
    pub lights: vk::Buffer,
    pub draw_commands: vk::Buffer,
    //Host buffer
    pub host_allocation: vk::DeviceMemory,
    pub staging: vk::Buffer,
    pub textures: Textures
}

impl DeviceScene {
    pub fn new(
        base: Rc<Base>,
        transaction: &mut Transaction,
        scene: &Scene,
        frame_count: usize
    ) -> Result<Self, vk::Result> {
        //Concatenate meshes
        let mut vertices = Vec::<Vertex>::new();
        let mut indices = Vec::<u16>::new();
        let mut mesh_commands = Vec::<vk::DrawIndexedIndirectCommand>::new();
        for mesh in &scene.meshes {
            mesh_commands.push(*vk::DrawIndexedIndirectCommand::builder()
                .index_count(mesh.indices.len() as u32)
                .instance_count(1)
                .first_index(indices.len() as u32)
                .vertex_offset(vertices.len() as i32)
                .first_instance(0)
            );
            vertices.extend_from_slice(&mesh.vertices);
            indices.extend_from_slice(&mesh.indices);
        }
        //Draw commands
        let draw_commands: Vec<_> = scene.nodes.iter().filter_map(
            |node| if let Some(mesh) = node.mesh {
                Some(mesh_commands[mesh as usize])
            } else {None}
        ).collect();
        //Node transformations
        let transform_matrices: Vec<_> = std::iter::zip(
            &scene.nodes,
            scene.transformations().iter().map(|t| t.to_homogeneous())
        ).filter_map(
            |(n, t)| if let Some(_) = n.mesh {Some(t)} else {None}
        ).collect();
        let transforms_size = transform_matrices.len() * std::mem::size_of::<na::Matrix4<f32>>();
        assert!(draw_commands.len() == transform_matrices.len());
        //Create device-local buffers
        let create_infos = [
            //Vertex buffer
            *vk::BufferCreateInfo::builder()
                .size((vertices.len() * std::mem::size_of::<Vertex>()) as u64)
                .usage(vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE),
            //Index buffer
            *vk::BufferCreateInfo::builder()
                .size((indices.len() * std::mem::size_of::<u16>()) as u64)
                .usage(vk::BufferUsageFlags::INDEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE),
            //Transforms buffer
            *vk::BufferCreateInfo::builder()
                .size((frame_count * transforms_size) as u64)
                .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE),
            //Materials buffer
            *vk::BufferCreateInfo::builder()
                .size((scene.materials.len() * std::mem::size_of::<Material>()) as u64)
                .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE),
            //Lights buffer
            *vk::BufferCreateInfo::builder()
                .size((scene.lights.len() * std::mem::size_of::<PointLight>()) as u64)
                .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE),
            //Draw commands
            *vk::BufferCreateInfo::builder()
                .size((draw_commands.len() * std::mem::size_of::<vk::DrawIndexedIndirectCommand>()) as u64)
                .usage(vk::BufferUsageFlags::INDIRECT_BUFFER | vk::BufferUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE),
        ];
        let (buffers, allocation) = base.create_buffers(
            &create_infos,
            vk::MemoryPropertyFlags::DEVICE_LOCAL
        )?;
        //Write to buffers
        transaction.buffer_write(&vertices, buffers[0], 0);
        transaction.buffer_write(&indices, buffers[1], 0);
        transaction.buffer_write(&scene.materials, buffers[3], 0);
        transaction.buffer_write(&scene.lights, buffers[4], 0);
        transaction.buffer_write(&draw_commands, buffers[5], 0);
        //Create staging buffer
        let create_info = vk::BufferCreateInfo::builder()
            .size(frame_count as u64 * transforms_size as u64)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let (host_buffers, host_allocation) = base.create_buffers(
            std::slice::from_ref(&create_info),
            vk::MemoryPropertyFlags::HOST_VISIBLE
        )?;
        let staging = host_buffers[0];
        //Write to staging buffer
        unsafe {
            let data = base.device.map_memory(host_allocation, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty())?;
            for i in 0..frame_count {
                transform_matrices.as_ptr().copy_to_nonoverlapping(
                    data.add(i * transforms_size) as *mut na::Matrix4<f32>,
                    transform_matrices.len()
                );
            }
            let memory_range = vk::MappedMemoryRange::builder()
                .memory(host_allocation)
                .offset(0)
                .size(vk::WHOLE_SIZE);
            base.device.flush_mapped_memory_ranges(std::slice::from_ref(&memory_range))?;
            base.device.unmap_memory(host_allocation);
        }
        let textures = Textures::new(base.clone(), transaction, &scene.textures)?;
        //Result
        Ok(Self {
            base,
            allocation,
            mesh_count: scene.meshes.len() as u32,
            transform_matrices,
            transforms_size,
            vertices: buffers[0],
            indices: buffers[1],
            transforms: buffers[2],
            materials: buffers[3],
            lights: buffers[4],
            draw_commands: buffers[5],
            host_allocation,
            staging,
            textures
        })
    }
}

impl Drop for DeviceScene {
    fn drop(&mut self) {
        unsafe {
            self.base.device.destroy_buffer(self.staging, None);
            self.base.device.free_memory(self.host_allocation, None);
            self.base.device.destroy_buffer(self.draw_commands, None);
            self.base.device.destroy_buffer(self.lights, None);
            self.base.device.destroy_buffer(self.materials, None);
            self.base.device.destroy_buffer(self.transforms, None);
            self.base.device.destroy_buffer(self.indices, None);
            self.base.device.destroy_buffer(self.vertices, None);
            self.base.device.free_memory(self.allocation, None);
        }
    }
}
