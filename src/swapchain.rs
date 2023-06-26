use ash::vk;
use ash::extensions::khr;
use crate::FRAME_COUNT;
use crate::base::Base;

pub struct Swapchain {
    pub extent: vk::Extent2D,
    pub loader: khr::Swapchain,
    pub swapchain: vk::SwapchainKHR,
    pub images: Vec<vk::Image>
}

impl Swapchain {
    pub fn new(base: &Base, old_swapchain: Option<vk::SwapchainKHR>)
        -> Result<Self, vk::Result> {
        let surface_capabilities = unsafe {
            base.surface_loader.get_physical_device_surface_capabilities(
                base.physical_device,
                base.surface
            )
        }?;
        let extent = if surface_capabilities.current_extent.width == u32::MAX
            || surface_capabilities.current_extent.height == u32::MAX {
            surface_capabilities.max_image_extent
        } else {
            surface_capabilities.current_extent
        };
        let create_info = vk::SwapchainCreateInfoKHR::builder()
            .surface(base.surface)
            .min_image_count((FRAME_COUNT as u32).max(surface_capabilities.min_image_count))
            .image_format(vk::Format::B8G8R8A8_SRGB)
            .image_color_space(vk::ColorSpaceKHR::SRGB_NONLINEAR)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::TRANSFER_DST)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(vk::SurfaceTransformFlagsKHR::IDENTITY)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(vk::PresentModeKHR::FIFO)
            .old_swapchain(if let Some(sc) = old_swapchain {sc} else {vk::SwapchainKHR::null()});
        let loader = khr::Swapchain::new(&base.instance, &base.device);
        unsafe {
            let swapchain = loader.create_swapchain(&create_info, None)?;
            let images = loader.get_swapchain_images(swapchain)?;
            Ok(Self {extent, loader, swapchain, images})
        }
    }
}

impl Drop for Swapchain {
    fn drop(&mut self) {
        unsafe {
            self.loader.destroy_swapchain(self.swapchain, None);
        }
    }
}
