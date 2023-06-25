use ash::vk::{self, BufferImageCopy2};
use ktx2::Reader;
use super::base::Base;
use super::transfer::transaction::Transaction;
use std::rc::Rc;

pub struct Environment {
    base: Rc<Base>,
    pub images: [vk::Image; 3],
    pub image_views: [vk::ImageView; 3],
    pub sampler: vk::Sampler,
    pub allocation: vk::DeviceMemory
}

impl Environment {
	pub fn new(
        base: Rc<Base>, 
        transaction: &mut Transaction,
        skybox: &[u8],
        diffuse: &[u8],
        specular: &[u8]
        ) -> Result<Environment, vk::Result> {
		//Read files to buffer
		let files = [skybox, diffuse, specular];
		let readers = files.map(|file| Reader::new(file).unwrap());
		//Create images
		let create_infos = [0, 1, 2].map(|i| {
			let header = readers[i].header();
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
        //Write to images
        for i in 0..3 {
            //Read image
            let reader = &readers[i];
            let header = reader.header();
            let mut texels = Vec::<u8>::new();
            let mut offsets = Vec::<usize>::new();
            for level in reader.levels() {
                offsets.push(texels.len());
                texels.extend_from_slice(level);
            }
            //Write
            let subresource_range = vk::ImageSubresourceRange::builder()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .base_mip_level(0)
                .level_count(header.level_count)
                .base_array_layer(0)
                .layer_count(6);
            //Regions
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
                    .buffer_offset(offsets[level as usize] as u64)
                    .image_subresource(*subresource)
                    .image_offset(vk::Offset3D::default())
                    .image_extent(*extent)
            }).collect();
            //Layout
            transaction.image_write(
                &texels,
                images[i],
                *subresource_range,
                &regions,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
            );
        }
		//Create image views
		let image_views = [0, 1, 2].map(|i| {
			let header = readers[i].header();
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
