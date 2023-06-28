use ash::vk;
use super::Renderer;
use super::{FRAME_COUNT, MAX_TEXTURES, MAX_LIGHTS};
use super::base::Base;
use super::device_scene::DeviceScene;
use super::environment::Environment;
use super::scene::{Scene, PointLight};
use std::rc::Rc;

pub struct Scenery {
    base: Rc<Base>,
    descriptor_pool: vk::DescriptorPool,
    //Descriptor sets: [scenes, skybox]
    pub descriptor_sets: Vec<vk::DescriptorSet>,
    pub scenes: Vec<DeviceScene>,
    pub environment: Environment,
    lights: vk::Buffer,
    buffer_alloc: vk::DeviceMemory
}

impl Scenery {
    pub fn new(
        renderer: &Renderer,
        environment: Environment
    ) -> Result<Scenery, vk::Result> {
        let base = renderer.base.clone();
        let pool_sizes = [
            *vk::DescriptorPoolSize::builder()
                .ty(vk::DescriptorType::SAMPLER)
                .descriptor_count(FRAME_COUNT as u32),
            *vk::DescriptorPoolSize::builder()
                .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(4 * FRAME_COUNT as u32)
        ];
        //Layouts: [mesh, skybox]
        let create_info = vk::DescriptorPoolCreateInfo::builder()
            .max_sets(4 * FRAME_COUNT as u32)
            .pool_sizes(&pool_sizes);
        let descriptor_pool = unsafe {
            base.device.create_descriptor_pool(&create_info, None)
        }?;
        //Lights
        let buffer_info = vk::BufferCreateInfo::builder()
            .size((MAX_LIGHTS * std::mem::size_of::<PointLight>()) as u64)
            .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let (lights_buffers, buffer_alloc) = base.create_buffers(
            std::slice::from_ref(&buffer_info),
            vk::MemoryPropertyFlags::DEVICE_LOCAL
        )?;
        Ok(Self {
            base,
            descriptor_pool,
            descriptor_sets: vec![],
            scenes: vec![],
            environment,
            lights: lights_buffers[0],
            buffer_alloc
        })
    }

    fn recreate_descriptors(&mut self, renderer: &Renderer) -> Result<(), vk::Result> {
        //TODO: Synchronization
        unsafe {
            self.base.device.destroy_descriptor_pool(self.descriptor_pool, None);
        }
        //Create pool
        let scene_set_count = (FRAME_COUNT * self.scenes.len()) as usize;
        let env_set_count = FRAME_COUNT as usize;
        let pool_sizes = [
            *vk::DescriptorPoolSize::builder()
                .ty(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(3 * scene_set_count as u32),
            *vk::DescriptorPoolSize::builder()
                .ty(vk::DescriptorType::SAMPLER)
                .descriptor_count(scene_set_count as u32),
            *vk::DescriptorPoolSize::builder()
                .ty(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count((scene_set_count * MAX_TEXTURES) as u32),
            *vk::DescriptorPoolSize::builder()
                .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(3 * scene_set_count as u32 + env_set_count as u32)
        ];
        let create_info = vk::DescriptorPoolCreateInfo::builder()
            .max_sets((scene_set_count + env_set_count) as u32)
            .pool_sizes(&pool_sizes);
        self.descriptor_pool = unsafe {
            self.base.device.create_descriptor_pool(&create_info, None)
        }?;
        //Allocate descriptor sets
        let layouts: Vec<vk::DescriptorSetLayout> = [
            std::iter::repeat(renderer.layouts[0].descriptor_set_layout).take(scene_set_count),
            std::iter::repeat(renderer.layouts[1].descriptor_set_layout).take(env_set_count)
        ].into_iter().flatten().collect();
        let allocate_info = vk::DescriptorSetAllocateInfo::builder()
            .descriptor_pool(self.descriptor_pool)
            .set_layouts(&layouts);
        self.descriptor_sets = unsafe {
            self.base.device.allocate_descriptor_sets(&allocate_info)
        }?;
        //Update descriptor sets
        //[scenes: [frames: [items: []]], skybox: [frames: []]]
        let mut writes = Vec::<vk::WriteDescriptorSet>::new();
        let mut buffer_infos = Vec::<vk::DescriptorBufferInfo>::new();
        let mut image_infos = Vec::<vk::DescriptorImageInfo>::new();
        //Scene
        let light_buffer_info = vk::DescriptorBufferInfo::builder()
            .buffer(self.lights)
            .offset(0)
            .range(vk::WHOLE_SIZE);
        let cubemap_image_infos = [
            *vk::DescriptorImageInfo::builder()
                .sampler(self.environment.sampler)
                .image_view(self.environment.image_views[1])
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL),
            *vk::DescriptorImageInfo::builder()
                .sampler(self.environment.sampler)
                .image_view(self.environment.image_views[2])
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        ];
        let dfg_image_info = vk::DescriptorImageInfo::builder()
            .sampler(renderer.dfg_lookup_sampler)
            .image_view(renderer.dfg_lookup_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        for (i, scene) in self.scenes.iter().enumerate() {
            //Buffers
            let buffer_offset = buffer_infos.len();
            buffer_infos.push(*vk::DescriptorBufferInfo::builder()
                .buffer(scene.materials)
                .offset(0)
                .range(vk::WHOLE_SIZE)
            ); //Materials
            buffer_infos.extend((0..FRAME_COUNT).map(
                |frame| *vk::DescriptorBufferInfo::builder()
                    .buffer(scene.transforms)
                    .offset((frame * scene.transforms_size) as u64)
                    .range(scene.transforms_size as u64)
            )); //Transformations
            //Images
            let image_offset = image_infos.len();
            image_infos.extend(
                scene.textures.image_views.iter().map(
                    |&image_view| *vk::DescriptorImageInfo::builder()
                        .image_view(image_view)
                        .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                ).chain(std::iter::repeat(*vk::DescriptorImageInfo::builder()
                    .image_view(scene.textures.image_views[0])
                    .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                ).take(MAX_TEXTURES - scene.textures.images.len()))
            ); //Textures
            assert!(image_infos.len() - image_offset == MAX_TEXTURES);
            //Per-frame descriptor writes
            for frame in 0..FRAME_COUNT {
                let descriptor_set = self.descriptor_sets[FRAME_COUNT * i + frame];
                writes.extend_from_slice(&[
                    //Transforms
                    *vk::WriteDescriptorSet::builder()
                        .dst_set(descriptor_set)
                        .dst_binding(0)
                        .dst_array_element(0)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(std::slice::from_ref(
                            &buffer_infos[buffer_offset + frame + 1]
                        )),
                    //Materials
                    *vk::WriteDescriptorSet::builder()
                        .dst_set(descriptor_set)
                        .dst_binding(1)
                        .dst_array_element(0)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(std::slice::from_ref(
                            &buffer_infos[buffer_offset]
                        )),
                    //Textures
                    *vk::WriteDescriptorSet::builder()
                        .dst_set(descriptor_set)
                        .dst_binding(3)
                        .dst_array_element(0)
                        .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                        .image_info(&image_infos[image_offset..image_offset + MAX_TEXTURES]),
                    //Lights
                    *vk::WriteDescriptorSet::builder()
                        .dst_set(descriptor_set)
                        .dst_binding(4)
                        .dst_array_element(0)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(std::slice::from_ref(&light_buffer_info)),
                    //Cubemaps
                    *vk::WriteDescriptorSet::builder()
                        .dst_set(descriptor_set)
                        .dst_binding(5)
                        .dst_array_element(0)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .image_info(&cubemap_image_infos),
                    //DFG lookup
                    *vk::WriteDescriptorSet::builder()
                        .dst_set(descriptor_set)
                        .dst_binding(6)
                        .dst_array_element(0)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .image_info(std::slice::from_ref(&dfg_image_info))
                ])
            }
        }
        //Skybox
        let skybox_image_info = vk::DescriptorImageInfo::builder()
            .sampler(self.environment.sampler)
            .image_view(self.environment.image_views[0])
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        writes.extend((0..FRAME_COUNT).map(
            |frame| *vk::WriteDescriptorSet::builder()
                .dst_set(self.descriptor_sets[scene_set_count + frame])
                .dst_binding(0)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(std::slice::from_ref(&skybox_image_info))
        ));
        assert!(writes.len() == 6 * scene_set_count + env_set_count);
        unsafe {
            self.base.device.update_descriptor_sets(&writes, &[]);
        }
        Ok(())
    }

    pub fn push_scene(&mut self, scene: &Scene, renderer: &Renderer) -> usize {
        let index = self.scenes.len();
        let mut transaction = renderer.transaction.borrow_mut();
        self.scenes.push(DeviceScene::new(
            self.base.clone(),
            &mut transaction,
            scene
        ).unwrap());
        self.recreate_descriptors(renderer).unwrap();
        index
    }

    pub fn update_scene(&mut self, scene: &Scene, index: usize) {
        self.scenes[index].update(scene);
    }

    pub fn scene_descriptors(&self, scene: usize, frame: usize) -> vk::DescriptorSet {
        assert!(scene < self.scenes.len());
        assert!(frame < FRAME_COUNT);
        self.descriptor_sets[scene * FRAME_COUNT + frame]
    }

    pub fn skybox_descriptors(&self, frame: usize) -> vk::DescriptorSet {
        assert!(frame < FRAME_COUNT);
        self.descriptor_sets[self.scenes.len() * FRAME_COUNT + frame]
    }
}

impl Drop for Scenery {
    fn drop(&mut self) {
        unsafe {
            self.base.device.device_wait_idle().unwrap();
            self.base.device.destroy_descriptor_pool(self.descriptor_pool, None);
            self.base.device.destroy_buffer(self.lights, None);
            self.base.device.free_memory(self.buffer_alloc, None);
        }
    }
}
