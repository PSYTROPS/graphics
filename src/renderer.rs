use ash::vk;
use ash::extensions::khr;
use std::fs::File;
use std::io::Read;
use std::io::Write;

/*
    Code conventions:
    1. Make static C strings with `std::ffi::CStr::from_bytes_with_nul_unchecked()`
    (instead of the similar `std::ffi::CStr::new()`).
    2. When creating Vulkan objects, use the provided builder pattern API whenever possible.
*/

struct Renderer {
    base: Base,
    resolution: Resolution,
    swapchain: Swapchain,
    frames: Vec<Frame>
}

struct Base {
    entry: ash::Entry,
    instance: ash::Instance,
    surface: vk::SurfaceKHR,
    surface_loader: khr::Surface,
    physical_device: vk::PhysicalDevice,
    device: ash::Device,
    //Command submission
    graphics_queue_family: u32,
    present_queue_family: u32,
    graphics_queue: vk::Queue,
    present_queue: vk::Queue,
    command_pool: vk::CommandPool,
    transfer_command_buffer: vk::CommandBuffer,
    pipeline_cache: vk::PipelineCache,
    //Layout
    //sampler: vk::Sampler,
    //descriptor_pool: vk::DescriptorPool,
    //descriptor_set_layout: vk::DescriptorSetLayout,
    pipeline_layout: vk::PipelineLayout,
}

//Resolution-dependent objects
struct Resolution {
    extent: vk::Extent2D,
    render_pass: vk::RenderPass,
    pipeline: vk::Pipeline
}

//Swapchain-related objects
struct Swapchain {
    extent: vk::Extent2D,
    swapchain: vk::SwapchainKHR,
    swapchain_images: Vec<vk::Image>
}

struct Frame {
    /*
        Images:
        1. Color
        2. Resolve
        3. Depth
    */
    images: [vk::Image; 3],
    image_views: [vk::ImageView; 3],
    framebuffers: [vk::Framebuffer; 3],
    command_buffer: vk::CommandBuffer,
    descriptor_set: vk::DescriptorSet,
    /*
        Semaphores:
        1. Swapchain image acquired
        2. Presentation
    */
    semaphores: [vk::Semaphore; 2],
    fence: vk::Fence,
}

impl Base {
    fn new(window: sdl2::video::Window) -> Base {
        //TODO: Proper error handling
        let entry = ash::Entry::linked();
        //Instance
        let app_info = vk::ApplicationInfo::builder()
            .application_name(unsafe {std::ffi::CStr::from_bytes_with_nul_unchecked(b"Psychotronic\0")})
            .application_version(1)
            .api_version(vk::make_api_version(0, 1, 3, 0));
        let layers = [unsafe {std::ffi::CStr::from_bytes_with_nul_unchecked(
            b"VK_LAYER_KHRONOS_validation\0")}.as_ptr()
        ];
        let extensions = window.vulkan_instance_extensions().unwrap().iter().map(
            |s| unsafe {std::ffi::CStr::from_bytes_with_nul_unchecked(
                s.bytes().chain([0]).collect::<Vec<_>>().as_slice()
            )}.as_ptr()
        ).collect::<Vec<_>>();
        let create_info = vk::InstanceCreateInfo::builder()
            .application_info(&app_info)
            .enabled_layer_names(&layers)
            .enabled_extension_names(&extensions);
        let instance = unsafe {entry.create_instance(&create_info, None)}
            .expect("Error creating Vulkan instance!");
        //Surface
        let surface: vk::SurfaceKHR = vk::Handle::from_raw(
            window.vulkan_create_surface(
                vk::Handle::as_raw(instance.handle()) as usize
            )
        .expect("Error creating Vulkan surface!"));
        let surface_loader = khr::Surface::new(&entry, &instance);
        //Physical device selection
        let physical_device = *unsafe {instance.enumerate_physical_devices()}.unwrap().iter().find(|phys_dev| {
            let queue_families = unsafe {instance.get_physical_device_queue_family_properties(**phys_dev)};
            let graphics_support = queue_families.iter().any(
                |props| props.queue_flags.contains(vk::QueueFlags::GRAPHICS)
            );
            let presentation_support = (0..queue_families.len()).any(
                |i| unsafe {surface_loader.get_physical_device_surface_support(
                    **phys_dev, i as u32, surface
                )}.unwrap()
            );
            graphics_support && presentation_support
        }).expect("No suitable physical device found!");
        //Queue families
        let queue_families = unsafe {instance.get_physical_device_queue_family_properties(physical_device)};
        let graphics_queue_family = queue_families.iter().enumerate().find(
            |(_, props)| props.queue_flags.contains(vk::QueueFlags::GRAPHICS)
        ).unwrap().0 as u32;
        let present_queue_family = (0..queue_families.len()).find(
            |i| unsafe {surface_loader.get_physical_device_surface_support(
                physical_device, *i as u32, surface
            )}.unwrap()
        ).unwrap() as u32;
        //Device
        let queue_create_infos = if graphics_queue_family == present_queue_family {
            vec![vk::DeviceQueueCreateInfo::builder()
                .queue_family_index(graphics_queue_family as u32)
                .queue_priorities(&[1.0]).build()
            ]
        } else {
            vec![
                vk::DeviceQueueCreateInfo::builder()
                    .queue_family_index(graphics_queue_family as u32)
                    .queue_priorities(&[1.0]).build(),
                vk::DeviceQueueCreateInfo::builder()
                    .queue_family_index(present_queue_family as u32)
                    .queue_priorities(&[1.0]).build(),
            ]
        };
        let create_info = vk::DeviceCreateInfo::builder()
            .queue_create_infos(queue_create_infos.as_slice());
        let device = unsafe {instance.create_device(physical_device, &create_info, None)}
            .expect("Error creating device!");
        //Queues
        let graphics_queue = unsafe {device.get_device_queue(graphics_queue_family, 0)};
        let present_queue = unsafe {device.get_device_queue(present_queue_family, 0)};
        //TODO: Create separate transfer queue
        //Command pool
        let create_info = vk::CommandPoolCreateInfo::builder()
            .queue_family_index(graphics_queue_family);
        let command_pool = unsafe {device.create_command_pool(&create_info, None)}
            .expect("Error creating command pool!");
        //Command buffer
        let create_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let transfer_command_buffer = unsafe {device.allocate_command_buffers(&create_info)}
            .expect("Error creating transfer command buffer!")[0];
        //Pipeline cache
        let mut pipeline_cache_path = std::env::current_exe().unwrap();
        pipeline_cache_path.pop();
        pipeline_cache_path.push("pipeline-cache.bin");
        let mut data = Vec::new();
        let create_info = if pipeline_cache_path.exists() {
            vk::PipelineCacheCreateInfo::builder()
        } else {
            let mut file = File::open(pipeline_cache_path).unwrap();
            file.read_to_end(&mut data).unwrap();
            vk::PipelineCacheCreateInfo::builder().initial_data(&data)
        };
        let pipeline_cache = unsafe {device.create_pipeline_cache(&create_info, None)}
            .expect("Error creating pipeline cache!");
        //Pipeline layout
        let create_info = vk::PipelineLayoutCreateInfo::builder();
        let pipeline_layout = unsafe {device.create_pipeline_layout(&create_info, None)}
            .expect("Error creating pipeline layout!");
        Base {
            entry,
            instance,
            surface,
            surface_loader,
            physical_device,
            device,
            graphics_queue_family,
            present_queue_family,
            graphics_queue,
            present_queue,
            command_pool,
            transfer_command_buffer,
            pipeline_cache,
            //sampler,
            //descriptor_pool,
            //descriptor_set_layout,
            pipeline_layout
        }
    }
}

impl Drop for Base {
    fn drop(&mut self) {
        unsafe {
            //Save pipeline cache
            let pipeline_cache_data = self.device.get_pipeline_cache_data(self.pipeline_cache).unwrap();
            let mut pipeline_cache_path = std::env::current_exe().unwrap();
            pipeline_cache_path.pop();
            pipeline_cache_path.push("pipeline-cache.bin");
            let mut pipeline_cache_file = File::open(pipeline_cache_path).unwrap();
            pipeline_cache_file.write_all(&pipeline_cache_data).unwrap();
            //Destroy Vulkan objects
            self.device.destroy_pipeline_cache(self.pipeline_cache, None);
            self.device.destroy_pipeline_layout(self.pipeline_layout, None);
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_device(None);
            self.surface_loader.destroy_surface(self.surface, None);
            self.instance.destroy_instance(None);
        }
    }
}
