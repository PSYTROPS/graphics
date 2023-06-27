use ash::vk;
use super::base::Base;
use std::rc::Rc;

pub mod mesh;
pub mod skybox;

pub struct PipelineLayout {
    base: Rc<Base>,
    pub samplers: Vec<vk::Sampler>,
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    pub pipeline_layout: vk::PipelineLayout,
    pub create_pipeline: fn(&Self, vk::Extent2D, vk::RenderPass) -> Result<vk::Pipeline, vk::Result>
}

impl Drop for PipelineLayout {
    fn drop(&mut self) {
        unsafe {
            for sampler in &self.samplers {
                self.base.device.destroy_sampler(*sampler, None);
            }
            self.base.device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            self.base.device.destroy_pipeline_layout(self.pipeline_layout, None);
        }
    }
}
