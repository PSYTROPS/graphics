use nalgebra as na;

/*
    Coordinate systems:
    * Worldspace: Right-handed with Y-axis up
    * Viewspace: Right-handed with Y-axis down, looking into +Z axis
    * Clipspace: Defined by Vulkan.
        X & Y axes have range [-1, 1] where (-1, -1) is the upper-left corner of the screen.
        Depth buffer (Z-axis) has range [0, 1].
*/

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Camera {
    pub pos: na::Point3<f32>,
    pub dir: na::UnitVector3<f32>,
    pub up: na::UnitVector3<f32>,
    pub fov: f32, //Field of view (radians)
    pub aspect: f32, //Aspect ratio (width / height)
    pub near: f32, //Near plane distance
    pub far: f32 //Far plane distance
}

impl Camera {
    pub fn new() -> Camera {
        Camera {
            pos: na::Point3::origin(),
            dir: -na::Vector3::z_axis(),
            up: na::Vector3::y_axis(),
            fov: na::RealField::frac_pi_4(),
            aspect: 1.0,
            near: 0.5,
            far: 64.0
        }
    }

    pub fn locomote(&mut self, forward: f32, strafe: f32, vertical: f32) {
        let right = self.dir.cross(&self.up);
        self.pos += forward * self.dir.into_inner();
        self.pos += strafe * right;
        self.pos += vertical * self.up.into_inner();
    }

    pub fn rotate(&mut self, pitch: f32, yaw: f32) {
        let right = na::UnitVector3::new_normalize(self.dir.cross(&self.up));
        let pitch_rot = na::Rotation3::<f32>::from_axis_angle(&right, pitch);
        let yaw_rot = na::Rotation3::<f32>::from_axis_angle(&self.up, yaw);
        self.dir = pitch_rot * yaw_rot * self.dir;
    }

    ///Transforms world-space coordinates to camera space
    pub fn view(&self) -> na::Matrix4<f32> {
        let translate = na::Translation3::new(
            -self.pos.x,
            -self.pos.y,
            -self.pos.z
        );
        let right = na::UnitVector3::new_normalize(self.dir.cross(&self.up));
        let up = na::UnitVector3::new_normalize(right.cross(&self.dir));
        let basis = na::Matrix4::from_iterator([
            right.x, right.y, right.z, 0.0,
            -up.x, -up.y, -up.z, 0.0,
            self.dir.x, self.dir.y, self.dir.z, 0.0,
            0.0, 0.0, 0.0, 1.0
        ]).transpose();
        basis * translate.to_homogeneous()
    }

    ///Transforms camera-space coordinates to clip space
    pub fn projection(&self) -> na::Matrix4<f32> {
        let distance = self.far - self.near;
        let temp = (self.fov / 2.0).tan();
        na::Matrix4::from_iterator([
            1.0 / (self.aspect * temp), 0.0, 0.0, 0.0,
            0.0, 1.0 / temp, 0.0, 0.0,
            0.0, 0.0, self.far / distance, 1.0,
            0.0, 0.0, -(self.near * self.far) / (distance), 0.0
        ])
    }
}
