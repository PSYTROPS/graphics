use ash::vk;
use super::base::Base;
use super::scene::Vertex;

pub struct Framebuffer {
    pub extent: vk::Extent2D,
    pub render_pass: vk::RenderPass,
    pub pipeline: vk::Pipeline,
    //Frame data
    pub image_allocation: vk::DeviceMemory,
    pub frames: Vec<Frame>
}

///Container for data needed to independently render a frame.
pub struct Frame {
    /*
        Images:
        1. Color
        2. Resolve
        3. Depth
    */
    pub images: [vk::Image; 3],
    pub image_views: [vk::ImageView; 3],
    pub framebuffer: vk::Framebuffer,
    pub command_buffer: vk::CommandBuffer,
    //descriptor_set: vk::DescriptorSet,
    /*
        Semaphores:
        1. Swapchain image acquired
        2. Presentation
    */
    pub semaphores: [vk::Semaphore; 2],
    pub fence: vk::Fence
}

impl Frame {
    pub fn destroy(&self, base: &Base) {
        unsafe {
            base.device.destroy_fence(self.fence, None);
            for semaphore in self.semaphores {
                base.device.destroy_semaphore(semaphore, None);
            }
            base.device.free_command_buffers(
                base.command_pool,
                std::slice::from_ref(&self.command_buffer)
            );
            base.device.destroy_framebuffer(self.framebuffer, None);
            for image_view in self.image_views {
                base.device.destroy_image_view(image_view, None);
            }
            for image in self.images {
                base.device.destroy_image(image, None);
            }
        }
    }
}

impl Framebuffer {
    pub fn new(base: &Base, width: u32, height: u32, frame_count: u32)
        -> Result<Framebuffer, vk::Result> {
        let extent = vk::Extent2D {width, height};
        //Render pass
        let color_format = vk::Format::B8G8R8A8_SRGB;
        let depth_format = vk::Format::D32_SFLOAT;
        let samples = vk::SampleCountFlags::TYPE_4;
        let attachments = [
            //Color attachment
            *vk::AttachmentDescription::builder()
                .format(color_format)
                .samples(samples)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL),
            //Resolve attachment
            *vk::AttachmentDescription::builder()
                .format(color_format)
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL),
            //Depth attachment
            *vk::AttachmentDescription::builder()
                .format(depth_format)
                .samples(samples)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
        ];
        let references = [
            *vk::AttachmentReference::builder()
                .attachment(0)
                .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL),
            *vk::AttachmentReference::builder()
                .attachment(1)
                .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL),
            *vk::AttachmentReference::builder()
                .attachment(2)
                .layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
        ];
        let subpasses = [
            vk::SubpassDescription::builder()
                .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
                .color_attachments(&references[0..1])
                .resolve_attachments(&references[1..2])
                .depth_stencil_attachment(&references[2])
                .build()
        ];
        let create_info = vk::RenderPassCreateInfo::builder()
            .attachments(attachments.as_slice())
            .subpasses(subpasses.as_slice());
        let render_pass = unsafe {base.device.create_render_pass(&create_info, None)}?;
        //Pipeline
        let mut shader_dir = std::env::current_exe().unwrap();
        shader_dir.pop();
        shader_dir.push("shaders/");
        let vertex_shader = base.create_shader_module(shader_dir.join("test.vert.spv"))?;
        let fragment_shader = base.create_shader_module(shader_dir.join("test.frag.spv"))?;
        let shader_stages = [
            *vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vertex_shader)
                .name(unsafe {std::ffi::CStr::from_bytes_with_nul_unchecked(b"main\0")}),
            *vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(fragment_shader)
                .name(unsafe {std::ffi::CStr::from_bytes_with_nul_unchecked(b"main\0")})
        ];
        //Fixed functions
        //Vertex input
        let vertex_bindings = [
            *vk::VertexInputBindingDescription::builder()
                .binding(0)
                .stride(std::mem::size_of::<Vertex>() as u32)
                .input_rate(vk::VertexInputRate::VERTEX)
        ];
        let vertex_attributes = [
            //Position
            *vk::VertexInputAttributeDescription::builder()
                .location(0)
                .binding(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(0),
            //Color
            *vk::VertexInputAttributeDescription::builder()
                .location(1)
                .binding(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(12)
        ];
        let vertex_input = vk::PipelineVertexInputStateCreateInfo::builder()
            .vertex_binding_descriptions(&vertex_bindings)
            .vertex_attribute_descriptions(&vertex_attributes);
        //Input assembly
        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::builder()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        //Viewport
        let viewports = [
            *vk::Viewport::builder()
                .width(width as f32).height(height as f32)
                .min_depth(0.0).max_depth(1.0)
        ];
        let scissors = [
            *vk::Rect2D::builder().extent(extent)
        ];
        let viewport = vk::PipelineViewportStateCreateInfo::builder()
            .viewport_count(viewports.len() as u32).viewports(&viewports)
            .scissor_count(scissors.len() as u32).scissors(&scissors);
        //Rasterization
        let rasterization = vk::PipelineRasterizationStateCreateInfo::builder()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .line_width(1.0);
        //Multisampling
        let multisample = vk::PipelineMultisampleStateCreateInfo::builder()
            .rasterization_samples(samples);
        //Depth stencil
        let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::builder()
            .depth_test_enable(true)
            .depth_write_enable(true)
            .depth_compare_op(vk::CompareOp::LESS);
        //Color blending
        let color_blend_attachments = [
            *vk::PipelineColorBlendAttachmentState::builder()
                .blend_enable(false)
                .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
                .dst_color_blend_factor(vk::BlendFactor::DST_ALPHA)
                .color_blend_op(vk::BlendOp::ADD)
                .src_alpha_blend_factor(vk::BlendFactor::ONE)
                .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
                .alpha_blend_op(vk::BlendOp::ADD)
                .color_write_mask(vk::ColorComponentFlags::RGBA)
        ];
        let color_blend = vk::PipelineColorBlendStateCreateInfo::builder()
            .logic_op_enable(false)
            .attachments(&color_blend_attachments);
        //Create pipeline
        let create_infos = [
            *vk::GraphicsPipelineCreateInfo::builder()
                .stages(&shader_stages)
                .vertex_input_state(&vertex_input)
                .input_assembly_state(&input_assembly)
                .viewport_state(&viewport)
                .rasterization_state(&rasterization)
                .multisample_state(&multisample)
                .depth_stencil_state(&depth_stencil)
                .color_blend_state(&color_blend)
                .layout(base.pipeline_layout)
                .render_pass(render_pass)
                .subpass(0)
        ];
        let pipelines = match unsafe {base.device.create_graphics_pipelines(
            base.pipeline_cache,
            &create_infos,
            None
        )} {
            Ok(v) => v,
            Err(e) => {return Err(e.1);}
        };
        let pipeline = pipelines[0];
        //Destroy shader modules
        unsafe {
            base.device.destroy_shader_module(vertex_shader, None);
            base.device.destroy_shader_module(fragment_shader, None);
        }
        // ---------------- Frames ----------------
        //Frame images
        let extent_3d = vk::Extent3D::builder()
            .width(extent.width)
            .height(extent.height)
            .depth(1);
        let create_infos: Vec<vk::ImageCreateInfo> = [
            //Color image
            *vk::ImageCreateInfo::builder()
                .image_type(vk::ImageType::TYPE_2D)
                .format(color_format)
                .extent(*extent_3d)
                .mip_levels(1)
                .array_layers(1)
                .samples(samples)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .initial_layout(vk::ImageLayout::UNDEFINED),
            //Resolve image
            *vk::ImageCreateInfo::builder()
                .image_type(vk::ImageType::TYPE_2D)
                .format(color_format)
                .extent(*extent_3d)
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .initial_layout(vk::ImageLayout::UNDEFINED),
            //Depth image
            *vk::ImageCreateInfo::builder()
                .image_type(vk::ImageType::TYPE_2D)
                .format(depth_format)
                .extent(*extent_3d)
                .mip_levels(1)
                .array_layers(1)
                .samples(samples)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
        ].into_iter().cycle().take(3 * frame_count as usize).collect();
        let (images, image_allocation) = base.create_images(
            &create_infos, vk::MemoryPropertyFlags::DEVICE_LOCAL
        )?;
        let mut image_chunks = images.chunks_exact(3);
        //Frame command buffers
        let alloc_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(base.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(frame_count);
        let command_buffers = unsafe {base.device.allocate_command_buffers(&alloc_info)}?;
        //Frames
        let mut frames = Vec::<Frame>::new();
        for i in 0..frame_count {
            //Images
            let chunk = image_chunks.next().unwrap();
            let images = [chunk[0], chunk[1], chunk[2]];
            //Image views
            let component_mapping = vk::ComponentMapping::builder()
                .r(vk::ComponentSwizzle::IDENTITY)
                .g(vk::ComponentSwizzle::IDENTITY)
                .b(vk::ComponentSwizzle::IDENTITY)
                .a(vk::ComponentSwizzle::IDENTITY);
            let color_subresource_range = vk::ImageSubresourceRange::builder()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .base_mip_level(0)
                .level_count(1)
                .base_array_layer(0)
                .layer_count(1);
            let depth_subresource_range = vk::ImageSubresourceRange::builder()
                .aspect_mask(vk::ImageAspectFlags::DEPTH)
                .base_mip_level(0)
                .level_count(1)
                .base_array_layer(0)
                .layer_count(1);
            let create_infos = [
                //Color image view
                vk::ImageViewCreateInfo::builder()
                    .image(images[0])
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(color_format)
                    .components(*component_mapping)
                    .subresource_range(*color_subresource_range),
                //Resolve image view
                vk::ImageViewCreateInfo::builder()
                    .image(images[1])
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(color_format)
                    .components(*component_mapping)
                    .subresource_range(*color_subresource_range),
                //Depth image view
                vk::ImageViewCreateInfo::builder()
                    .image(images[2])
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(depth_format)
                    .components(*component_mapping)
                    .subresource_range(*depth_subresource_range)
            ];
            let image_views = create_infos.map(
                |create_info| unsafe {base.device.create_image_view(&create_info, None)}
                    .expect("Image view creation error")
            );
            //Framebuffer
            let create_info = vk::FramebufferCreateInfo::builder()
                .render_pass(render_pass)
                .attachments(&image_views)
                .width(extent.width)
                .height(extent.height)
                .layers(1);
            let framebuffer = unsafe {base.device.create_framebuffer(&create_info, None)}?;
            //Command buffer
            let command_buffer = command_buffers[i as usize];
            //Semaphores
            let semaphores = [vk::SemaphoreCreateInfo::default(); 2].map(
                |create_info| unsafe {base.device.create_semaphore(&create_info, None)}
                    .expect("Semaphore creation error")
            );
            //Fence
            let create_info = vk::FenceCreateInfo::builder()
                .flags(vk::FenceCreateFlags::SIGNALED);
            let fence = unsafe {base.device.create_fence(&create_info, None)}?;
            frames.push(Frame {
                images,
                image_views,
                framebuffer,
                command_buffer,
                semaphores,
                fence
            });
        }
        Ok(Framebuffer {extent, render_pass, pipeline, image_allocation, frames})
    }

    pub fn destroy(&self, base: &Base) {
        for frame in &self.frames {
            frame.destroy(base);
        }
        unsafe {
            base.device.free_memory(self.image_allocation, None);
            base.device.destroy_pipeline(self.pipeline, None);
            base.device.destroy_render_pass(self.render_pass, None);
        }
    }
}
