use ash::vk;
use ktx2::Reader;
use super::base::Base;
use std::rc::Rc;

pub struct Environment {
    base: Rc<Base>,
    pub images: [vk::Image; 3],
    pub image_views: [vk::ImageView; 3],
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
		//Write to staging buffer

		//Record command buffer
		//Pipeline barrier
		//Copy buffer to images
		//Pipeline barrier
		//Create image views
		let image_views = [vk::ImageView::default(); 3];
		Ok(Environment{
			base,
			images: images.try_into().unwrap(),
			image_views,
			allocation
		})
	}
}

impl Drop for Environment {
    fn drop(&mut self) {
        unsafe {
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
