use ash::vk;
use super::base::Base;

#[derive(Default)]
pub struct Textures {
    pub images: Vec<vk::Image>,
    pub image_views: Vec<vk::ImageView>,
    pub allocation: vk::DeviceMemory
}

impl Textures {
    pub fn new(base: &Base, assets: &[image::RgbaImage]) -> Result<Textures, vk::Result> {
        //TODO: Reswizzle RgbaImage textures to BGRA format
        //let format = vk::Format::B8G8R8A8_SRGB;
        let format = vk::Format::R8G8B8A8_SRGB;
        //Create images
        let create_infos: Vec<_> = assets.iter().map(|asset| {
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
        let (images, allocation) = base.create_images(
            &create_infos,
            vk::MemoryPropertyFlags::DEVICE_LOCAL
        )?;
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
        assert!(images.len() == image_views.len());
        //Create staging buffer
        let create_info = vk::BufferCreateInfo::builder()
            .size(assets.iter().fold(
                0, |acc, item| acc + item.len() as u64
            )).usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let (staging, staging_alloc) = base.create_buffers(
            std::slice::from_ref(&create_info),
            vk::MemoryPropertyFlags::HOST_VISIBLE
        )?;
        unsafe {
            //Write to staging buffer
            let data = base.device.map_memory(staging_alloc, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty())?;
            let mut offset = 0;
            for asset in assets {
                asset.as_ptr().copy_to_nonoverlapping(
                    data.add(offset) as *mut u8,
                    asset.len()
                );
                offset += asset.len();
            }
            let memory_range = vk::MappedMemoryRange::builder()
                .memory(staging_alloc)
                .offset(0)
                .size(vk::WHOLE_SIZE);
            base.device.flush_mapped_memory_ranges(std::slice::from_ref(&memory_range))?;
            base.device.unmap_memory(staging_alloc);
            //Record commands
            //Pipeline barrier
            let begin_info = vk::CommandBufferBeginInfo::builder()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            base.device.begin_command_buffer(base.transfer_command_buffer, &begin_info)?;
            let image_barriers: Vec<_> = images.iter().map(|image| {
                let subresource_range = vk::ImageSubresourceRange::builder()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(1);
                *vk::ImageMemoryBarrier2::builder()
                    .src_stage_mask(vk::PipelineStageFlags2::NONE)
                    .src_access_mask(vk::AccessFlags2::NONE)
                    .dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
                    .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                    .old_layout(vk::ImageLayout::UNDEFINED)
                    .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .src_queue_family_index(base.graphics_queue_family)
                    .dst_queue_family_index(base.graphics_queue_family)
                    .image(*image)
                    .subresource_range(*subresource_range)
            }).collect();
            let dependency = vk::DependencyInfo::builder()
                .image_memory_barriers(&image_barriers);
            base.device.cmd_pipeline_barrier2(base.transfer_command_buffer, &dependency);
            //Copy buffer to image
            let mut offset = 0;
            for (i, asset) in assets.iter().enumerate() {
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
                    .buffer_offset(offset)
                    .image_subresource(*subresource)
                    .image_offset(vk::Offset3D::default())
                    .image_extent(*extent);
                let copy = vk::CopyBufferToImageInfo2::builder()
                    .src_buffer(staging[0])
                    .dst_image(images[i])
                    .dst_image_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .regions(std::slice::from_ref(&region));
                base.device.cmd_copy_buffer_to_image2(base.transfer_command_buffer, &copy);
                offset += asset.len() as u64;
            }
            //Pipeline barrier
            let image_barriers: Vec<_> = images.iter().map(|image| {
                let subresource_range = vk::ImageSubresourceRange::builder()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(1);
                *vk::ImageMemoryBarrier2::builder()
                    .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
                    .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                    .dst_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
                    .dst_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ)
                    .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .src_queue_family_index(base.graphics_queue_family)
                    .dst_queue_family_index(base.graphics_queue_family)
                    .image(*image)
                    .subresource_range(*subresource_range)
            }).collect();
            let dependency = vk::DependencyInfo::builder()
                .image_memory_barriers(&image_barriers);
            base.device.cmd_pipeline_barrier2(base.transfer_command_buffer, &dependency);
            base.device.end_command_buffer(base.transfer_command_buffer)?;
        }
        //Submit to queue
        let submit_info = vk::SubmitInfo::builder()
            .command_buffers(std::slice::from_ref(&base.transfer_command_buffer));
        unsafe {
            base.device.queue_submit(
                base.graphics_queue,
                std::slice::from_ref(&submit_info),
                base.transfer_fence
            )?;
            base.device.wait_for_fences(
                std::slice::from_ref(&base.transfer_fence),
                false,
                1_000_000_000, //8 seconds
            )?;
            base.device.reset_fences(std::slice::from_ref(&base.transfer_fence))?;
            base.device.destroy_buffer(staging[0], None);
            base.device.free_memory(staging_alloc, None);
        }
        Ok(Textures {images, image_views, allocation})
    }

    pub fn destroy(&self, base: &Base) {
        unsafe {
            for image_view in &self.image_views {
                base.device.destroy_image_view(*image_view, None);
            }
            for image in &self.images {
                base.device.destroy_image(*image, None);
            }
            base.device.free_memory(self.allocation, None);
        }
    }
}
