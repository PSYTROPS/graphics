use ash::vk;
use ash::extensions::khr;
use std::fs::File;
use std::io::Read;
use std::io::Write;

//Remember to match these values in the fragment shader
pub const MAX_TEXTURES: u32 = 64;
pub const MAX_LIGHTS: u32 = 64;

///Container for persistent Vulkan objects (created once and never reassigned).
///Used to create transient Vulkan objects.
pub struct Base {
    pub entry: ash::Entry,
    pub instance: ash::Instance,
    pub surface: vk::SurfaceKHR,
    pub surface_loader: khr::Surface,
    pub physical_device: vk::PhysicalDevice,
    pub device: ash::Device,
    //Command submission
    pub graphics_queue_family: u32,
    pub graphics_queue: vk::Queue,
    pub command_pool: vk::CommandPool,
    pub transfer_command_buffer: vk::CommandBuffer,
    pub transfer_fence: vk::Fence,
    pub pipeline_cache: vk::PipelineCache,
    //Layout
    pub sampler: vk::Sampler,
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    pub pipeline_layout: vk::PipelineLayout
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
            let Some((physical_device, graphics_queue_family)) = physical_devices.iter().find_map(|phys_dev| {
                let properties = instance.get_physical_device_queue_family_properties(*phys_dev);
                //Find queue family with graphics & presentation support
                //(In practice, the graphics & present queue families are never different)
                if let Some((family, _)) = properties.iter().enumerate().find(
                    |(i, props)| props.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                        && surface_loader.get_physical_device_surface_support(*phys_dev, *i as u32, surface).unwrap_or_default()
                ) {Some((*phys_dev, family as u32))} else {None}
            }) else {
                surface_loader.destroy_surface(surface, None);
                instance.destroy_instance(None);
                return Err(vk::Result::ERROR_UNKNOWN);
            };
            //Device
            let queue_create_info = vk::DeviceQueueCreateInfo::builder()
                .queue_family_index(graphics_queue_family)
                .queue_priorities(&[1.0]);
            let extensions = [
                khr::Swapchain::name().as_ptr(),
                vk::KhrShaderDrawParametersFn::name().as_ptr()
            ];
            let features = vk::PhysicalDeviceFeatures::builder()
                .multi_draw_indirect(true);
            let mut synchronization2 = vk::PhysicalDeviceSynchronization2Features::builder()
                .synchronization2(true);
            let mut vk12_features = vk::PhysicalDeviceVulkan12Features::builder()
                .descriptor_indexing(true)
                .shader_sampled_image_array_non_uniform_indexing(true)
                .shader_storage_buffer_array_non_uniform_indexing(true);
            let create_info = vk::DeviceCreateInfo::builder()
                .queue_create_infos(std::slice::from_ref(&queue_create_info))
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
            //Command buffer (transfer)
            let create_info = vk::CommandBufferAllocateInfo::builder()
                .command_pool(command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1);
            let command_buffers = device.allocate_command_buffers(&create_info)?;
            let transfer_command_buffer = command_buffers[0];
            //Fence (transfer)
            let create_info = vk::FenceCreateInfo::builder();
            let transfer_fence = device.create_fence(&create_info, None)?;
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
            //Sampler
            let create_info = vk::SamplerCreateInfo::builder()
                .mag_filter(vk::Filter::NEAREST)
                .min_filter(vk::Filter::NEAREST)
                .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                .address_mode_u(vk::SamplerAddressMode::REPEAT)
                .address_mode_v(vk::SamplerAddressMode::REPEAT)
                .address_mode_w(vk::SamplerAddressMode::REPEAT)
                .anisotropy_enable(false);
            let sampler = device.create_sampler(&create_info, None)?;
            //Descriptor set layout
            let bindings = [
                //Transforms
                *vk::DescriptorSetLayoutBinding::builder()
                    .binding(0)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::VERTEX),
                //Materials
                *vk::DescriptorSetLayoutBinding::builder()
                    .binding(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT),
                //Sampler
                *vk::DescriptorSetLayoutBinding::builder()
                    .binding(2)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                    .immutable_samplers(std::slice::from_ref(&sampler)),
                //Textures
                *vk::DescriptorSetLayoutBinding::builder()
                    .binding(3)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .descriptor_count(MAX_TEXTURES)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT),
                //Lights
                *vk::DescriptorSetLayoutBinding::builder()
                    .binding(4)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            ];
            let create_info = vk::DescriptorSetLayoutCreateInfo::builder()
                .bindings(&bindings);
            let descriptor_set_layout = device.create_descriptor_set_layout(&create_info, None)?;
            //Pipeline layout
            let push_constant = vk::PushConstantRange::builder()
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
                .offset(0)
                .size(2 * 16 * 4);
            let create_info = vk::PipelineLayoutCreateInfo::builder()
                .set_layouts(std::slice::from_ref(&descriptor_set_layout))
                .push_constant_ranges(std::slice::from_ref(&push_constant));
            let pipeline_layout = device.create_pipeline_layout(&create_info, None)?;
            Ok(Self {
                entry,
                instance,
                surface,
                surface_loader,
                physical_device,
                device,
                graphics_queue_family,
                graphics_queue,
                command_pool,
                transfer_command_buffer,
                transfer_fence,
                pipeline_cache,
                sampler,
                descriptor_set_layout,
                pipeline_layout
            })
        }
    }

    pub fn destroy(&mut self) {
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
            self.device.destroy_sampler(self.sampler, None);
            self.device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            self.device.destroy_pipeline_layout(self.pipeline_layout, None);
            self.device.destroy_fence(self.transfer_fence, None);
            self.device.free_command_buffers(self.command_pool, &[self.transfer_command_buffer]);
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_device(None);
            self.surface_loader.destroy_surface(self.surface, None);
            self.instance.destroy_instance(None);
        }
    }

    pub fn create_shader_module<P: AsRef<std::path::Path>>(&self, filename: P)
        -> Result<vk::ShaderModule, vk::Result> {
        let mut file = std::fs::File::open(filename).unwrap();
        let code = ash::util::read_spv(&mut file).unwrap();
        let create_info = vk::ShaderModuleCreateInfo::builder().code(&code);
        unsafe {self.device.create_shader_module(&create_info, None)}
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
            if size % reqs.alignment != 0 {
                size = (size / reqs.alignment + 1) * reqs.alignment;
            }
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

    pub fn staged_buffer_write<T>(&self, from: *const T, to: vk::Buffer, count: usize)
        -> Result<(), vk::Result> {
        let size = count * std::mem::size_of::<T>();
        //Create staging buffer
        let create_info = vk::BufferCreateInfo::builder()
            .size(size as u64)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let (buffers, allocation) = self.create_buffers(
            std::slice::from_ref(&create_info),
            vk::MemoryPropertyFlags::HOST_VISIBLE
        )?;
        let staging = buffers[0];
        //Write to staging buffer
        unsafe {
            let data = self.device.map_memory(allocation, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty())?;
            from.copy_to_nonoverlapping(data as *mut T, count);
            let memory_range = vk::MappedMemoryRange::builder()
                .memory(allocation)
                .offset(0)
                .size(vk::WHOLE_SIZE);
            self.device.flush_mapped_memory_ranges(std::slice::from_ref(&memory_range))?;
            self.device.unmap_memory(allocation);
            //Record commands
            let begin_info = vk::CommandBufferBeginInfo::builder()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            self.device.begin_command_buffer(self.transfer_command_buffer, &begin_info)?;
            let region = vk::BufferCopy::builder()
                .src_offset(0)
                .dst_offset(0)
                .size(size as u64);
            self.device.cmd_copy_buffer(
                self.transfer_command_buffer,
                staging,
                to,
                std::slice::from_ref(&region)
            );
            self.device.end_command_buffer(self.transfer_command_buffer)?;
            //Submit to queue
            let submit_info = vk::SubmitInfo::builder()
                .command_buffers(std::slice::from_ref(&self.transfer_command_buffer));
            self.device.queue_submit(
                self.graphics_queue,
                std::slice::from_ref(&submit_info),
                self.transfer_fence
            )?;
            //Destroy staging buffer
            self.device.wait_for_fences(
                std::slice::from_ref(&self.transfer_fence),
                false,
                1_000_000_000, //1 second
            )?;
            self.device.reset_fences(std::slice::from_ref(&self.transfer_fence))?;
            self.device.destroy_buffer(staging, None);
            self.device.free_memory(allocation, None);
        }
        Ok(())
    }
}
