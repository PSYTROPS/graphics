use ash::vk::{self, BufferImageCopy2};
use ktx2::Reader;
use super::base::Base;
use std::rc::Rc;

pub struct Environment {
    base: Rc<Base>,
    pub images: [vk::Image; 3],
    pub image_views: [vk::ImageView; 3],
    pub sampler: vk::Sampler,
    pub allocation: vk::DeviceMemory
}

impl Environment {
	pub fn new(base: Rc<Base>, skybox: &[u8], diffuse: &[u8], specular: &[u8])
		-> Result<Environment, vk::Result> {
		//Read files to buffer
		let files = [skybox, diffuse, specular];
		let mut texels = Vec::<u8>::new();
		let mut offsets = Vec::<usize>::new(); //Offsets per mip level
		let headers = files.map(|file| {
			let reader = Reader::new(file).unwrap();
			let header = reader.header();
			for level in reader.levels() {
				offsets.push(texels.len());
				texels.extend_from_slice(level);
			}
			header
		});
		//Create images
		let create_infos = [0, 1, 2].map(|i| {
			let header = headers[i];
            let extent = vk::Extent3D::builder()
                .width(header.pixel_width)
                .height(header.pixel_height)
                .depth(1);
            *vk::ImageCreateInfo::builder()
				.flags(vk::ImageCreateFlags::CUBE_COMPATIBLE)
                .image_type(vk::ImageType::TYPE_2D)
                .format(vk::Format::from_raw(u32::from(header.format.unwrap().0) as i32))
                .extent(*extent)
                .mip_levels(header.level_count)
                .array_layers(6)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
		});
        let (images, allocation) = base.create_images(
            &create_infos,
            vk::MemoryPropertyFlags::DEVICE_LOCAL
        )?;
		//Create staging buffer
        let create_info = vk::BufferCreateInfo::builder()
			.size(texels.len() as u64)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let (staging, staging_alloc) = base.create_buffers(
            std::slice::from_ref(&create_info),
            vk::MemoryPropertyFlags::HOST_VISIBLE
        )?;
		unsafe {
			//Write to staging buffer
            let data = base.device.map_memory(staging_alloc, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty())?;
			texels.as_ptr().copy_to_nonoverlapping(data as *mut u8, texels.len());
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
			let image_barriers = [0, 1, 2].map(|i| {
				let header = headers[i];
                let subresource_range = vk::ImageSubresourceRange::builder()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(header.level_count)
                    .base_array_layer(0)
                    .layer_count(6);
                *vk::ImageMemoryBarrier2::builder()
                    .src_stage_mask(vk::PipelineStageFlags2::NONE)
                    .src_access_mask(vk::AccessFlags2::NONE)
                    .dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
                    .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                    .old_layout(vk::ImageLayout::UNDEFINED)
                    .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .src_queue_family_index(base.graphics_queue_family)
                    .dst_queue_family_index(base.graphics_queue_family)
                    .image(images[i])
                    .subresource_range(*subresource_range)
			});
            let dependency = vk::DependencyInfo::builder()
                .image_memory_barriers(&image_barriers);
            base.device.cmd_pipeline_barrier2(base.transfer_command_buffer, &dependency);
			//Copy buffer to images
			let mut total_mip_levels = 0;
			for i in 0..3 {
				let header = headers[i];
				let regions: Vec::<BufferImageCopy2> = (0..header.level_count).map(|level| {
					let subresource = vk::ImageSubresourceLayers::builder()
						.aspect_mask(vk::ImageAspectFlags::COLOR)
						.mip_level(level)
						.base_array_layer(0)
						.layer_count(6);
					let extent = vk::Extent3D::builder()
						.width(header.pixel_width >> level)
						.height(header.pixel_height >> level)
						.depth(1);
					*vk::BufferImageCopy2::builder()
						.buffer_offset(offsets[
							total_mip_levels
							+ level as usize
						] as u64)
						.image_subresource(*subresource)
						.image_offset(vk::Offset3D::default())
						.image_extent(*extent)
				}).collect();
				total_mip_levels += header.level_count as usize;
                let copy = vk::CopyBufferToImageInfo2::builder()
                    .src_buffer(staging[0])
                    .dst_image(images[i])
                    .dst_image_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .regions(&regions);
                base.device.cmd_copy_buffer_to_image2(base.transfer_command_buffer, &copy);
			}
			//Pipeline barrier
			let image_barriers = [0, 1, 2].map(|i| {
				let header = headers[i];
                let subresource_range = vk::ImageSubresourceRange::builder()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(header.level_count)
                    .base_array_layer(0)
                    .layer_count(6);
                *vk::ImageMemoryBarrier2::builder()
                    .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
                    .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                    .dst_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
                    .dst_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ)
                    .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .src_queue_family_index(base.graphics_queue_family)
                    .dst_queue_family_index(base.graphics_queue_family)
                    .image(images[i])
                    .subresource_range(*subresource_range)
			});
            let dependency = vk::DependencyInfo::builder()
                .image_memory_barriers(&image_barriers);
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
		//Create image views
		let image_views = [0, 1, 2].map(|i| {
			let header = headers[i];
            let component_mapping = vk::ComponentMapping::builder()
                .r(vk::ComponentSwizzle::IDENTITY)
                .g(vk::ComponentSwizzle::IDENTITY)
                .b(vk::ComponentSwizzle::IDENTITY)
                .a(vk::ComponentSwizzle::IDENTITY);
            let subresource_range = vk::ImageSubresourceRange::builder()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .base_mip_level(0)
                .level_count(header.level_count)
                .base_array_layer(0)
                .layer_count(6);
            let create_info = vk::ImageViewCreateInfo::builder()
                .image(images[i])
                .view_type(vk::ImageViewType::CUBE)
                .format(create_infos[i].format)
                .components(*component_mapping)
                .subresource_range(*subresource_range);
            unsafe {
                base.device.create_image_view(&create_info, None).unwrap()
            }
		});
        //Samplers
        let create_info = vk::SamplerCreateInfo::builder()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
            .address_mode_u(vk::SamplerAddressMode::REPEAT)
            .address_mode_v(vk::SamplerAddressMode::REPEAT)
            .address_mode_w(vk::SamplerAddressMode::REPEAT)
            .anisotropy_enable(false)
            .max_lod(vk::LOD_CLAMP_NONE);
        let sampler = unsafe {base.device.create_sampler(&create_info, None)?};
		Ok(Environment{
			base,
			images: images.try_into().unwrap(),
			image_views,
            sampler,
			allocation
		})
	}
}

impl Drop for Environment {
    fn drop(&mut self) {
        unsafe {
            self.base.device.destroy_sampler(self.sampler, None);
            for image_view in &self.image_views {
                self.base.device.destroy_image_view(*image_view, None);
            }
            for image in &self.images {
                self.base.device.destroy_image(*image, None);
            }
            self.base.device.free_memory(self.allocation, None);
        }
    }
}
