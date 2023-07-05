use ash::vk;
use crate::base::Base;
use crate::{SAMPLE_COUNT, MAX_TEXTURES};
use crate::scene::Vertex;
use super::PipelineLayout;
use std::rc::Rc;

pub fn create_layout(base: Rc<Base>) -> Result<PipelineLayout, vk::Result> {
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
        //Camera
        *vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
        //Primitives
        *vk::DescriptorSetLayoutBinding::builder()
            .binding(1)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX),
        //Nodes
        *vk::DescriptorSetLayoutBinding::builder()
            .binding(2)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX),
        //Draw command extras
        *vk::DescriptorSetLayoutBinding::builder()
            .binding(3)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX),
        //Materials
        *vk::DescriptorSetLayoutBinding::builder()
            .binding(4)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        //Sampler
        *vk::DescriptorSetLayoutBinding::builder()
            .binding(5)
            .descriptor_type(vk::DescriptorType::SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .immutable_samplers(std::slice::from_ref(&sampler)),
        //Textures
        *vk::DescriptorSetLayoutBinding::builder()
            .binding(6)
            .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
            .descriptor_count(MAX_TEXTURES as u32)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        //Lights
        *vk::DescriptorSetLayoutBinding::builder()
            .binding(7)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        //Cubemaps
        *vk::DescriptorSetLayoutBinding::builder()
            .binding(8)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(2)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        //DFG lookup
        *vk::DescriptorSetLayoutBinding::builder()
            .binding(9)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
    ];
    let create_info = vk::DescriptorSetLayoutCreateInfo::builder()
        .bindings(&bindings);
    let descriptor_set_layout = unsafe {
        base.device.create_descriptor_set_layout(&create_info, None)?
    };
    //Pipeline layout
    let create_info = vk::PipelineLayoutCreateInfo::builder()
        .set_layouts(std::slice::from_ref(&descriptor_set_layout));
    let pipeline_layout = unsafe {
        base.device.create_pipeline_layout(&create_info, None)?
    };
    Ok(PipelineLayout {
        base,
        samplers: vec![sampler],
        descriptor_set_layout,
        pipeline_layout,
        create_pipeline: create_pipeline
    })
}

fn create_pipeline(
    layout: &PipelineLayout,
    extent: vk::Extent2D,
    render_pass: vk::RenderPass
) -> Result<vk::Pipeline, vk::Result> {
    let base = &layout.base;
    //Pipeline
    //Shaders
    let code = ash::util::read_spv(
        &mut std::io::Cursor::new(include_bytes!("../../spv/pbr.vert.spv"))
    ).unwrap();
    let create_info = vk::ShaderModuleCreateInfo::builder().code(&code);
    let vertex_shader = unsafe {
        base.device.create_shader_module(&create_info, None)?
    };
    let code = ash::util::read_spv(
        &mut std::io::Cursor::new(include_bytes!("../../spv/pbr.frag.spv"))
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
            .offset(24)
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
            .layout(layout.pipeline_layout)
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
    Ok(pipelines[0])
}
