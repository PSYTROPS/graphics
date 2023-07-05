use ash::vk;
use super::Renderer;
use super::camera::Camera;
use super::{FRAME_COUNT, MAX_TEXTURES, MAX_LIGHTS};
use super::base::Base;
use super::device_scene::DeviceScene;
use super::environment::Environment;
use super::scene::{Scene, PointLight};
use std::rc::Rc;

const UNIFORM_SIZE: usize = 2 * 64 + 16;

pub struct SceneSet {
    base: Rc<Base>,
    pub camera: Camera,
    descriptor_pool: vk::DescriptorPool,
    //Descriptor sets: [scenes, skybox]
    pub descriptor_sets: Vec<vk::DescriptorSet>,
    pub scenes: Vec<DeviceScene>,
    pub environment: Environment,
    pub lights: [PointLight; MAX_LIGHTS],
    pub camera_uniform_size: usize,
    pub lights_buffer: vk::Buffer,
    pub camera_buffer: vk::Buffer,
    buffer_alloc: vk::DeviceMemory,
    buffer_descriptors: [vk::DescriptorBufferInfo; 2 * FRAME_COUNT]
}

impl SceneSet {
    pub fn new(
        renderer: &Renderer,
        environment: Environment
    ) -> Result<SceneSet, vk::Result> {
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
        //Buffers
        let lights = [PointLight::default(); MAX_LIGHTS];
        let alignment = base.physical_device_properties.limits.min_uniform_buffer_offset_alignment as usize;
        let uniform_size = (UNIFORM_SIZE + alignment - 1) & !(alignment - 1);
        let buffer_sizes = [
            MAX_LIGHTS * std::mem::size_of::<PointLight>(),
            uniform_size
        ];
        let create_infos = [
            //Lights
            *vk::BufferCreateInfo::builder()
                .size((FRAME_COUNT * buffer_sizes[0]) as u64)
                .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE),
            //Camera
            *vk::BufferCreateInfo::builder()
                .size((FRAME_COUNT * buffer_sizes[1]) as u64)
                .usage(vk::BufferUsageFlags::UNIFORM_BUFFER | vk::BufferUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
        ];
        let (buffers, buffer_alloc) = base.create_buffers(
            &create_infos,
            vk::MemoryPropertyFlags::DEVICE_LOCAL
        )?;
        //Buffer descriptors
        let mut buffer_descriptors = [vk::DescriptorBufferInfo::default(); 2 * FRAME_COUNT];
        for b in 0..2 {
            let buffer = buffers[b];
            let size = buffer_sizes[b];
            for f in 0..FRAME_COUNT {
                buffer_descriptors[b * FRAME_COUNT + f] = *vk::DescriptorBufferInfo::builder()
                    .buffer(buffer)
                    .offset((f * size) as u64)
                    .range(size as u64);
            }
        }
        Ok(Self {
            base,
            camera: Camera::new(),
            descriptor_pool,
            descriptor_sets: vec![],
            scenes: vec![],
            environment,
            lights,
            camera_uniform_size: uniform_size,
            lights_buffer: buffers[0],
            camera_buffer: buffers[1],
            buffer_alloc,
            buffer_descriptors
        })
    }

    fn recreate_descriptors(&mut self, renderer: &Renderer) -> Result<(), vk::Result> {
        unsafe {
            self.base.device.queue_wait_idle(self.base.graphics_queue)?;
            self.base.device.destroy_descriptor_pool(self.descriptor_pool, None);
        }
        //Create pool
        let pbr_set_count = FRAME_COUNT * self.scenes.len();
        let cull_set_count = FRAME_COUNT * self.scenes.len();
        let env_set_count = FRAME_COUNT;
        //TODO: Automatic pool size counting
        let pool_sizes = [
            *vk::DescriptorPoolSize::builder()
                .ty(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count((pbr_set_count + cull_set_count + env_set_count) as u32),
            *vk::DescriptorPoolSize::builder()
                .ty(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count((5 * pbr_set_count + 6 * cull_set_count) as u32),
            *vk::DescriptorPoolSize::builder()
                .ty(vk::DescriptorType::SAMPLER)
                .descriptor_count(pbr_set_count as u32),
            *vk::DescriptorPoolSize::builder()
                .ty(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count((pbr_set_count * MAX_TEXTURES) as u32),
            *vk::DescriptorPoolSize::builder()
                .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(3 * pbr_set_count as u32 + env_set_count as u32)
        ];
        let create_info = vk::DescriptorPoolCreateInfo::builder()
            .max_sets((pbr_set_count + cull_set_count + env_set_count) as u32)
            .pool_sizes(&pool_sizes);
        self.descriptor_pool = unsafe {
            self.base.device.create_descriptor_pool(&create_info, None)
        }?;
        //Allocate descriptor sets
        let layouts: Vec<vk::DescriptorSetLayout> = [
            std::iter::repeat(renderer.layouts[0].descriptor_set_layout).take(pbr_set_count),
            std::iter::repeat(renderer.cull_layout.descriptor_set_layout).take(cull_set_count),
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
        //PBR pipeline
        for (i, scene) in self.scenes.iter().enumerate() {
            //Per-frame descriptor writes
            for frame in 0..FRAME_COUNT {
                let descriptor_set = self.descriptor_sets[FRAME_COUNT * i + frame];
                writes.extend_from_slice(&[
                    //Uniforms
                    *vk::WriteDescriptorSet::builder()
                        .dst_set(descriptor_set)
                        .dst_binding(0)
                        .dst_array_element(0)
                        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                        .buffer_info(std::slice::from_ref(
                            &self.buffer_descriptors[FRAME_COUNT + frame]
                        )),
                    //Primitives
                    *vk::WriteDescriptorSet::builder()
                        .dst_set(descriptor_set)
                        .dst_binding(1)
                        .dst_array_element(0)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(std::slice::from_ref(
                            &scene.buffer_descriptors[0]
                        )),
                    //Nodes
                    *vk::WriteDescriptorSet::builder()
                        .dst_set(descriptor_set)
                        .dst_binding(2)
                        .dst_array_element(0)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(std::slice::from_ref(
                            &scene.buffer_descriptors[4 + frame]
                        )),
                    //Draw command extras
                    *vk::WriteDescriptorSet::builder()
                        .dst_set(descriptor_set)
                        .dst_binding(3)
                        .dst_array_element(0)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(std::slice::from_ref(
                            &scene.buffer_descriptors[4 + FRAME_COUNT + frame]
                        )),
                    //Materials
                    *vk::WriteDescriptorSet::builder()
                        .dst_set(descriptor_set)
                        .dst_binding(4)
                        .dst_array_element(0)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(std::slice::from_ref(
                            &scene.buffer_descriptors[2]
                        )),
                    //Textures
                    *vk::WriteDescriptorSet::builder()
                        .dst_set(descriptor_set)
                        .dst_binding(6)
                        .dst_array_element(0)
                        .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                        .image_info(&scene.image_descriptors),
                    //Lights
                    *vk::WriteDescriptorSet::builder()
                        .dst_set(descriptor_set)
                        .dst_binding(7)
                        .dst_array_element(0)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(std::slice::from_ref(
                            &self.buffer_descriptors[frame]
                        )),
                    //Cubemaps
                    *vk::WriteDescriptorSet::builder()
                        .dst_set(descriptor_set)
                        .dst_binding(8)
                        .dst_array_element(0)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .image_info(&self.environment.descriptors[1..=2]),
                    //DFG lookup
                    *vk::WriteDescriptorSet::builder()
                        .dst_set(descriptor_set)
                        .dst_binding(9)
                        .dst_array_element(0)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .image_info(std::slice::from_ref(&renderer.dfg_descriptor))
                ])
            }
        }

        //Compute culling pipeline
        for (i, scene) in self.scenes.iter().enumerate() {
            //Per-frame descriptor writes
            for frame in 0..FRAME_COUNT {
                let descriptor_set = self.descriptor_sets[pbr_set_count + FRAME_COUNT * i + frame];
                writes.extend_from_slice(&[
                    //Uniforms
                    *vk::WriteDescriptorSet::builder()
                        .dst_set(descriptor_set)
                        .dst_binding(0)
                        .dst_array_element(0)
                        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                        .buffer_info(std::slice::from_ref(
                            &self.buffer_descriptors[FRAME_COUNT + frame]
                        )),
                    //Nodes
                    *vk::WriteDescriptorSet::builder()
                        .dst_set(descriptor_set)
                        .dst_binding(1)
                        .dst_array_element(0)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(std::slice::from_ref(
                            &scene.buffer_descriptors[4 + frame]
                        )),
                    //Meshes
                    *vk::WriteDescriptorSet::builder()
                        .dst_set(descriptor_set)
                        .dst_binding(2)
                        .dst_array_element(0)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(std::slice::from_ref(
                            &scene.buffer_descriptors[1]
                        )),
                    //Primitives
                    *vk::WriteDescriptorSet::builder()
                        .dst_set(descriptor_set)
                        .dst_binding(3)
                        .dst_array_element(0)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(std::slice::from_ref(
                            &scene.buffer_descriptors[3]
                        )),
                    //Draw count
                    *vk::WriteDescriptorSet::builder()
                        .dst_set(descriptor_set)
                        .dst_binding(4)
                        .dst_array_element(0)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(std::slice::from_ref(
                            &scene.buffer_descriptors[4 + 2 * FRAME_COUNT + frame]
                        )),
                    //Draw commands
                    *vk::WriteDescriptorSet::builder()
                        .dst_set(descriptor_set)
                        .dst_binding(5)
                        .dst_array_element(0)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(std::slice::from_ref(
                            &scene.buffer_descriptors[4 + 3 * FRAME_COUNT + frame]
                        )),
                    //Extras
                    *vk::WriteDescriptorSet::builder()
                        .dst_set(descriptor_set)
                        .dst_binding(6)
                        .dst_array_element(0)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(std::slice::from_ref(
                            &scene.buffer_descriptors[4 + FRAME_COUNT + frame]
                        ))
                ])
            }
        }

        //Skybox pipeline
        //Camera
        writes.extend((0..FRAME_COUNT).map(
            |frame| *vk::WriteDescriptorSet::builder()
                .dst_set(self.descriptor_sets[pbr_set_count + cull_set_count + frame])
                .dst_binding(0)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .buffer_info(
                    std::slice::from_ref(&self.buffer_descriptors[FRAME_COUNT + frame])
                )
        ));
        //Skybox image
        writes.extend((0..FRAME_COUNT).map(
            |frame| *vk::WriteDescriptorSet::builder()
                .dst_set(self.descriptor_sets[pbr_set_count + cull_set_count + frame])
                .dst_binding(1)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(std::slice::from_ref(&self.environment.descriptors[0]))
        ));

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

    pub fn cull_descriptors(&self, scene: usize, frame: usize) -> vk::DescriptorSet {
        assert!(scene < self.scenes.len());
        assert!(frame < FRAME_COUNT);
        self.descriptor_sets[(self.scenes.len() + scene) * FRAME_COUNT + frame]
    }

    pub fn skybox_descriptors(&self, frame: usize) -> vk::DescriptorSet {
        assert!(frame < FRAME_COUNT);
        self.descriptor_sets[2 * self.scenes.len() * FRAME_COUNT + frame]
    }
}

impl Drop for SceneSet {
    fn drop(&mut self) {
        unsafe {
            self.base.device.device_wait_idle().unwrap();
            self.base.device.destroy_descriptor_pool(self.descriptor_pool, None);
            self.base.device.destroy_buffer(self.lights_buffer, None);
            self.base.device.destroy_buffer(self.camera_buffer, None);
            self.base.device.free_memory(self.buffer_alloc, None);
        }
    }
}
