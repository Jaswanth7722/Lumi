//! Camera system with orthographic projection for the character.
//!
//! Lumas uses an orthographic camera — the character is a 2D-projected 3D object
//! on a flat desktop surface, not a 3D scene with perspective.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};

/// Camera projection type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CameraProjection {
    Orthographic {
        left: f32, right: f32,
        bottom: f32, top: f32,
        near: f32, far: f32,
    },
    Perspective {
        fov_y_radians: f32,
        aspect: f32,
        near: f32, far: f32,
    },
}

impl CameraProjection {
    /// Compute the projection matrix.
    pub fn to_matrix(&self) -> Mat4 {
        match self {
            CameraProjection::Orthographic { left, right, bottom, top, near, far } => {
                Mat4::orthographic_rh(*left, *right, *bottom, *top, *near, *far)
            }
            CameraProjection::Perspective { fov_y_radians, aspect, near, far } => {
                Mat4::perspective_rh(*fov_y_radians, *aspect, *near, *far)
            }
        }
    }
}

/// Viewport dimensions.
#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// The camera system.
#[derive(Clone)]
pub struct Camera {
    /// Camera position in world space.
    pub position: Vec3,
    /// Camera target (look-at point).
    pub target: Vec3,
    /// Up vector.
    pub up: Vec3,
    /// Projection configuration.
    pub projection: CameraProjection,
    /// Viewport dimensions.
    pub viewport: Viewport,
}

impl Camera {
    /// Create a new orthographic camera for character rendering.
    pub fn orthographic(width: f32, height: f32) -> Self {
        Self {
            position: Vec3::new(0.0, 0.0, 10.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            projection: CameraProjection::Orthographic {
                left: -width / 2.0,
                right: width / 2.0,
                bottom: -height / 2.0,
                top: height / 2.0,
                near: -100.0,
                far: 100.0,
            },
            viewport: Viewport {
                x: 0,
                y: 0,
                width: width as u32,
                height: height as u32,
            },
        }
    }

    /// Compute the view matrix.
    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.position, self.target, self.up)
    }

    /// Compute the combined view-projection matrix.
    pub fn view_proj_matrix(&self) -> Mat4 {
        self.projection.to_matrix() * self.view_matrix()
    }

    /// Build the camera uniform buffer data.
    pub fn build_ubo(&self, time_seconds: f32) -> CameraUBO {
        let view_proj = self.view_proj_matrix();
        let view = self.view_matrix();
        let proj = self.projection.to_matrix();

        CameraUBO {
            view_proj: view_proj.to_cols_array_2d(),
            view: view.to_cols_array_2d(),
            proj: proj.to_cols_array_2d(),
            camera_pos: [self.position.x, self.position.y, self.position.z, 1.0],
            viewport_size: [self.viewport.width as f32, self.viewport.height as f32],
            time_seconds,
            _pad: 0.0,
        }
    }

    /// Resize the camera viewport.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.viewport = Viewport {
            x: 0,
            y: 0,
            width,
            height,
        };
        // Update orthographic projection bounds to match aspect ratio.
        if let CameraProjection::Orthographic {
            ref mut left,
            ref mut right,
            ref mut bottom,
            ref mut top,
            ..
        } = self.projection
        {
            let aspect = width as f32 / height as f32;
            let half_height = 5.0; // Fixed half-height, adjust width to match aspect.
            let half_width = half_height * aspect;
            *left = -half_width;
            *right = half_width;
            *bottom = -half_height;
            *top = half_height;
        }
    }
}

impl std::fmt::Debug for Camera {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Camera")
            .field("pos", &self.position)
            .field("target", &self.target)
            .field("viewport", &(self.viewport.width, self.viewport.height))
            .finish()
    }
}

/// GPU uniform buffer layout for the camera.
/// Must match `struct CameraUBO` in WGSL shaders exactly.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct CameraUBO {
    /// Combined view × projection matrix, column-major.
    pub view_proj: [[f32; 4]; 4],
    /// View matrix (world → view space).
    pub view: [[f32; 4]; 4],
    /// Projection matrix (view → clip space).
    pub proj: [[f32; 4]; 4],
    /// Camera position in world space (xyz, w=1.0).
    pub camera_pos: [f32; 4],
    /// Viewport size (width, height).
    pub viewport_size: [f32; 2],
    /// Elapsed time in seconds (for animated shaders).
    pub time_seconds: f32,
    /// Padding to 16-byte alignment.
    pub _pad: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orthographic_camera_creation() {
        let camera = Camera::orthographic(800.0, 600.0);
        assert_eq!(camera.viewport.width, 800);
        assert_eq!(camera.viewport.height, 600);
    }

    #[test]
    fn test_view_proj_matrix_is_finite() {
        let camera = Camera::orthographic(800.0, 600.0);
        let vp = camera.view_proj_matrix();
        // Check that all elements are finite (no NaN/Inf from bad math).
        for i in 0..4 {
            for j in 0..4 {
                assert!(vp.to_cols_array()[i * 4 + j].is_finite());
            }
        }
    }

    #[test]
    fn test_ubo_creation() {
        let camera = Camera::orthographic(800.0, 600.0);
        let ubo = camera.build_ubo(1.0);
        assert!((ubo.time_seconds - 1.0).abs() < f32::EPSILON);
        assert_eq!(ubo.viewport_size[0], 800.0);
    }

    #[test]
    fn test_resize_updates_viewport() {
        let mut camera = Camera::orthographic(1920.0, 1080.0);
        camera.resize(800, 600);
        assert_eq!(camera.viewport.width, 800);
        assert_eq!(camera.viewport.height, 600);
    }
}
