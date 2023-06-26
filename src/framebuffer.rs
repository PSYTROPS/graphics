use ash::vk;
use crate::{FRAME_COUNT, COLOR_FORMAT, DEPTH_FORMAT, SAMPLE_COUNT, MAX_TEXTURES};
use super::base::Base;
use super::pipeline;
use std::rc::Rc;

pub struct Framebuffer {
    base: Rc<Base>,
    pub extent: vk::Extent2D,
    pub render_pass: vk::RenderPass,
    pub pipelines: [pipeline::Pipeline; 2],
    pub descriptor_pool: vk::DescriptorPool,
    pub image_allocation: vk::DeviceMemory,
    pub frames: [Frame; FRAME_COUNT]
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
    pub descriptor_set: [vk::DescriptorSet; 2], //[mesh, skybox]
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
        extent: vk::Extent2D
    ) -> Result<Self, vk::Result> {
        //Render pass
        let attachments = [
            //Color attachment
            *vk::AttachmentDescription::builder()
                .format(COLOR_FORMAT)
                .samples(SAMPLE_COUNT)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL),
            //Resolve attachment
            *vk::AttachmentDescription::builder()
                .format(COLOR_FORMAT)
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL),
            //Depth attachment
            *vk::AttachmentDescription::builder()
                .format(DEPTH_FORMAT)
                .samples(SAMPLE_COUNT)
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
        let render_pass = unsafe {
            base.device.create_render_pass(&create_info, None)?
        };
        //Pipelines
        let pipelines = [
            pipeline::mesh::new(base.clone(), extent, render_pass)?,
            pipeline::skybox::new(base.clone(), extent, render_pass)?
        ];
        //Descriptor pool
        let pool_sizes = [
            *vk::DescriptorPoolSize::builder()
                .ty(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(3 * FRAME_COUNT as u32),
            *vk::DescriptorPoolSize::builder()
                .ty(vk::DescriptorType::SAMPLER)
                .descriptor_count(FRAME_COUNT as u32),
            *vk::DescriptorPoolSize::builder()
                .ty(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count((FRAME_COUNT * MAX_TEXTURES) as u32),
            *vk::DescriptorPoolSize::builder()
                .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(4 * FRAME_COUNT as u32)
        ];
        let create_info = vk::DescriptorPoolCreateInfo::builder()
            .max_sets(2 * FRAME_COUNT as u32)
            .pool_sizes(&pool_sizes);
        let descriptor_pool = unsafe {base.device.create_descriptor_pool(&create_info, None)}?;
        //Descriptor sets
        let layouts: Vec<vk::DescriptorSetLayout> =
            std::iter::repeat(pipelines[0].descriptor_set_layout).take(FRAME_COUNT)
            .chain(std::iter::repeat(pipelines[1].descriptor_set_layout).take(FRAME_COUNT))
            .collect();
        let allocate_info = vk::DescriptorSetAllocateInfo::builder()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&layouts);
        let descriptor_sets = unsafe {base.device.allocate_descriptor_sets(&allocate_info)}?;
        //Frame images
        let extent_3d = vk::Extent3D::builder()
            .width(extent.width)
            .height(extent.height)
            .depth(1);
        let create_infos: Vec<vk::ImageCreateInfo> = [
            //Color image
            *vk::ImageCreateInfo::builder()
                .image_type(vk::ImageType::TYPE_2D)
                .format(COLOR_FORMAT)
                .extent(*extent_3d)
                .mip_levels(1)
                .array_layers(1)
                .samples(SAMPLE_COUNT)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .initial_layout(vk::ImageLayout::UNDEFINED),
            //Resolve image
            *vk::ImageCreateInfo::builder()
                .image_type(vk::ImageType::TYPE_2D)
                .format(COLOR_FORMAT)
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
                .format(DEPTH_FORMAT)
                .extent(*extent_3d)
                .mip_levels(1)
                .array_layers(1)
                .samples(SAMPLE_COUNT)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
        ].into_iter().cycle().take(3 * FRAME_COUNT).collect();
        let (images, image_allocation) = base.create_images(
            &create_infos, vk::MemoryPropertyFlags::DEVICE_LOCAL
        )?;
        let mut image_chunks = images.chunks_exact(3);
        //Command buffers
        let alloc_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(base.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(FRAME_COUNT as u32);
        let command_buffers = unsafe {base.device.allocate_command_buffers(&alloc_info)}?;
        //Frames
        let frames = [0, 1].map(|i| {
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
                    .format(COLOR_FORMAT)
                    .components(*component_mapping)
                    .subresource_range(*color_subresource_range),
                //Resolve image view
                vk::ImageViewCreateInfo::builder()
                    .image(images[1])
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(COLOR_FORMAT)
                    .components(*component_mapping)
                    .subresource_range(*color_subresource_range),
                //Depth image view
                vk::ImageViewCreateInfo::builder()
                    .image(images[2])
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(DEPTH_FORMAT)
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
                .render_pass(render_pass)
                .attachments(&image_views)
                .width(extent.width)
                .height(extent.height)
                .layers(1);
            let framebuffer = unsafe {
                base.device.create_framebuffer(&create_info, None)
            }.unwrap();
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
            let fence = unsafe {
                base.device.create_fence(&create_info, None)
            }.unwrap();
            Frame {
                base,
                images,
                image_views,
                framebuffer,
                command_buffer,
                descriptor_set: [descriptor_sets[i as usize], descriptor_sets[(i + FRAME_COUNT) as usize]],
                semaphores,
                fence
            }
        });
        Ok(Self {
            base,
            extent,
            render_pass,
            pipelines,
            descriptor_pool,
            image_allocation,
            frames
        })
    }
}

impl Drop for Framebuffer {
    fn drop(&mut self) {
        unsafe {
            self.base.device.destroy_render_pass(self.render_pass, None);
            self.base.device.destroy_descriptor_pool(self.descriptor_pool, None);
            self.base.device.free_memory(self.image_allocation, None);
        }
    }
}
