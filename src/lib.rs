use ash::vk;
use nalgebra as na;

use base::Base;
use framebuffer::Framebuffer;
use swapchain::Swapchain;
use camera::Camera;
use device_scene::DeviceScene;
use environment::Environment;

use std::rc::Rc;

pub mod scene;
mod base;
mod framebuffer;
mod swapchain;
mod camera;
mod device_scene;
mod textures;
mod environment;
mod pipeline;

pub const COLOR_FORMAT: vk::Format = vk::Format::B8G8R8A8_SRGB;
pub const DEPTH_FORMAT: vk::Format = vk::Format::D32_SFLOAT;
pub const SAMPLE_COUNT: vk::SampleCountFlags = vk::SampleCountFlags::TYPE_4;
pub const MAX_TEXTURES: u32 = 64;
//pub const MAX_LIGHTS: u32 = 64;

pub struct Renderer {
    base: Rc<Base>,
    framebuffer: Framebuffer,
    swapchain: Swapchain,
    //Scene data
    pub camera: Camera,
    scenes: Vec<DeviceScene>,
    environment: Environment,
    skybox_vertex_buffer: vk::Buffer,
    skybox_vertex_alloc: vk::DeviceMemory,
    dfg_lookup: vk::Image,
    dfg_lookup_view: vk::ImageView,
    dfg_lookup_sampler: vk::Sampler,
    dfg_lookup_alloc: vk::DeviceMemory,
    current_frame: u32
}

impl Renderer {
    pub fn new(window: &sdl2::video::Window) -> Result<Self, vk::Result> {
        let base = Rc::new(Base::new(window)?);
        let extent = vk::Extent2D {
            width: 1024,
            height: 1024
        };
        let framebuffer = Framebuffer::new(base.clone(), extent, 2)?;
        let swapchain = Swapchain::new(&base, None)?;
        let camera = Camera::new();
        let environment = Environment::new(
            base.clone(),
            include_bytes!("../assets/specular.ktx2"),
            include_bytes!("../assets/diffuse.ktx2"),
            include_bytes!("../assets/specular.ktx2")
        )?;
        //Bind environment map descriptors
        let image_infos = [1, 2].map(|i|
            *vk::DescriptorImageInfo::builder()
                .sampler(environment.sampler)
                .image_view(environment.image_views[i])
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        );
        let mut writes: Vec<vk::WriteDescriptorSet> = framebuffer.frames.iter().map(|frame| {
            *vk::WriteDescriptorSet::builder()
                .dst_set(frame.descriptor_set[0])
                .dst_binding(5)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&image_infos)
        }).collect();
        //Bind skybox descriptors
        for frame in &framebuffer.frames {
            let image_info = vk::DescriptorImageInfo::builder()
                .sampler(environment.sampler)
                .image_view(environment.image_views[0])
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
            writes.push(
                *vk::WriteDescriptorSet::builder()
                    .dst_set(frame.descriptor_set[1])
                    .dst_binding(0)
                    .dst_array_element(0)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(std::slice::from_ref(&image_info))
            )
        }
        unsafe {
            base.device.update_descriptor_sets(&writes, &[]);
        }
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
        base.staged_buffer_write(skybox_vertices.as_ptr(), vertex_buffers[0], skybox_vertices.len())?;
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
        let create_info = vk::BufferCreateInfo::builder()
			.size(dfg_lookup_bytes.len() as u64)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let (staging, staging_alloc) = base.create_buffers(
            std::slice::from_ref(&create_info),
            vk::MemoryPropertyFlags::HOST_VISIBLE
        )?;
		unsafe {
			//Write to staging buffer
            let data = base.device.map_memory(staging_alloc, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty())?;
			dfg_lookup_bytes.as_ptr().copy_to_nonoverlapping(
                data as *mut u8, dfg_lookup_bytes.len()
            );
            let memory_range = vk::MappedMemoryRange::builder()
                .memory(staging_alloc)
                .offset(0)
                .size(vk::WHOLE_SIZE);
            base.device.flush_mapped_memory_ranges(std::slice::from_ref(&memory_range))?;
            base.device.unmap_memory(staging_alloc);
			//Record command buffer
            let begin_info = vk::CommandBufferBeginInfo::builder()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            base.device.begin_command_buffer(base.transfer_command_buffer, &begin_info)?;
			//Pipeline barrier
            let subresource_range = vk::ImageSubresourceRange::builder()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .base_mip_level(0)
                .level_count(1)
                .base_array_layer(0)
                .layer_count(1);
            let barrier = vk::ImageMemoryBarrier2::builder()
                .src_stage_mask(vk::PipelineStageFlags2::NONE)
                .src_access_mask(vk::AccessFlags2::NONE)
                .dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
                .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .src_queue_family_index(base.graphics_queue_family)
                .dst_queue_family_index(base.graphics_queue_family)
                .image(lut_images[0])
                .subresource_range(*subresource_range);
            let dependency = vk::DependencyInfo::builder()
                .image_memory_barriers(std::slice::from_ref(&barrier));
            base.device.cmd_pipeline_barrier2(base.transfer_command_buffer, &dependency);
			//Copy buffer to images
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
            let copy = vk::CopyBufferToImageInfo2::builder()
                .src_buffer(staging[0])
                .dst_image(lut_images[0])
                .dst_image_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .regions(std::slice::from_ref(&region));
            base.device.cmd_copy_buffer_to_image2(base.transfer_command_buffer, &copy);
            //Barrier
            let subresource_range = vk::ImageSubresourceRange::builder()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .base_mip_level(0)
                .level_count(1)
                .base_array_layer(0)
                .layer_count(1);
            let barrier = vk::ImageMemoryBarrier2::builder()
                .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
                .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
                .dst_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ)
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .src_queue_family_index(base.graphics_queue_family)
                .dst_queue_family_index(base.graphics_queue_family)
                .image(lut_images[0])
                .subresource_range(*subresource_range);
            let dependency = vk::DependencyInfo::builder()
                .image_memory_barriers(std::slice::from_ref(&barrier));
            base.device.cmd_pipeline_barrier2(base.transfer_command_buffer, &dependency);
            base.device.end_command_buffer(base.transfer_command_buffer)?;
			//Submit command buffer to queue
			let submit_info = vk::SubmitInfo::builder()
				.command_buffers(std::slice::from_ref(&base.transfer_command_buffer));
            base.device.queue_submit(
                base.graphics_queue,
                std::slice::from_ref(&submit_info),
                base.transfer_fence
            )?;
            base.device.wait_for_fences(
                std::slice::from_ref(&base.transfer_fence),
                false,
                1_000_000_000,
            )?;
            base.device.reset_fences(std::slice::from_ref(&base.transfer_fence))?;
            base.device.destroy_buffer(staging[0], None);
            base.device.free_memory(staging_alloc, None);
		}
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
        //Bind DFG lookup descriptors
        let image_info = vk::DescriptorImageInfo::builder()
            .sampler(dfg_lookup_sampler)
            .image_view(dfg_lookup_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        let writes: Vec<vk::WriteDescriptorSet> = framebuffer.frames.iter().map(|frame| {
            *vk::WriteDescriptorSet::builder()
                .dst_set(frame.descriptor_set[0])
                .dst_binding(6)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(std::slice::from_ref(&image_info))
        }).collect();
        unsafe {
            base.device.update_descriptor_sets(&writes, &[]);
        }
        Ok(Renderer {
            base,
            framebuffer,
            swapchain,
            camera,
            scenes: vec![],
            environment,
            skybox_vertex_buffer: vertex_buffers[0],
            skybox_vertex_alloc: vertex_alloc,
            dfg_lookup: lut_images[0],
            dfg_lookup_view,
            dfg_lookup_sampler,
            dfg_lookup_alloc: lut_allocation,
            current_frame: 0
        })
    }

    pub fn load_scene(&mut self, scene: &scene::Scene) -> Result<(), vk::Result> {
        let dev_scene = DeviceScene::new(self.base.clone(), &scene, self.framebuffer.frames.len())?;
        //Update descriptor sets
        let mut writes = Vec::<vk::WriteDescriptorSet>::new();
        let mut image_infos: Vec<_> = dev_scene.textures.image_views.iter().map(
            |image_view| *vk::DescriptorImageInfo::builder()
                .image_view(*image_view)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        ).collect();
        image_infos.extend(std::iter::repeat(image_infos[0].clone())
            .take(MAX_TEXTURES as usize - image_infos.len())
        );
        for (i, frame) in self.framebuffer.frames.iter().enumerate() {
            //Transforms
            let buffer_info = vk::DescriptorBufferInfo::builder()
                .buffer(dev_scene.transforms)
                .offset((i * dev_scene.transforms_size) as u64)
                .range(dev_scene.transforms_size as u64);
            writes.push(*vk::WriteDescriptorSet::builder()
                .dst_set(frame.descriptor_set[0])
                .dst_binding(0)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(std::slice::from_ref(&buffer_info))
            );
            //Materials
            let buffer_info = vk::DescriptorBufferInfo::builder()
                .buffer(dev_scene.materials)
                .offset(0)
                .range(vk::WHOLE_SIZE);
            writes.push(*vk::WriteDescriptorSet::builder()
                .dst_set(frame.descriptor_set[0])
                .dst_binding(1)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(std::slice::from_ref(&buffer_info))
            );
            //Textures
            writes.push(*vk::WriteDescriptorSet::builder()
                .dst_set(frame.descriptor_set[0])
                .dst_binding(3)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .image_info(&image_infos)
            );
            //Lights
            let buffer_info = vk::DescriptorBufferInfo::builder()
                .buffer(dev_scene.lights)
                .offset(0)
                .range(vk::WHOLE_SIZE);
            writes.push(*vk::WriteDescriptorSet::builder()
                .dst_set(frame.descriptor_set[0])
                .dst_binding(4)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(std::slice::from_ref(&buffer_info))
            );
        }
        unsafe {
            self.base.device.update_descriptor_sets(&writes, &[]);
        }
        self.scenes.push(dev_scene);
        Ok(())
    }

	/*
    pub fn update_scene(&mut self, scene: &scene::Scene) {
        if let Some(dev_scene) = &mut self.scene {
            for (i, node) in scene.nodes.iter().enumerate() {
                let transform = node.matrix().to_homogeneous();
                dev_scene.transform_matrices[i] = transform;
            }
        }
    }
    */

    /**
        Draw bound scenes.
        The instructions proceed as follows:
        1. Acquire swapchain image
        2. Record graphics command buffer
            1. Write push constants
            2. Update scene data
            3. Draw scenes
            4. Blit drawn image to swapchain image
    */
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
            //Push constants
            let mut push_constants: [f32; 32] = [0.0; 32];
            push_constants[0..16].copy_from_slice(self.camera.view().as_slice());
            push_constants[16..32].copy_from_slice(self.camera.projection().as_slice());
            self.base.device.cmd_push_constants(
                frame.command_buffer,
                self.framebuffer.pipelines[0].pipeline_layout,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                0,
                std::slice::from_raw_parts(
                    push_constants.as_ptr() as *const u8,
                    4 * push_constants.len()
                )
            );
            //Update scenes
            for scene in &self.scenes {
                //Update transformations
                let offset = self.current_frame as u64 * scene.transforms_size as u64;
                let data = self.base.device.map_memory(
                    scene.host_allocation,
                    offset,
                    scene.transforms_size as u64,
                    vk::MemoryMapFlags::empty()
                )?;
                scene.transform_matrices.as_ptr().copy_to_nonoverlapping(
                    data as *mut na::Matrix4<f32>,
                    scene.transform_matrices.len()
                );
                let memory_range = vk::MappedMemoryRange::builder()
                    .memory(scene.host_allocation)
                    .offset(offset as u64)
                    .size(scene.transforms_size as u64);
                self.base.device.flush_mapped_memory_ranges(std::slice::from_ref(&memory_range))?;
                self.base.device.unmap_memory(scene.host_allocation);
                //Staging buffer barrier
                let buffer_barrier = vk::BufferMemoryBarrier2::builder()
                    .src_stage_mask(vk::PipelineStageFlags2::HOST)
                    .src_access_mask(vk::AccessFlags2::HOST_WRITE)
                    .dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
                    .dst_access_mask(vk::AccessFlags2::TRANSFER_READ)
                    .src_queue_family_index(self.base.graphics_queue_family)
                    .dst_queue_family_index(self.base.graphics_queue_family)
                    .buffer(scene.staging)
                    .offset(offset)
                    .size(scene.transforms_size as u64);
                let dependency = vk::DependencyInfo::builder()
                    .buffer_memory_barriers(std::slice::from_ref(&buffer_barrier));
                self.base.device.cmd_pipeline_barrier2(frame.command_buffer, &dependency);
                //Write to transformations buffer
                let region = vk::BufferCopy::builder()
                    .src_offset(offset)
                    .dst_offset(offset)
                    .size(scene.transforms_size as u64);
                self.base.device.cmd_copy_buffer(
                    frame.command_buffer,
                    scene.staging,
                    scene.transforms,
                    std::slice::from_ref(&region)
                );
                //Transformations buffer barrier
                let buffer_barrier = vk::BufferMemoryBarrier2::builder()
                    .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
                    .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                    .dst_stage_mask(vk::PipelineStageFlags2::VERTEX_SHADER)
                    .dst_access_mask(vk::AccessFlags2::SHADER_STORAGE_READ)
                    .src_queue_family_index(self.base.graphics_queue_family)
                    .dst_queue_family_index(self.base.graphics_queue_family)
                    .buffer(scene.transforms)
                    .offset(offset)
                    .size(scene.transforms_size as u64);
                let dependency = vk::DependencyInfo::builder()
                    .buffer_memory_barriers(std::slice::from_ref(&buffer_barrier));
                self.base.device.cmd_pipeline_barrier2(frame.command_buffer, &dependency);
            }
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
                self.framebuffer.pipelines[0].pipeline
            );
            //TODO: Per-scene descriptor sets
            self.base.device.cmd_bind_descriptor_sets(
                frame.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.framebuffer.pipelines[0].pipeline_layout,
                0,
                std::slice::from_ref(&frame.descriptor_set[0]),
                &[]
            );
            //Draw scene
            for scene in &mut self.scenes {
                self.base.device.cmd_bind_vertex_buffers(
                    frame.command_buffer,
                    0,
                    std::slice::from_ref(&scene.vertices),
                    &[0]
                );
                self.base.device.cmd_bind_index_buffer(
                    frame.command_buffer,
                    scene.indices,
                    0,
                    vk::IndexType::UINT16
                );
                self.base.device.cmd_draw_indexed_indirect(
                    frame.command_buffer,
                    scene.draw_commands,
                    0,
                    scene.mesh_count,
                    std::mem::size_of::<vk::DrawIndexedIndirectCommand>() as u32
                );
            }
            //Draw skybox
            self.base.device.cmd_bind_pipeline(
                frame.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.framebuffer.pipelines[1].pipeline
            );
            self.base.device.cmd_bind_descriptor_sets(
                frame.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.framebuffer.pipelines[1].pipeline_layout,
                0,
                std::slice::from_ref(&frame.descriptor_set[1]),
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
            self.base.device.destroy_buffer(self.skybox_vertex_buffer, None);
            self.base.device.free_memory(self.skybox_vertex_alloc, None);
            self.base.device.destroy_sampler(self.dfg_lookup_sampler, None);
            self.base.device.destroy_image_view(self.dfg_lookup_view, None);
            self.base.device.destroy_image(self.dfg_lookup, None);
            self.base.device.free_memory(self.dfg_lookup_alloc, None);
        }
    }
}
