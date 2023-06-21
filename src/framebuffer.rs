use ash::vk;
use super::base::Base;
use super::renderpass::RenderPass;
use super::renderpass;
use std::rc::Rc;

pub struct Framebuffer {
    base: Rc<Base>,
    pub extent: vk::Extent2D,
    pub descriptor_pool: vk::DescriptorPool,
    pub image_allocation: vk::DeviceMemory,
    pub frames: Vec<Frame>
}

///Container for data needed to independently render a frame.
pub struct Frame {
    base: Rc<Base>,
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
    //Descriptor sets: [mesh, skybox]
    pub descriptor_set: [vk::DescriptorSet; 2],
    //Synchronization
    /*
        Semaphores:
        1. Swapchain image acquired
        2. Presentation
    */
    pub semaphores: [vk::Semaphore; 2],
    pub fence: vk::Fence
}

impl Drop for Frame {
    fn drop(&mut self) {
        unsafe {
            self.base.device.destroy_fence(self.fence, None);
            for semaphore in self.semaphores {
                self.base.device.destroy_semaphore(semaphore, None);
            }
            self.base.device.free_command_buffers(
                self.base.command_pool,
                std::slice::from_ref(&self.command_buffer)
            );
            self.base.device.destroy_framebuffer(self.framebuffer, None);
            for image_view in self.image_views {
                self.base.device.destroy_image_view(image_view, None);
            }
            for image in self.images {
                self.base.device.destroy_image(image, None);
            }
        }
    }
}

impl Framebuffer {
    pub fn new(
        base: Rc<Base>,
        renderpass: &RenderPass,
        frame_count: u32
    ) -> Result<Self, vk::Result> {
        //Descriptor pool
        let pool_sizes = [
            *vk::DescriptorPoolSize::builder()
                .ty(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(3 * frame_count),
            *vk::DescriptorPoolSize::builder()
                .ty(vk::DescriptorType::SAMPLER)
                .descriptor_count(frame_count),
            *vk::DescriptorPoolSize::builder()
                .ty(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(frame_count * super::renderpass::MAX_TEXTURES),
            *vk::DescriptorPoolSize::builder()
                .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(3 * frame_count)
        ];
        let create_info = vk::DescriptorPoolCreateInfo::builder()
            .max_sets(2 * frame_count)
            .pool_sizes(&pool_sizes);
        let descriptor_pool = unsafe {base.device.create_descriptor_pool(&create_info, None)}?;
        //Descriptor sets
        let layouts: Vec<vk::DescriptorSetLayout> =
            (0..frame_count).map(|_| renderpass.mesh_pipeline.descriptor_set_layout)
            .chain((0..frame_count).map(|_| renderpass.skybox_pipeline.descriptor_set_layout))
            .collect();
        let allocate_info = vk::DescriptorSetAllocateInfo::builder()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&layouts);
        let descriptor_sets = unsafe {base.device.allocate_descriptor_sets(&allocate_info)}?;
        //Frame images
        let extent_3d = vk::Extent3D::builder()
            .width(renderpass.extent.width)
            .height(renderpass.extent.height)
            .depth(1);
        let create_infos: Vec<vk::ImageCreateInfo> = [
            //Color image
            *vk::ImageCreateInfo::builder()
                .image_type(vk::ImageType::TYPE_2D)
                .format(renderpass::COLOR_FORMAT)
                .extent(*extent_3d)
                .mip_levels(1)
                .array_layers(1)
                .samples(renderpass::SAMPLE_COUNT)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .initial_layout(vk::ImageLayout::UNDEFINED),
            //Resolve image
            *vk::ImageCreateInfo::builder()
                .image_type(vk::ImageType::TYPE_2D)
                .format(renderpass::COLOR_FORMAT)
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
                .format(renderpass::DEPTH_FORMAT)
                .extent(*extent_3d)
                .mip_levels(1)
                .array_layers(1)
                .samples(renderpass::SAMPLE_COUNT)
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
                    .format(renderpass::COLOR_FORMAT)
                    .components(*component_mapping)
                    .subresource_range(*color_subresource_range),
                //Resolve image view
                vk::ImageViewCreateInfo::builder()
                    .image(images[1])
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(renderpass::COLOR_FORMAT)
                    .components(*component_mapping)
                    .subresource_range(*color_subresource_range),
                //Depth image view
                vk::ImageViewCreateInfo::builder()
                    .image(images[2])
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(renderpass::DEPTH_FORMAT)
                    .components(*component_mapping)
                    .subresource_range(*depth_subresource_range)
            ];
            let base = base.clone();
            let image_views = create_infos.map(
                |create_info| unsafe {&base.device.create_image_view(&create_info, None)}
                    .expect("Image view creation error")
            );
            //Framebuffer
            let create_info = vk::FramebufferCreateInfo::builder()
                .render_pass(renderpass.render_pass)
                .attachments(&image_views)
                .width(renderpass.extent.width)
                .height(renderpass.extent.height)
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
                base,
                images,
                image_views,
                framebuffer,
                command_buffer,
                descriptor_set: [descriptor_sets[i as usize], descriptor_sets[(i + frame_count) as usize]],
                semaphores,
                fence
            });
        }
        Ok(Self {
            base,
            extent: renderpass.extent,
            descriptor_pool,
            image_allocation,
            frames
        })
    }
}

impl Drop for Framebuffer {
    fn drop(&mut self) {
        unsafe {
            self.base.device.destroy_descriptor_pool(self.descriptor_pool, None);
            self.base.device.free_memory(self.image_allocation, None);
        }
    }
}
