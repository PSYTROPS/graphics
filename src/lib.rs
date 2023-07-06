use ash::vk;

use base::Base;
use framebuffer::Framebuffer;
use swapchain::Swapchain;
use transfer::Transfer;
use transfer::transaction::Transaction;
use pipeline::PipelineLayout;
use scene_set::SceneSet;
use scene::PointLight;

use std::rc::Rc;
use std::cell::RefCell;

pub mod scene;
pub mod scene_set;
pub mod environment;
mod base;
mod transfer;
mod framebuffer;
mod swapchain;
mod camera;
mod device_scene;
mod pipeline;

pub const FRAME_COUNT: usize = 2;
pub const COLOR_FORMAT: vk::Format = vk::Format::B8G8R8A8_SRGB;
pub const DEPTH_FORMAT: vk::Format = vk::Format::D32_SFLOAT;
pub const SAMPLE_COUNT: vk::SampleCountFlags = vk::SampleCountFlags::TYPE_4;
pub const MAX_TEXTURES: usize = 64;
pub const MAX_LIGHTS: usize = 64;
pub const TIMEOUT: u64 = 1_000_000_000;

pub struct Renderer {
    pub base: Rc<Base>,
    transfer: Transfer,
    pub transaction: RefCell<Transaction>,
    framebuffer: Framebuffer,
    //Layouts: [mesh, skybox]
    layouts: [PipelineLayout; 2],
    swapchain: Swapchain,
    //Scene data
    skybox_vertex_buffer: vk::Buffer,
    skybox_vertex_alloc: vk::DeviceMemory,
    dfg_lookup: vk::Image,
    dfg_lookup_view: vk::ImageView,
    dfg_lookup_sampler: vk::Sampler,
    dfg_lookup_alloc: vk::DeviceMemory,
    dfg_descriptor: vk::DescriptorImageInfo,
    //Compute
    cull_layout: PipelineLayout,
    cull_pipeline: vk::Pipeline,
    current_frame: usize
}

impl<'a> Renderer {
    pub fn new(window: &sdl2::video::Window) -> Result<Self, vk::Result> {
        let base = Rc::new(Base::new(window)?);
        let transfer = Transfer::new(base.clone())?;
        let transaction = RefCell::new(Transaction::new(
            base.transfer_queue_family,
            base.graphics_queue_family
        ));
        let extent = vk::Extent2D {
            width: 1024,
            height: 1024
        };
        let layouts = [
            pipeline::mesh::create_layout(base.clone())?,
            pipeline::skybox::create_layout(base.clone())?
        ];
        let framebuffer = Framebuffer::new(base.clone(), extent, &layouts)?;
        let swapchain = Swapchain::new(base.clone(), None)?;
        //Compute culling
        let cull_layout = pipeline::cull::create_layout(base.clone())?;
        let cull_pipeline = (cull_layout.create_pipeline)(
            &cull_layout,
            vk::Extent2D::default(),
            vk::RenderPass::default()
        )?;
        //Skybox mesh
        let skybox_vertices: [f32; 3 * 14] = [
            1.0, -1.0, -1.0,
            1.0, 1.0, -1.0,
            1.0, -1.0, 1.0,
            1.0, 1.0, 1.0,
            -1.0, 1.0, 1.0,
            1.0, 1.0, -1.0,
            -1.0, 1.0, -1.0,
            1.0, -1.0, -1.0,
            -1.0, -1.0, -1.0,
            1.0, -1.0, 1.0,
            -1.0, -1.0, 1.0,
            -1.0, 1.0, 1.0,
            -1.0, -1.0, -1.0,
            -1.0, 1.0, -1.0
        ];
        let create_info = vk::BufferCreateInfo::builder()
            .size(skybox_vertices.len() as u64 * 4)
            .usage(vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let (vertex_buffers, vertex_alloc) = base.create_buffers(
            std::slice::from_ref(&create_info),
            vk::MemoryPropertyFlags::HOST_VISIBLE
        )?;
        transaction.borrow_mut().buffer_write(&skybox_vertices, vertex_buffers[0], 0);
        //DFG lookup texture
        let dfg_lookup_bytes = include_bytes!("../assets/dfg_lut.bin");
        let extent = vk::Extent3D::builder().width(256).height(256).depth(1);
        let create_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::R16G16B16A16_SFLOAT)
            .extent(*extent)
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let (lut_images, lut_allocation) = base.create_images(
            std::slice::from_ref(&create_info),
            vk::MemoryPropertyFlags::DEVICE_LOCAL
        )?;
        //Write to DFG lookup texture
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
        let region = vk::BufferImageCopy2::builder()
            .buffer_offset(0)
            .image_subresource(*subresource)
            .image_offset(vk::Offset3D::default())
            .image_extent(*extent);
        transaction.borrow_mut().image_write(
            dfg_lookup_bytes,
            lut_images[0],
            *subresource_range, 
            std::slice::from_ref(&region),
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
        );
        //DGF lookup image view
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
            .image(lut_images[0])
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(vk::Format::R16G16B16A16_SFLOAT)
            .components(*component_mapping)
            .subresource_range(*subresource_range);
        let dfg_lookup_view = unsafe {
            base.device.create_image_view(&create_info, None).unwrap()
        };
        //DFG lookup sampler
        let create_info = vk::SamplerCreateInfo::builder()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .anisotropy_enable(false);
        let dfg_lookup_sampler = unsafe {base.device.create_sampler(&create_info, None)?};
        let dfg_descriptor = *vk::DescriptorImageInfo::builder()
            .sampler(dfg_lookup_sampler)
            .image_view(dfg_lookup_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        Ok(Renderer {
            base,
            transfer,
            transaction,
            layouts,
            framebuffer,
            swapchain,
            skybox_vertex_buffer: vertex_buffers[0],
            skybox_vertex_alloc: vertex_alloc,
            dfg_lookup: lut_images[0],
            dfg_lookup_view,
            dfg_lookup_sampler,
            dfg_lookup_alloc: lut_allocation,
            dfg_descriptor,
            cull_layout,
            cull_pipeline,
            current_frame: 0
        })
    }

    /**
        Draw bound scenes.
        The instructions proceed as follows:
        1. Execute transfers
        2. Acquire swapchain image
        3. Record graphics command buffer
            1. Update scene data
            2. Draw scenes
            3. Blit drawn image to swapchain image
    */
    pub fn draw(&mut self, scene_set: &SceneSet) -> Result<(), vk::Result> {
        let frame = &self.framebuffer.frames[self.current_frame];
        let mut transaction = self.transaction.borrow_mut();
        unsafe {
            //Acquire swapchain image
            let mut swapchain_index = 0;
            let mut swapchain_suboptimal = true;
            while swapchain_suboptimal {
                (swapchain_index, swapchain_suboptimal) = self.swapchain.loader.acquire_next_image(
                    self.swapchain.swapchain,
                    TIMEOUT,
                    frame.semaphores[0],
                    vk::Fence::null()
                )?;
                if swapchain_suboptimal {
                    //Recreate swapchain
                    self.base.device.queue_wait_idle(self.base.graphics_queue)?;
                    self.swapchain = swapchain::Swapchain::new(
                        self.base.clone(),
                        Some(self.swapchain.swapchain)
                    )?;
                }
            }
            let swapchain_image = self.swapchain.images[swapchain_index as usize];
            //Wait for frame fence
            self.base.device.wait_for_fences(
                std::slice::from_ref(&frame.fence),
                false,
                TIMEOUT
            )?;
            self.base.device.reset_fences(std::slice::from_ref(&frame.fence))?;
            //Transactions
            //Update uniforms
            let mut uniforms: [f32; 36] = [0.0; 36];
            uniforms[0..16].copy_from_slice(scene_set.camera.view().as_slice());
            uniforms[16..32].copy_from_slice(scene_set.camera.projection().as_slice());
            uniforms[32..36].copy_from_slice(scene_set.camera.pos.to_homogeneous().as_slice());
            transaction.buffer_write(
                &uniforms,
                scene_set.camera_buffer,
                self.current_frame * scene_set.camera_uniform_size
            );
            //Update lights
            transaction.buffer_write(
                &scene_set.lights,
                scene_set.lights_buffer,
                self.current_frame * MAX_LIGHTS * std::mem::size_of::<PointLight>()
            );
            //Update scene dynamic data
            for scene in &scene_set.scenes {
                //Nodes
                transaction.buffer_write(
                    &scene.nodes,
                    scene.buffers[5],
                    self.current_frame * scene.buffer_sizes[5]
                );
                //Draw count
                transaction.buffer_write::<u32>(
                    std::slice::from_ref(&0),
                    scene.buffers[8],
                    self.current_frame * scene.buffer_sizes[8]
                );
            }
            //Transfer operations
            let (transfer_semaphore, transfer_semaphore_value) = self.transfer.submit(
                &transaction,
                self.current_frame
            )?;
            //Record command buffer
            let begin_info = vk::CommandBufferBeginInfo::builder()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            self.base.device.begin_command_buffer(frame.command_buffer, &begin_info)?;
            //Pipeline barrier
            let memory_barrier = vk::MemoryBarrier2::builder()
                .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
                .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
                .dst_access_mask(vk::AccessFlags2::SHADER_WRITE);
            let dependency = vk::DependencyInfo::builder()
                .memory_barriers(std::slice::from_ref(&memory_barrier));
            self.base.device.cmd_pipeline_barrier2(frame.command_buffer, &dependency);
            //Transfer barriers
            if transaction.end_image_barriers.len() > 0 {
                let dependency = vk::DependencyInfo::builder()
                    .image_memory_barriers(&transaction.end_image_barriers);
                self.base.device.cmd_pipeline_barrier2(frame.command_buffer, &dependency);
            }
            //Compute culling
            self.base.device.cmd_bind_pipeline(
                frame.command_buffer,
                vk::PipelineBindPoint::COMPUTE,
                self.cull_pipeline
            );
            for (i, scene) in scene_set.scenes.iter().enumerate() {
                self.base.device.cmd_push_constants(
                    frame.command_buffer,
                    self.cull_layout.pipeline_layout,
                    vk::ShaderStageFlags::COMPUTE,
                    0,
                    &(scene.nodes.len() as u32).to_le_bytes()
                );
                self.base.device.cmd_bind_descriptor_sets(
                    frame.command_buffer,
                    vk::PipelineBindPoint::COMPUTE,
                    self.cull_layout.pipeline_layout,
                    0,
                    std::slice::from_ref(&scene_set.cull_descriptors(i, self.current_frame)),
                    &[]
                );
                self.base.device.cmd_dispatch(
                    frame.command_buffer,
                    ((scene.nodes.len() + 63) / 64) as u32,
                    1,
                    1
                );
            }
            //Pipeline barrier
            let memory_barrier = vk::MemoryBarrier2::builder()
                .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
                .src_access_mask(vk::AccessFlags2::SHADER_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::ALL_GRAPHICS)
                .dst_access_mask(vk::AccessFlags2::SHADER_READ);
            let dependency = vk::DependencyInfo::builder()
                .memory_barriers(std::slice::from_ref(&memory_barrier));
            self.base.device.cmd_pipeline_barrier2(frame.command_buffer, &dependency);
            //Drawing
            let render_area = vk::Rect2D::builder()
                .offset(vk::Offset2D::default())
                .extent(self.framebuffer.extent);
            let clear_values = [
                vk::ClearValue {color: vk::ClearColorValue {float32: [0.0, 0.0, 0.0, 1.0]}}, //Color
                vk::ClearValue {color: vk::ClearColorValue {float32: [0.0, 0.0, 0.0, 1.0]}}, //Resolve
                vk::ClearValue {depth_stencil: *vk::ClearDepthStencilValue::builder().depth(1.0)} //Depth
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
            //Draw meshes
            self.base.device.cmd_bind_pipeline(
                frame.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.framebuffer.pipelines[0]
            );
            //Draw scenes
            for (i, scene) in scene_set.scenes.iter().enumerate() {
                self.base.device.cmd_bind_vertex_buffers(
                    frame.command_buffer,
                    0,
                    std::slice::from_ref(&scene.buffers[0]),
                    &[0]
                );
                self.base.device.cmd_bind_index_buffer(
                    frame.command_buffer,
                    scene.buffers[1],
                    0,
                    vk::IndexType::UINT16
                );
                self.base.device.cmd_bind_descriptor_sets(
                    frame.command_buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    self.layouts[0].pipeline_layout,
                    0,
                    std::slice::from_ref(&scene_set.scene_descriptors(i, self.current_frame)),
                    &[]
                );
                /*
                self.base.device.cmd_draw_indexed_indirect(
                    frame.command_buffer,
                    scene.buffers[6],
                    0,
                    scene.nodes.len() as u32,
                    std::mem::size_of::<vk::DrawIndexedIndirectCommand>() as u32
                );
                */
                self.base.device.cmd_draw_indexed_indirect_count(
                    frame.command_buffer,
                    scene.buffers[6],
                    (self.current_frame * scene.buffer_sizes[6]) as u64,
                    scene.buffers[8],
                    (self.current_frame * scene.buffer_sizes[8]) as u64,
                    scene.nodes.len() as u32,
                    std::mem::size_of::<vk::DrawIndexedIndirectCommand>() as u32
                );
            }
            //Draw skybox
            self.base.device.cmd_bind_pipeline(
                frame.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.framebuffer.pipelines[1]
            );
            self.base.device.cmd_bind_descriptor_sets(
                frame.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.layouts[1].pipeline_layout,
                0,
                std::slice::from_ref(&scene_set.skybox_descriptors(self.current_frame)),
                &[]
            );
            self.base.device.cmd_bind_vertex_buffers(
                frame.command_buffer,
                0,
                std::slice::from_ref(&self.skybox_vertex_buffer),
                &[0]
            );
            self.base.device.cmd_draw(frame.command_buffer, 14, 1, 0, 0);
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
                .filter(vk::Filter::LINEAR);
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
            let wait_semaphore_infos = [
                *vk::SemaphoreSubmitInfo::builder()
                    .semaphore(frame.semaphores[0])
                    .stage_mask(vk::PipelineStageFlags2::BLIT),
                *vk::SemaphoreSubmitInfo::builder()
                    .semaphore(transfer_semaphore)
                    .value(transfer_semaphore_value)
                    .stage_mask(vk::PipelineStageFlags2::TRANSFER)
            ];
            let command_buffer_info = vk::CommandBufferSubmitInfo::builder()
                .command_buffer(frame.command_buffer);
            let signal_semaphore_info = vk::SemaphoreSubmitInfo::builder()
                .semaphore(frame.semaphores[1])
                .stage_mask(vk::PipelineStageFlags2::BLIT);
            let submit_info = vk::SubmitInfo2::builder()
                .wait_semaphore_infos(&wait_semaphore_infos)
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
        self.current_frame = (self.current_frame + 1) % self.framebuffer.frames.len();
        transaction.clear();
        Ok(())
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            self.base.device.device_wait_idle().unwrap();
            self.base.device.destroy_pipeline(self.cull_pipeline, None);
            self.base.device.destroy_buffer(self.skybox_vertex_buffer, None);
            self.base.device.free_memory(self.skybox_vertex_alloc, None);
            self.base.device.destroy_sampler(self.dfg_lookup_sampler, None);
            self.base.device.destroy_image_view(self.dfg_lookup_view, None);
            self.base.device.destroy_image(self.dfg_lookup, None);
            self.base.device.free_memory(self.dfg_lookup_alloc, None);
        }
    }
}
