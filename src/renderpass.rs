use ash::vk;
use super::base::Base;
use super::scene::Vertex;
use std::rc::Rc;

//Remember to match these values in the shaders
pub const COLOR_FORMAT: vk::Format = vk::Format::B8G8R8A8_SRGB;
pub const DEPTH_FORMAT: vk::Format = vk::Format::D32_SFLOAT;
pub const SAMPLE_COUNT: vk::SampleCountFlags = vk::SampleCountFlags::TYPE_4;
pub const MAX_TEXTURES: u32 = 64;
//pub const MAX_LIGHTS: u32 = 64;

pub struct RenderPass {
    base: Rc<Base>,
    pub extent: vk::Extent2D,
    pub render_pass: vk::RenderPass,
    pub mesh_pipeline: MeshPipeline,
    pub skybox_pipeline: SkyboxPipeline
}

pub struct MeshPipeline {
    base: Rc<Base>,
    pub sampler: vk::Sampler,
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    pub pipeline_layout: vk::PipelineLayout,
    pub pipeline: vk::Pipeline
}

pub struct SkyboxPipeline {
    base: Rc<Base>,
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    pub pipeline_layout: vk::PipelineLayout,
    pub pipeline: vk::Pipeline
}

impl SkyboxPipeline {
    pub fn new(base: Rc<Base>, extent: vk::Extent2D, render_pass: vk::RenderPass)
        -> Result<Self, vk::Result> {
        //Descriptor set layout
        let binding = vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT);
        let create_info = vk::DescriptorSetLayoutCreateInfo::builder()
            .bindings(std::slice::from_ref(&binding));
        let descriptor_set_layout = unsafe {
            base.device.create_descriptor_set_layout(&create_info, None)?
        };
        //Pipeline layout
        let push_constant = vk::PushConstantRange::builder()
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(2 * 16 * 4);
        let create_info = vk::PipelineLayoutCreateInfo::builder()
            .set_layouts(std::slice::from_ref(&descriptor_set_layout))
            .push_constant_ranges(std::slice::from_ref(&push_constant));
        let pipeline_layout = unsafe {
            base.device.create_pipeline_layout(&create_info, None)?
        };
        //Pipeline
        //Shaders
        let code = ash::util::read_spv(
            &mut std::io::Cursor::new(include_bytes!("../spv/skybox.vert.spv"))
        ).unwrap();
        let create_info = vk::ShaderModuleCreateInfo::builder().code(&code);
        let vertex_shader = unsafe {
            base.device.create_shader_module(&create_info, None)?
        };
        let code = ash::util::read_spv(
            &mut std::io::Cursor::new(include_bytes!("../spv/skybox.frag.spv"))
        ).unwrap();
        let create_info = vk::ShaderModuleCreateInfo::builder().code(&code);
        let fragment_shader = unsafe {
            base.device.create_shader_module(&create_info, None)?
        };
        let shader_stages = [
            *vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vertex_shader)
                .name(unsafe {std::ffi::CStr::from_bytes_with_nul_unchecked(b"main\0")}),
            *vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(fragment_shader)
                .name(unsafe {std::ffi::CStr::from_bytes_with_nul_unchecked(b"main\0")})
        ];
        //Fixed functions
        //Vertex input
        let vertex_binding = vk::VertexInputBindingDescription::builder()
            .binding(0)
            .stride(12)
            .input_rate(vk::VertexInputRate::VERTEX);
        let vertex_attribute = vk::VertexInputAttributeDescription::builder()
            .location(0)
            .binding(0)
            .format(vk::Format::R32G32B32_SFLOAT)
            .offset(0);
        let vertex_input = vk::PipelineVertexInputStateCreateInfo::builder()
            .vertex_binding_descriptions(std::slice::from_ref(&vertex_binding))
            .vertex_attribute_descriptions(std::slice::from_ref(&vertex_attribute));
        //Input assembly
        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::builder()
            .topology(vk::PrimitiveTopology::TRIANGLE_STRIP);
        //Viewport
        let viewports = [
            *vk::Viewport::builder()
                .width(extent.width as f32)
                .height(extent.height as f32)
                .min_depth(0.0)
                .max_depth(1.0)
        ];
        let scissors = [
            *vk::Rect2D::builder().extent(extent)
        ];
        let viewport = vk::PipelineViewportStateCreateInfo::builder()
            .viewport_count(viewports.len() as u32).viewports(&viewports)
            .scissor_count(scissors.len() as u32).scissors(&scissors);
        //Rasterization
        let rasterization = vk::PipelineRasterizationStateCreateInfo::builder()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .line_width(1.0);
        //Multisampling
        let multisample = vk::PipelineMultisampleStateCreateInfo::builder()
            .rasterization_samples(SAMPLE_COUNT);
        //Depth stencil
        let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::builder()
            .depth_test_enable(true)
            .depth_write_enable(false)
            .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL);
        //Color blending
        let color_blend_attachments = [
            *vk::PipelineColorBlendAttachmentState::builder()
                .blend_enable(false)
                .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
                .dst_color_blend_factor(vk::BlendFactor::DST_ALPHA)
                .color_blend_op(vk::BlendOp::ADD)
                .src_alpha_blend_factor(vk::BlendFactor::ONE)
                .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
                .alpha_blend_op(vk::BlendOp::ADD)
                .color_write_mask(vk::ColorComponentFlags::RGBA)
        ];
        let color_blend = vk::PipelineColorBlendStateCreateInfo::builder()
            .logic_op_enable(false)
            .attachments(&color_blend_attachments);
        //Create pipeline
        let create_infos = [
            *vk::GraphicsPipelineCreateInfo::builder()
                .stages(&shader_stages)
                .vertex_input_state(&vertex_input)
                .input_assembly_state(&input_assembly)
                .viewport_state(&viewport)
                .rasterization_state(&rasterization)
                .multisample_state(&multisample)
                .depth_stencil_state(&depth_stencil)
                .color_blend_state(&color_blend)
                .layout(pipeline_layout)
                .render_pass(render_pass)
                .subpass(0)
        ];
        let pipelines = match unsafe {base.device.create_graphics_pipelines(
            base.pipeline_cache,
            &create_infos,
            None
        )} {
            Ok(v) => v,
            Err(e) => {return Err(e.1);}
        };
        //Destroy shader modules
        unsafe {
            base.device.destroy_shader_module(vertex_shader, None);
            base.device.destroy_shader_module(fragment_shader, None);
        }
        Ok(Self {
            base,
            descriptor_set_layout,
            pipeline_layout,
            pipeline: pipelines[0]
        })
    }
}

impl Drop for SkyboxPipeline {
    fn drop(&mut self) {
        unsafe {
            self.base.device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            self.base.device.destroy_pipeline_layout(self.pipeline_layout, None);
            self.base.device.destroy_pipeline(self.pipeline, None);
        }
    }
}

impl MeshPipeline {
    pub fn new(base: Rc<Base>, extent: vk::Extent2D, render_pass: vk::RenderPass)
        -> Result<MeshPipeline , vk::Result> {
        //Sampler
        let create_info = vk::SamplerCreateInfo::builder()
            .mag_filter(vk::Filter::NEAREST)
            .min_filter(vk::Filter::NEAREST)
            .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
            .address_mode_u(vk::SamplerAddressMode::REPEAT)
            .address_mode_v(vk::SamplerAddressMode::REPEAT)
            .address_mode_w(vk::SamplerAddressMode::REPEAT)
            .anisotropy_enable(false);
        let sampler = unsafe {
            base.device.create_sampler(&create_info, None)?
        };
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
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            //Cubemaps
            *vk::DescriptorSetLayoutBinding::builder()
                .binding(5)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(2)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT)
        ];
        let create_info = vk::DescriptorSetLayoutCreateInfo::builder()
            .bindings(&bindings);
        let descriptor_set_layout = unsafe {
            base.device.create_descriptor_set_layout(&create_info, None)?
        };
        //Pipeline layout
        let push_constant = vk::PushConstantRange::builder()
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(2 * 16 * 4);
        let create_info = vk::PipelineLayoutCreateInfo::builder()
            .set_layouts(std::slice::from_ref(&descriptor_set_layout))
            .push_constant_ranges(std::slice::from_ref(&push_constant));
        let pipeline_layout = unsafe {
            base.device.create_pipeline_layout(&create_info, None)?
        };
        //Pipeline
        //Shaders
        let code = ash::util::read_spv(
            &mut std::io::Cursor::new(include_bytes!("../spv/pbr.vert.spv"))
        ).unwrap();
        let create_info = vk::ShaderModuleCreateInfo::builder().code(&code);
        let vertex_shader = unsafe {
            base.device.create_shader_module(&create_info, None)?
        };
        let code = ash::util::read_spv(
            &mut std::io::Cursor::new(include_bytes!("../spv/pbr.frag.spv"))
        ).unwrap();
        let create_info = vk::ShaderModuleCreateInfo::builder().code(&code);
        let fragment_shader = unsafe {
            base.device.create_shader_module(&create_info, None)?
        };
        let shader_stages = [
            *vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vertex_shader)
                .name(unsafe {std::ffi::CStr::from_bytes_with_nul_unchecked(b"main\0")}),
            *vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(fragment_shader)
                .name(unsafe {std::ffi::CStr::from_bytes_with_nul_unchecked(b"main\0")})
        ];
        //Fixed functions
        //Vertex input
        let vertex_bindings = [
            *vk::VertexInputBindingDescription::builder()
                .binding(0)
                .stride(std::mem::size_of::<Vertex>() as u32)
                .input_rate(vk::VertexInputRate::VERTEX)
        ];
        let vertex_attributes = [
            //Position
            *vk::VertexInputAttributeDescription::builder()
                .location(0)
                .binding(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(0),
            //Normal
            *vk::VertexInputAttributeDescription::builder()
                .location(1)
                .binding(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(12),
            //Texture coordinates
            *vk::VertexInputAttributeDescription::builder()
                .location(2)
                .binding(0)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(24),
            //Material index
            *vk::VertexInputAttributeDescription::builder()
                .location(3)
                .binding(0)
                .format(vk::Format::R32_UINT)
                .offset(32)
        ];
        let vertex_input = vk::PipelineVertexInputStateCreateInfo::builder()
            .vertex_binding_descriptions(&vertex_bindings)
            .vertex_attribute_descriptions(&vertex_attributes);
        //Input assembly
        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::builder()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        //Viewport
        let viewports = [
            *vk::Viewport::builder()
                .width(extent.width as f32)
                .height(extent.height as f32)
                .min_depth(0.0)
                .max_depth(1.0)
        ];
        let scissors = [
            *vk::Rect2D::builder().extent(extent)
        ];
        let viewport = vk::PipelineViewportStateCreateInfo::builder()
            .viewport_count(viewports.len() as u32).viewports(&viewports)
            .scissor_count(scissors.len() as u32).scissors(&scissors);
        //Rasterization
        let rasterization = vk::PipelineRasterizationStateCreateInfo::builder()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::BACK)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .line_width(1.0);
        //Multisampling
        let multisample = vk::PipelineMultisampleStateCreateInfo::builder()
            .rasterization_samples(SAMPLE_COUNT);
        //Depth stencil
        let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::builder()
            .depth_test_enable(true)
            .depth_write_enable(true)
            .depth_compare_op(vk::CompareOp::LESS);
        //Color blending
        let color_blend_attachments = [
            *vk::PipelineColorBlendAttachmentState::builder()
                .blend_enable(false)
                .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
                .dst_color_blend_factor(vk::BlendFactor::DST_ALPHA)
                .color_blend_op(vk::BlendOp::ADD)
                .src_alpha_blend_factor(vk::BlendFactor::ONE)
                .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
                .alpha_blend_op(vk::BlendOp::ADD)
                .color_write_mask(vk::ColorComponentFlags::RGBA)
        ];
        let color_blend = vk::PipelineColorBlendStateCreateInfo::builder()
            .logic_op_enable(false)
            .attachments(&color_blend_attachments);
        //Create pipeline
        let create_infos = [
            *vk::GraphicsPipelineCreateInfo::builder()
                .stages(&shader_stages)
                .vertex_input_state(&vertex_input)
                .input_assembly_state(&input_assembly)
                .viewport_state(&viewport)
                .rasterization_state(&rasterization)
                .multisample_state(&multisample)
                .depth_stencil_state(&depth_stencil)
                .color_blend_state(&color_blend)
                .layout(pipeline_layout)
                .render_pass(render_pass)
                .subpass(0)
        ];
        let pipelines = match unsafe {base.device.create_graphics_pipelines(
            base.pipeline_cache,
            &create_infos,
            None
        )} {
            Ok(v) => v,
            Err(e) => {return Err(e.1);}
        };
        //Destroy shader modules
        unsafe {
            base.device.destroy_shader_module(vertex_shader, None);
            base.device.destroy_shader_module(fragment_shader, None);
        }
        Ok(Self {
            base,
            sampler,
            descriptor_set_layout,
            pipeline_layout,
            pipeline: pipelines[0]
        })
    }
}

impl Drop for MeshPipeline {
    fn drop(&mut self) {
        unsafe {
            self.base.device.destroy_sampler(self.sampler, None);
            self.base.device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            self.base.device.destroy_pipeline_layout(self.pipeline_layout, None);
            self.base.device.destroy_pipeline(self.pipeline, None);
        }
    }
}

impl RenderPass {
    pub fn new(base: Rc<Base>, extent: vk::Extent2D) -> Result<RenderPass, vk::Result> {
        //Render pass
        let attachments = [
            //Color attachment
            *vk::AttachmentDescription::builder()
                .format(COLOR_FORMAT)
                .samples(SAMPLE_COUNT)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL),
            //Resolve attachment
            *vk::AttachmentDescription::builder()
                .format(COLOR_FORMAT)
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL),
            //Depth attachment
            *vk::AttachmentDescription::builder()
                .format(DEPTH_FORMAT)
                .samples(SAMPLE_COUNT)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
        ];
        let references = [
            *vk::AttachmentReference::builder()
                .attachment(0)
                .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL),
            *vk::AttachmentReference::builder()
                .attachment(1)
                .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL),
            *vk::AttachmentReference::builder()
                .attachment(2)
                .layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
        ];
        let subpasses = [
            vk::SubpassDescription::builder()
                .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
                .color_attachments(&references[0..1])
                .resolve_attachments(&references[1..2])
                .depth_stencil_attachment(&references[2])
                .build()
        ];
        let create_info = vk::RenderPassCreateInfo::builder()
            .attachments(attachments.as_slice())
            .subpasses(subpasses.as_slice());
        let render_pass = unsafe {
            base.device.create_render_pass(&create_info, None)
        }?;
        //Pipelines
        let mesh_pipeline = MeshPipeline::new(base.clone(), extent, render_pass)?;
        let skybox_pipeline = SkyboxPipeline::new(base.clone(), extent, render_pass)?;
        //Result
        Ok(Self {
            base,
            extent,
            render_pass,
            mesh_pipeline,
            skybox_pipeline
        })
    }
}

impl Drop for RenderPass {
    fn drop(&mut self) {
        unsafe {
            self.base.device.destroy_render_pass(self.render_pass, None);
        }
    }
}