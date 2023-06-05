use ash::vk;
use nalgebra as na;
use super::base::Base;
use super::scene::{Vertex, Scene};

#[derive(Default)]
pub struct DeviceScene {
    pub mesh_count: u32,
    pub transforms: Vec<na::Matrix4<f32>>,
    pub storage_size: usize,
    //Device buffers
    pub allocation: vk::DeviceMemory,
    pub vertices: vk::Buffer,
    pub indices: vk::Buffer,
    pub draw_commands: vk::Buffer,
    pub storage: vk::Buffer,
    pub host_allocation: vk::DeviceMemory,
    pub staging: vk::Buffer
}

impl DeviceScene {
    pub fn new(base: &Base, scene: &Scene, frame_count: usize) -> Result<Self, vk::Result> {
        //Concatenate meshes
        let mut vertices = Vec::<Vertex>::new();
        let mut indices = Vec::<u16>::new(); let mut draw_commands = Vec::<vk::DrawIndexedIndirectCommand>::new();
        for mesh in &scene.meshes {
            draw_commands.push(*vk::DrawIndexedIndirectCommand::builder()
                .index_count(mesh.indices.len() as u32)
                .instance_count(1)
                .first_index(indices.len() as u32)
                .vertex_offset(vertices.len() as i32)
                .first_instance(0)
            );
            vertices.extend_from_slice(&mesh.vertices);
            indices.extend_from_slice(&mesh.indices);
        }
        //Node transformations
        let transforms: Vec::<na::Matrix4<f32>> = scene.nodes.iter().map(
            |node| node.matrix().to_homogeneous()
        ).collect();
        let storage_size = transforms.len() * std::mem::size_of::<na::Matrix4<f32>>();
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
            //Draw commands
            *vk::BufferCreateInfo::builder()
                .size((draw_commands.len() * std::mem::size_of::<vk::DrawIndexedIndirectCommand>()) as u64)
                .usage(vk::BufferUsageFlags::INDIRECT_BUFFER | vk::BufferUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE),
            //Storage buffer
            *vk::BufferCreateInfo::builder()
                .size(frame_count as u64 * storage_size as u64)
                .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
        ];
        let (buffers, allocation) = base.create_buffers(
            &create_infos,
            vk::MemoryPropertyFlags::DEVICE_LOCAL
        )?;
        //Write to buffers
        base.staged_buffer_write(vertices.as_ptr(), buffers[0], vertices.len())?;
        base.staged_buffer_write(indices.as_ptr(), buffers[1], indices.len())?;
        base.staged_buffer_write(draw_commands.as_ptr(), buffers[2], draw_commands.len())?;
        //Create staging buffer
        let create_info = vk::BufferCreateInfo::builder()
            .size(frame_count as u64 * storage_size as u64)
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
                transforms.as_ptr().copy_to_nonoverlapping(
                    data.add(i * storage_size) as *mut na::Matrix4<f32>,
                    transforms.len()
                );
            }
            let memory_range = vk::MappedMemoryRange::builder()
                .memory(host_allocation)
                .offset(0)
                .size(vk::WHOLE_SIZE);
            base.device.flush_mapped_memory_ranges(std::slice::from_ref(&memory_range))?;
            base.device.unmap_memory(host_allocation);
        }
        Ok(Self {
            allocation,
            mesh_count: scene.meshes.len() as u32,
            transforms,
            storage_size,
            vertices: buffers[0],
            indices: buffers[1],
            draw_commands: buffers[2],
            storage: buffers[3],
            host_allocation,
            staging
        })
    }

    pub fn destroy(&self, base: &Base) {
        unsafe {
            base.device.destroy_buffer(self.staging, None);
            base.device.free_memory(self.host_allocation, None);
            base.device.destroy_buffer(self.storage, None);
            base.device.destroy_buffer(self.draw_commands, None);
            base.device.destroy_buffer(self.indices, None);
            base.device.destroy_buffer(self.vertices, None);
            base.device.free_memory(self.allocation, None);
        }
    }
}
