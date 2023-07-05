use ash::vk;
use crate::base::Base;
use super::PipelineLayout;
use std::rc::Rc;

pub fn create_layout(base: Rc<Base>) -> Result<PipelineLayout, vk::Result> {
    //Descriptor set layout
    let bindings = [
        //Camera
        *vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::COMPUTE),
        //Nodes
        *vk::DescriptorSetLayoutBinding::builder()
            .binding(1)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::COMPUTE),
        //Meshes
        *vk::DescriptorSetLayoutBinding::builder()
            .binding(2)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::COMPUTE),
        //Primitives
        *vk::DescriptorSetLayoutBinding::builder()
            .binding(3)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::COMPUTE),
        //Draw count
        *vk::DescriptorSetLayoutBinding::builder()
            .binding(4)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::COMPUTE),
        //Draw commands
        *vk::DescriptorSetLayoutBinding::builder()
            .binding(5)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::COMPUTE),
        //Extras
        *vk::DescriptorSetLayoutBinding::builder()
            .binding(6)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
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
        samplers: vec![],
        descriptor_set_layout,
        pipeline_layout,
        create_pipeline: create_pipeline
    })
}

fn create_pipeline(
    layout: &PipelineLayout,
    _extent: vk::Extent2D,
    _render_pass: vk::RenderPass
) -> Result<vk::Pipeline, vk::Result> {
    let base = &layout.base;
    //Shaders
    let code = ash::util::read_spv(
        &mut std::io::Cursor::new(include_bytes!("../../spv/cull.comp.spv"))
    ).unwrap();
    let create_info = vk::ShaderModuleCreateInfo::builder().code(&code);
    let shader = unsafe {
        base.device.create_shader_module(&create_info, None)?
    };
    let shader_stage = *vk::PipelineShaderStageCreateInfo::builder()
        .stage(vk::ShaderStageFlags::COMPUTE)
        .module(shader)
        .name(unsafe {std::ffi::CStr::from_bytes_with_nul_unchecked(b"main\0")});
    let create_info = vk::ComputePipelineCreateInfo::builder()
        .stage(shader_stage)
        .layout(layout.pipeline_layout);
    let pipelines = match unsafe {base.device.create_compute_pipelines(
        base.pipeline_cache,
        std::slice::from_ref(&create_info),
        None
    )} {
        Ok(v) => v,
        Err(e) => {return Err(e.1);}
    };
    //Destroy shader modules
    unsafe {
        base.device.destroy_shader_module(shader, None);
    }
    Ok(pipelines[0])
}
