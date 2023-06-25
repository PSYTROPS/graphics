use ash::vk;
use super::base::Base;
use transaction::Transaction;
use std::rc::Rc;

const FENCE_TIMEOUT: u64 = 2_000_000_000; //TODO: Rename

mod arena;
pub mod transaction;

///Host-visible staging memory
struct Staging {
    base: Rc<Base>,
    pub buffer: vk::Buffer,
    pub alloc: vk::DeviceMemory,
    pub size: usize,
    pub ptr: *mut u8
}

pub struct Transfer {
    base: Rc<Base>,
    staging: Staging,
    queue: vk::Queue,
    command_pool: vk::CommandPool,
    command_buffer: vk::CommandBuffer,
    pub semaphore: vk::Semaphore,
    pub count: u64,
}

impl Staging {
    fn new(base: Rc<Base>, size: usize) -> Result<Self, vk::Result> {
        assert!(size > 0);
        let create_info = vk::BufferCreateInfo::builder()
            .size(size as u64)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let (buffers, alloc) = base.create_buffers(
            std::slice::from_ref(&create_info),
            vk::MemoryPropertyFlags::HOST_VISIBLE
        )?;
        let ptr = unsafe {
            base.device.map_memory(
                alloc,
                0,
                vk::WHOLE_SIZE,
                vk::MemoryMapFlags::empty()
            )? as *mut u8
        };
        Ok(Self {
            base,
            buffer: buffers[0],
            alloc,
            size,
            ptr
        })
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        unsafe {
            self.base.device.unmap_memory(self.alloc);
            self.base.device.destroy_buffer(self.buffer, None);
            self.base.device.free_memory(self.alloc, None);
        }
    }
}

impl Transfer {
    pub fn new(base: Rc<Base>) -> Result<Transfer, vk::Result> {
        //Staging buffer
        unsafe {
            let staging = Staging::new(base.clone(), 64).unwrap();
            let queue = base.device.get_device_queue(base.transfer_queue_family, 0);
            let create_info = vk::CommandPoolCreateInfo::builder()
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                .queue_family_index(base.transfer_queue_family);
            let command_pool = base.device.create_command_pool(&create_info, None)?;
            let create_info = vk::CommandBufferAllocateInfo::builder()
                .command_pool(command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1);
            let command_buffers = base.device.allocate_command_buffers(&create_info)?;
            let command_buffer = command_buffers[0];
            let count = 0;
            let mut type_info = vk::SemaphoreTypeCreateInfo::builder()
                .semaphore_type(vk::SemaphoreType::TIMELINE)
                .initial_value(count);
            let create_info = vk::SemaphoreCreateInfo::builder()
                .push_next(&mut type_info);
            let semaphore = base.device.create_semaphore(&create_info, None)?;
            Ok(Self {
                base,
                staging,
                queue,
                command_pool,
                command_buffer,
                semaphore,
                count
            })
        }
    }

    pub fn submit(
        &mut self,
        transaction: &Transaction,
    ) -> Result<(), vk::Result> {
        unsafe {
            //Wait for previous transfer
            let wait_info = vk::SemaphoreWaitInfo::builder()
                .semaphores(std::slice::from_ref(&self.semaphore))
                .values(std::slice::from_ref(&self.count));
            self.base.device.wait_semaphores(&wait_info, FENCE_TIMEOUT)?;
            //Write to mapped memory
            if self.staging.size < transaction.arena.size() {
                self.staging = Staging::new(self.base.clone(), transaction.arena.size())?;
            }
            transaction.arena.ptr().copy_to_nonoverlapping(
                self.staging.ptr,
                transaction.arena.size()
            );
            //Record command buffer
            let begin_info = vk::CommandBufferBeginInfo::builder()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            self.base.device.begin_command_buffer(self.command_buffer, &begin_info)?;
            //Copy buffers
            for transfer in &transaction.buffer_transfers {
                let region = vk::BufferCopy::builder()
                    .src_offset(transfer.src_offset as u64)
                    .dst_offset(transfer.dst_offset as u64)
                    .size(transfer.size as u64);
                self.base.device.cmd_copy_buffer(
                    self.command_buffer,
                    self.staging.buffer,
                    transfer.dst,
                    std::slice::from_ref(&region)
                );
            }
            //Copy images
            //Start barriers
            if transaction.start_image_barriers.len() > 0 {
                let dependency = vk::DependencyInfo::builder()
                    .image_memory_barriers(&transaction.start_image_barriers);
                self.base.device.cmd_pipeline_barrier2(self.command_buffer, &dependency);
            }
            //Copies
            for transfer in &transaction.image_transfers {
                let copy = vk::CopyBufferToImageInfo2::builder()
                    .src_buffer(self.staging.buffer)
                    .dst_image(transfer.dst)
                    .dst_image_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .regions(&transaction.regions[
                        transfer.region_offset..(transfer.region_offset + transfer.region_count)
                    ]);
                self.base.device.cmd_copy_buffer_to_image2(self.command_buffer, &copy);
            }
            //End barriers
            if transaction.end_image_barriers.len() > 0 {
                let dependency = vk::DependencyInfo::builder()
                    .image_memory_barriers(&transaction.end_image_barriers);
                self.base.device.cmd_pipeline_barrier2(self.command_buffer, &dependency);
            }
            self.base.device.end_command_buffer(self.command_buffer)?;
            //Submit to queue
            self.count += 1;
            let command_info = vk::CommandBufferSubmitInfo::builder()
                .command_buffer(self.command_buffer);
            let signal_info = vk::SemaphoreSubmitInfo::builder()
                .semaphore(self.semaphore)
                .value(self.count)
                .stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
                .device_index(0);
            let submit_info = vk::SubmitInfo2::builder()
                .command_buffer_infos(std::slice::from_ref(&command_info))
                .signal_semaphore_infos(std::slice::from_ref(&signal_info));
            self.base.device.queue_submit2(
                self.queue,
                std::slice::from_ref(&submit_info),
                vk::Fence::null()
            )?;
        }
        Ok(())
    }
}

impl Drop for Transfer {
    fn drop(&mut self) {
        unsafe {
            self.base.device.queue_wait_idle(self.queue).unwrap();
            self.base.device.destroy_semaphore(self.semaphore, None);
            self.base.device.free_command_buffers(
                self.command_pool,
                std::slice::from_ref(&self.command_buffer)
            );
            self.base.device.destroy_command_pool(self.command_pool, None);
        }
    }
}
