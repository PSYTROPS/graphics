use nalgebra_glm as glm;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Vertex {
    pub pos: glm::Vec3,
    pub color: glm::Vec3
}
