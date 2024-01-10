use cgmath::{Vector3, Matrix4, Rad};
use wgpu::util::DeviceExt;

pub struct Transform {
    pub translation: Vector3<f32>,
    pub rotation: Vector3<f32>,
    pub scaling: Vector3<f32>,
}

impl Transform {
    pub fn new() -> Self {
        Transform {
            translation: Vector3::new(0.0, 0.0, 0.0),
            rotation: Vector3::new(0.0, 0.0, 0.0),
            scaling: Vector3::new(1.0, 1.0, 1.0),
        }
    }

    pub fn build_transform_matrix(&self) -> Matrix4<f32> {
        let translation_mat = Matrix4::from_translation(self.translation);
        let rotation_x_mat = Matrix4::from_angle_x(Rad(self.rotation.x));
        let rotation_y_mat = Matrix4::from_angle_y(Rad(self.rotation.y));
        let rotation_z_mat = Matrix4::from_angle_z(Rad(self.rotation.z));
        let scaling_mat = Matrix4::from_nonuniform_scale(self.scaling.x, self.scaling.y, self.scaling.z);

        let model_mat = translation_mat * rotation_z_mat * rotation_y_mat * rotation_x_mat * scaling_mat;

        model_mat
    }
}

// We need this for Rust to store our data correctly for the shaders
#[repr(C)]
// This is so we can store this in a buffer
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TransformUniform {
    // We can't use cgmath with bytemuck directly, so we'll have
    // to convert the Matrix4 into a 4x4 f32 array
    pub model_transform: [[f32; 4]; 4],
}

impl TransformUniform {
    pub fn new() -> Self {
        use cgmath::SquareMatrix;
        Self {
            model_transform: cgmath::Matrix4::identity().into(),
        }
    }
    
    pub fn update_model_transform(&mut self, transform: &Transform) {
        self.model_transform = transform.build_transform_matrix().into();
    }

    pub fn create_model_transform_buffer(&self, device: &wgpu::Device) -> wgpu::Buffer {
        let transform_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Transform Buffer"),
                contents: bytemuck::cast_slice(&[*self]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }
        );
        transform_buffer
    }

}


pub fn create_transform_bg_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let transform_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }
        ],
        label: Some("Transform_bind_group_layout"),
    });
    transform_bind_group_layout
}        


pub fn create_transform_bind_group(device: &wgpu::Device, transform_bind_group_layout: &wgpu::BindGroupLayout, transform_buffer: wgpu::Buffer) -> wgpu::BindGroup {

    let transform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        layout: transform_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: transform_buffer.as_entire_binding(),
            }
        ],
        label: Some("Transform_bind_group"),
    });
    transform_bind_group

}

