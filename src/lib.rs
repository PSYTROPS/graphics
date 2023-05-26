mod base;
mod framebuffer;
mod swapchain;
mod scene;

use base::Base;
use framebuffer::Framebuffer;
use swapchain::Swapchain;
use ash::vk;
use nalgebra_glm as glm;

pub struct Renderer {
    base: Base,
    framebuffer: Framebuffer,
    swapchain: Swapchain,
    //Scene data
    //TODO: Scene structure
    vertex_buffer: vk::Buffer,
    vertex_alloc: vk::DeviceMemory,
    //indices: vk::Buffer
    current_frame: u32
}

impl Renderer {
    pub fn new(window: &sdl2::video::Window) -> Result<Renderer, vk::Result> {
        let base = Base::new(window)?;
        let framebuffer = Framebuffer::new(&base, 512, 512, 2)?;
        let swapchain = Swapchain::new(&base, None)?;
        //Scene data
        let vertices = [
            scene::Vertex {
                pos: glm::vec3(-0.5, 0.5, 0.0),
                color: glm::vec3(1.0, 0.0, 0.0)
            },
            scene::Vertex {
                pos: glm::vec3(0.5, 0.5, 0.0),
                color: glm::vec3(0.0, 1.0, 0.0)
            },
            scene::Vertex {
                pos: glm::vec3(0.0, -0.5, 0.0),
                color: glm::vec3(0.0, 0.0, 1.0)
            },
        ];
        let create_info = vk::BufferCreateInfo::builder()
            .size((vertices.len() * std::mem::size_of::<scene::Vertex>()) as u64)
            .usage(vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let (buffers, vertex_alloc) = base.create_buffers(
            std::slice::from_ref(&create_info),
            vk::MemoryPropertyFlags::DEVICE_LOCAL
        )?;
        let vertex_buffer = buffers[0];
        base.staged_buffer_write(vertices.as_ptr(), vertex_buffer, vertices.len())?;
        Ok(Renderer {
            base,
            framebuffer,
            swapchain,
            vertex_buffer,
            vertex_alloc,
            current_frame: 0
        })
    }

    pub fn draw(&mut self) -> Result<(), vk::Result> {
        let frame = &self.framebuffer.frames[self.current_frame as usize];
        unsafe {
            //Acquire swapchain image
            let mut swapchain_index = 0;
            let mut swapchain_suboptimal = true;
            while swapchain_suboptimal {
                (swapchain_index, swapchain_suboptimal) = self.swapchain.loader.acquire_next_image(
                    self.swapchain.swapchain,
                    100_000_000, //100 milliseconds
                    frame.semaphores[0],
                    vk::Fence::null()
                )?;
                if swapchain_suboptimal {
                    //Recreate swapchain
                    self.base.device.queue_wait_idle(self.base.graphics_queue)?;
                    self.swapchain = swapchain::Swapchain::new(
                        &self.base,
                        Some(self.swapchain.swapchain)
                    )?;
                }
            }
            let swapchain_image = self.swapchain.images[swapchain_index as usize];
            //Wait for frame fence
            self.base.device.wait_for_fences(
                std::slice::from_ref(&frame.fence),
                false,
                100_000_000, //100 milliseconds
            )?;
            self.base.device.reset_fences(std::slice::from_ref(&frame.fence))?;
            //Record command buffer
            let begin_info = vk::CommandBufferBeginInfo::builder()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            self.base.device.begin_command_buffer(frame.command_buffer, &begin_info)?;
            //Drawing
            let render_area = vk::Rect2D::builder()
                .offset(vk::Offset2D::default())
                .extent(self.framebuffer.extent);
            let clear_values = [
                //Color
                vk::ClearValue {color: vk::ClearColorValue {float32: [0.0, 0.0, 0.0, 1.0]}},
                //Resolve
                vk::ClearValue {color: vk::ClearColorValue {float32: [0.0, 0.0, 0.0, 1.0]}},
                //Depth
                vk::ClearValue {depth_stencil: *vk::ClearDepthStencilValue::builder().depth(1.0)}
            ];
            let begin_info = vk::RenderPassBeginInfo::builder()
                .render_pass(self.framebuffer.render_pass)
                .framebuffer(frame.framebuffer)
                .render_area(*render_area)
                .clear_values(&clear_values);
            self.base.device.cmd_begin_render_pass(
                frame.command_buffer,
                &begin_info,
                vk::SubpassContents::INLINE
            );
            self.base.device.cmd_bind_pipeline(
                frame.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.framebuffer.pipeline
            );
            self.base.device.cmd_bind_vertex_buffers(
                frame.command_buffer,
                0,
                std::slice::from_ref(&self.vertex_buffer),
                &[0]
            );
            self.base.device.cmd_draw(
                frame.command_buffer,
                3, 1, 0, 0
            );
            self.base.device.cmd_end_render_pass(frame.command_buffer);
            //Pre-blitting image transition
            let subresource_range = vk::ImageSubresourceRange::builder()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .base_mip_level(0)
                .level_count(1)
                .base_array_layer(0)
                .layer_count(1);
            let image_barriers = [
                //Resolve image
                *vk::ImageMemoryBarrier2::builder()
                    .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
                    .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
                    .dst_stage_mask(vk::PipelineStageFlags2::BLIT)
                    .dst_access_mask(vk::AccessFlags2::TRANSFER_READ)
                    .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                    .src_queue_family_index(self.base.graphics_queue_family)
                    .dst_queue_family_index(self.base.graphics_queue_family)
                    .image(frame.images[1])
                    .subresource_range(*subresource_range),
                //Swapchain image
                *vk::ImageMemoryBarrier2::builder()
                    .src_stage_mask(vk::PipelineStageFlags2::NONE)
                    .src_access_mask(vk::AccessFlags2::NONE)
                    .dst_stage_mask(vk::PipelineStageFlags2::BLIT)
                    .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                    .old_layout(vk::ImageLayout::UNDEFINED)
                    .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .src_queue_family_index(self.base.graphics_queue_family)
                    .dst_queue_family_index(self.base.graphics_queue_family)
                    .image(swapchain_image)
                    .subresource_range(*subresource_range)
            ];
            let dependency = vk::DependencyInfo::builder()
                .image_memory_barriers(&image_barriers);
            self.base.device.cmd_pipeline_barrier2(frame.command_buffer, &dependency);
            //Blitting
            let subresource_layers = vk::ImageSubresourceLayers::builder()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .mip_level(0)
                .base_array_layer(0)
                .layer_count(1);
            let regions = vk::ImageBlit2::builder()
                .src_subresource(*subresource_layers)
                .src_offsets([
                    vk::Offset3D::default(),
                    *vk::Offset3D::builder()
                        .x(self.framebuffer.extent.width as i32)
                        .y(self.framebuffer.extent.height as i32)
                        .z(1)
                ]).dst_subresource(*subresource_layers)
                .dst_offsets([
                    vk::Offset3D::default(),
                    *vk::Offset3D::builder()
                        .x(self.swapchain.extent.width as i32)
                        .y(self.swapchain.extent.height as i32)
                        .z(1)
                ]);
            let blit_info = vk::BlitImageInfo2::builder()
                .src_image(frame.images[1])
                .src_image_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                .dst_image(swapchain_image)
                .dst_image_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .regions(std::slice::from_ref(&regions))
                .filter(vk::Filter::NEAREST);
            self.base.device.cmd_blit_image2(frame.command_buffer, &blit_info);
            //Transition swapchain image
            let image_barrier = vk::ImageMemoryBarrier2::builder()
                .src_stage_mask(vk::PipelineStageFlags2::BLIT)
                .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                .dst_access_mask(vk::AccessFlags2::NONE)
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                .src_queue_family_index(self.base.graphics_queue_family)
                .dst_queue_family_index(self.base.graphics_queue_family)
                .image(swapchain_image)
                .subresource_range(*subresource_range);
            let dependency = vk::DependencyInfo::builder()
                .image_memory_barriers(std::slice::from_ref(&image_barrier));
            self.base.device.cmd_pipeline_barrier2(frame.command_buffer, &dependency);
            self.base.device.end_command_buffer(frame.command_buffer)?;
            //Submit to queue
            let wait_semaphore_info = vk::SemaphoreSubmitInfo::builder()
                .semaphore(frame.semaphores[0])
                .stage_mask(vk::PipelineStageFlags2::BLIT);
            let command_buffer_info = vk::CommandBufferSubmitInfo::builder()
                .command_buffer(frame.command_buffer);
            let signal_semaphore_info = vk::SemaphoreSubmitInfo::builder()
                .semaphore(frame.semaphores[1])
                .stage_mask(vk::PipelineStageFlags2::BLIT);
            let submit_info = vk::SubmitInfo2::builder()
                .wait_semaphore_infos(std::slice::from_ref(&wait_semaphore_info))
                .command_buffer_infos(std::slice::from_ref(&command_buffer_info))
                .signal_semaphore_infos(std::slice::from_ref(&signal_semaphore_info));
            self.base.device.queue_submit2(
                self.base.graphics_queue,
                std::slice::from_ref(&submit_info),
                frame.fence
            )?;
            //Presentation
            let present_info = vk::PresentInfoKHR::builder()
                .wait_semaphores(std::slice::from_ref(&frame.semaphores[1]))
                .swapchains(std::slice::from_ref(&self.swapchain.swapchain))
                .image_indices(std::slice::from_ref(&swapchain_index));
            self.swapchain.loader.queue_present(self.base.graphics_queue, &present_info)?;
        }
        self.current_frame = (self.current_frame + 1) % self.framebuffer.frames.len() as u32;
        Ok(())
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            self.base.device.queue_wait_idle(self.base.graphics_queue).unwrap();
            self.base.device.destroy_buffer(self.vertex_buffer, None);
            self.base.device.free_memory(self.vertex_alloc, None);
            self.swapchain.destroy();
            self.framebuffer.destroy(&self.base);
            self.base.destroy();
        }
    }
}
