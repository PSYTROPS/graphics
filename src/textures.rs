use ash::vk;
use super::base::Base;
use super::transfer::transaction::Transaction;
use std::rc::Rc;

pub struct Textures {
    base: Rc<Base>,
    pub images: Vec<vk::Image>,
    pub image_views: Vec<vk::ImageView>,
    pub allocation: vk::DeviceMemory
}

impl Textures {
    pub fn new(
        base: Rc<Base>,
        transaction: &mut Transaction,
        assets: &[image::RgbaImage]
    ) -> Result<Textures, vk::Result> {
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
        //Write to images
        for (asset, image) in std::iter::zip(assets, &images) {
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
            let extent = vk::Extent3D::builder()
                .width(asset.width())
                .height(asset.height())
                .depth(1);
            let region = vk::BufferImageCopy2::builder()
                .buffer_offset(0)
                .image_subresource(*subresource)
                .image_offset(vk::Offset3D::default())
                .image_extent(*extent);
            transaction.image_write(
                asset.as_raw(),
                *image,
                *subresource_range,
                std::slice::from_ref(&region),
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
            );
        }
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
        Ok(Textures {base, images, image_views, allocation})
    }
}

impl Drop for Textures {
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
