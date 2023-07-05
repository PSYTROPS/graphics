use ash::vk;
use ash::extensions::khr;
use std::fs::File;
use std::io::Read;
use std::io::Write;

///Container for persistent Vulkan objects (created once and never reassigned).
///Used to create transient Vulkan objects.
pub struct Base {
    pub entry: ash::Entry,
    pub instance: ash::Instance,
    pub surface: vk::SurfaceKHR,
    pub surface_loader: khr::Surface,
    pub physical_device: vk::PhysicalDevice,
    pub physical_device_properties: vk::PhysicalDeviceProperties,
    pub device: ash::Device,
    //Command submission
    pub graphics_queue_family: u32,
    pub transfer_queue_family: u32,
    pub graphics_queue: vk::Queue,
    pub command_pool: vk::CommandPool,
    pub pipeline_cache: vk::PipelineCache,
}

impl Base {
    pub fn new(window: &sdl2::video::Window) -> Result<Self, vk::Result> {
        //TODO: Vulkan portability subset support (needed for MoltenVK)
        //TODO: Debug utils messenger support
        unsafe {
            let entry = ash::Entry::linked();
            //Instance
            let app_name = std::ffi::CStr::from_bytes_with_nul_unchecked(b"Psychotronic\0");
            let app_info = vk::ApplicationInfo::builder()
                .application_name(app_name)
                .application_version(1)
                .engine_name(app_name)
                .engine_version(1)
                .api_version(vk::make_api_version(0, 1, 3, 0));
            let layer = std::ffi::CStr::from_bytes_with_nul_unchecked(
                b"VK_LAYER_KHRONOS_validation\0"
            ).as_ptr();
            let extensions = window.vulkan_instance_extensions()
                .expect("Couldn't get Vulkan instance extensions");
            let extensions = extensions.iter().map(
                |s| std::ffi::CString::new(*s).unwrap()
            ).collect::<Vec<_>>();
            let extension_names: Vec<*const std::os::raw::c_char> = extensions.iter().map(|s| s.as_ptr()).collect();
            let create_info = vk::InstanceCreateInfo::builder()
                .application_info(&app_info)
                .enabled_layer_names(std::slice::from_ref(&layer))
                .enabled_extension_names(&extension_names);
            let instance = entry.create_instance(&create_info, None)?;
            //Surface
            let surface: vk::SurfaceKHR = vk::Handle::from_raw(
                window.vulkan_create_surface(
                    vk::Handle::as_raw(instance.handle()) as usize
                ).expect("Surface creation error")
            );
            let surface_loader = khr::Surface::new(&entry, &instance);
            //Physical device
            //TODO: Support a separate transfer queue
            let physical_devices = instance.enumerate_physical_devices()?;
            let Some(&physical_device) = physical_devices.iter().find(|&&pd| {
                let properties = instance.get_physical_device_queue_family_properties(pd);
                //Find queue family with graphics & presentation support
                //(In practice, the graphics & present queue families are always the same)
                if let Some(_) = properties.iter().enumerate().find(
                    |(i, props)| props.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                        && props.queue_flags.contains(vk::QueueFlags::COMPUTE)
                        && surface_loader.get_physical_device_surface_support(pd, *i as u32, surface).unwrap_or_default()
                ) {true} else {false}
            }) else {
                surface_loader.destroy_surface(surface, None);
                instance.destroy_instance(None);
                return Err(vk::Result::ERROR_UNKNOWN);
            };
            let physical_device_properties = instance.get_physical_device_properties(physical_device);
            //Queue families
            let properties = instance.get_physical_device_queue_family_properties(physical_device);
            let graphics_queue_family = properties.iter().enumerate().position(
                |(i, props)| props.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                    && props.queue_flags.contains(vk::QueueFlags::COMPUTE)
                    && surface_loader.get_physical_device_surface_support(physical_device, i as u32, surface).unwrap_or_default()
            ).unwrap() as u32;
            let transfer_queue_family = if let Some(i) = properties.iter().position(
                |props| props.queue_flags.contains(vk::QueueFlags::TRANSFER)
                    && !props.queue_flags.contains(vk::QueueFlags::GRAPHICS)
            ) {i as u32} else {graphics_queue_family};
            //Device
            let queue_create_info = if graphics_queue_family != transfer_queue_family {
                vec![
                    *vk::DeviceQueueCreateInfo::builder()
                        .queue_family_index(graphics_queue_family)
                        .queue_priorities(&[1.0]),
                    *vk::DeviceQueueCreateInfo::builder()
                        .queue_family_index(transfer_queue_family)
                        .queue_priorities(&[1.0])
                ]
            } else {
                vec![
                    *vk::DeviceQueueCreateInfo::builder()
                        .queue_family_index(graphics_queue_family)
                        .queue_priorities(&[1.0])
                ]
            };
            let extensions = [
                khr::Swapchain::name().as_ptr(),
                vk::KhrShaderDrawParametersFn::name().as_ptr()
            ];
            let features = vk::PhysicalDeviceFeatures::builder()
                .multi_draw_indirect(true);
            let mut synchronization2 = vk::PhysicalDeviceSynchronization2Features::builder()
                .synchronization2(true);
            let mut vk12_features = vk::PhysicalDeviceVulkan12Features::builder()
                .draw_indirect_count(true)
                .descriptor_indexing(true)
                .shader_sampled_image_array_non_uniform_indexing(true)
                .shader_storage_buffer_array_non_uniform_indexing(true)
                .timeline_semaphore(true);
            let create_info = vk::DeviceCreateInfo::builder()
                .queue_create_infos(&queue_create_info)
                .enabled_extension_names(&extensions)
                .enabled_features(&features)
                .push_next(&mut synchronization2)
                .push_next(&mut vk12_features);
            let device = instance.create_device(physical_device, &create_info, None)?;
            //Queue
            let graphics_queue = device.get_device_queue(graphics_queue_family, 0);
            //Command pool
            let create_info = vk::CommandPoolCreateInfo::builder()
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                .queue_family_index(graphics_queue_family);
            let command_pool = device.create_command_pool(&create_info, None)?;
            //Pipeline cache
            let mut pipeline_cache_path = std::env::current_exe().unwrap();
            pipeline_cache_path.pop();
            pipeline_cache_path.push("pipeline-cache.bin");
            let mut data = Vec::new();
            let create_info = if pipeline_cache_path.exists() {
                let mut file = File::open(pipeline_cache_path).unwrap();
                file.read_to_end(&mut data).unwrap();
                vk::PipelineCacheCreateInfo::builder().initial_data(&data)
            } else {
                vk::PipelineCacheCreateInfo::builder()
            };
            let pipeline_cache = device.create_pipeline_cache(&create_info, None)?;
            Ok(Self {
                entry,
                instance,
                surface,
                surface_loader,
                physical_device,
                physical_device_properties,
                device,
                graphics_queue_family,
                transfer_queue_family,
                graphics_queue,
                command_pool,
                pipeline_cache
            })
        }
    }

    ///Allocate a memory block which satisfies the given memory requirements.
    ///Note that buffers & images cannot share a memory block.
    fn allocate(
        &self,
        requirements: &[vk::MemoryRequirements],
        properties: vk::MemoryPropertyFlags
    ) -> Result<(vk::DeviceMemory, Vec<vk::DeviceSize>), vk::Result> {
        //Determine offsets
        let mut offsets = Vec::<vk::DeviceSize>::new();
        let mut size: vk::DeviceSize = 0;
        let mut supported_memory_types = u32::MAX;
        for reqs in requirements {
            size = (size + reqs.alignment - 1) & !(reqs.alignment - 1);
            offsets.push(size);
            size += reqs.size;
            supported_memory_types &= reqs.memory_type_bits;
        }
        assert!(requirements.len() == offsets.len());
        //Find valid memory type
        let device_memory = unsafe {
            self.instance.get_physical_device_memory_properties(self.physical_device)
        };
        let Some((memory_type_index, _)) = device_memory
            .memory_types[..(device_memory.memory_type_count as usize)]
            .iter().enumerate()
            .find(|(i, mem_type)|
                mem_type.property_flags.contains(properties)
                && (supported_memory_types >> i) & 1 == 1
            ) else {return Err(vk::Result::ERROR_UNKNOWN)};
        //Allocate
        let create_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(size)
            .memory_type_index(memory_type_index as u32);
        let allocation = unsafe {self.device.allocate_memory(&create_info, None)}?;
        Ok((allocation, offsets))
    }

    ///Create buffers bound to a shared memory allocation.
    pub fn create_buffers(
        &self,
        create_infos: &[vk::BufferCreateInfo],
        properties: vk::MemoryPropertyFlags
        ) -> Result<(Vec<vk::Buffer>, vk::DeviceMemory), vk::Result> {
        unsafe {
            //Create buffers
            let buffers: Vec<_> = create_infos.iter().map(
                |create_info| self.device.create_buffer(create_info, None)
                    .expect("Buffer creation error")
            ).collect();
            //Allocate memory
            let requirements: Vec<_> = buffers.iter().map(
                |buffer| self.device.get_buffer_memory_requirements(*buffer)
            ).collect();
            let (allocation, offsets) = self.allocate(&requirements, properties)?;
            //Bind buffers to memory
            let bind_infos: Vec<_> = buffers.iter().zip(offsets).map(
                |(buffer, offset)| vk::BindBufferMemoryInfo::builder()
                    .buffer(*buffer).memory(allocation).memory_offset(offset).build()
            ).collect();
            self.device.bind_buffer_memory2(&bind_infos)?;
            Ok((buffers, allocation))
        }
    }

    ///Creates images bound to a shared memory allocation.
    pub fn create_images(
        &self,
        create_infos: &[vk::ImageCreateInfo],
        properties: vk::MemoryPropertyFlags
    ) -> Result<(Vec<vk::Image>, vk::DeviceMemory), vk::Result> {
        //Create images
        unsafe {
            let images: Vec<_> = create_infos.iter().map(
                |create_info| self.device.create_image(create_info, None)
                    .expect("Image creation error")
            ).collect();
            //Allocate memory
            let requirements: Vec<_> = images.iter().map(
                |image| self.device.get_image_memory_requirements(*image)
            ).collect();
            let (allocation, offsets) = self.allocate(&requirements, properties)?;
            //Bind images to memory
            let bind_infos: Vec<_> = std::iter::zip(&images, &offsets).map(
                |(image , offset)| *vk::BindImageMemoryInfo::builder()
                    .image(*image)
                    .memory(allocation)
                    .memory_offset(*offset)
            ).collect();
            self.device.bind_image_memory2(&bind_infos)?;
            Ok((images, allocation))
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
            let mut pipeline_cache_file = File::create(pipeline_cache_path).unwrap();
            pipeline_cache_file.write_all(&pipeline_cache_data).unwrap();
            //Destroy Vulkan objects
            self.device.device_wait_idle().unwrap();
            self.device.destroy_pipeline_cache(self.pipeline_cache, None);
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_device(None);
            self.surface_loader.destroy_surface(self.surface, None);
            self.instance.destroy_instance(None);
        }
    }
}
