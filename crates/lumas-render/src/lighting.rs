//! Lighting system — spherical harmonics ambient, directional, and point lights.
//!
//! Lumi's lighting is fully dynamic but artistically driven:
//! - Ambient: spherical harmonics from desktop wallpaper (soft, color-correct)
//! - Key: directional, angle from time-of-day
//! - Crystal fill: point light from forehead crystal, color from CrystalState
//! - Rim: fixed directional light behind Lumas to separate from background

use bytemuck::{Pod, Zeroable};
use glam::{Vec3, Vec4};
use std::sync::Arc;

/// Spherical harmonics ambient lighting (L0 + L1 bands).
#[derive(Debug, Clone, Copy)]
pub struct AmbientSH {
    /// L0 band (DC term).
    pub l0: [f32; 3],
    /// L1 band (3 directional terms).
    pub l1: [[f32; 3]; 3],
}

impl Default for AmbientSH {
    fn default() -> Self {
        // Default: soft white ambient.
        Self {
            l0: [0.5, 0.5, 0.5],
            l1: [[0.0; 3]; 3],
        }
    }
}

/// A directional light.
#[derive(Debug, Clone, Copy)]
pub struct DirectionalLight {
    pub direction: Vec3,
    pub color: Vec3,
    pub intensity: f32,
}

impl DirectionalLight {
    /// Create a key light from time of day (0.0 = midnight, 0.5 = noon).
    pub fn from_time_of_day(t: f32) -> Self {
        let angle = t * std::f32::consts::PI * 2.0;
        let height = angle.sin().max(0.1); // Never fully dark.
        Self {
            direction: Vec3::new(angle.cos() * 0.5, height, angle.sin() * 0.5).normalize(),
            color: Vec3::new(1.0, 0.95, 0.9),
            intensity: height.min(1.0),
        }
    }
}

impl Default for DirectionalLight {
    fn default() -> Self {
        Self {
            direction: Vec3::new(0.5, 0.8, 0.3).normalize(),
            color: Vec3::new(1.0, 0.95, 0.9),
            intensity: 1.0,
        }
    }
}

/// A point light with attenuation.
#[derive(Debug, Clone, Copy)]
pub struct PointLight {
    pub position: Vec3,
    pub color: Vec3,
    pub intensity: f32,
    pub range: f32,
    pub attenuation: LightAttenuation,
}

impl Default for PointLight {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            color: Vec3::new(1.0, 1.0, 1.0),
            intensity: 1.0,
            range: 10.0,
            attenuation: LightAttenuation::InverseSquare,
        }
    }
}

/// Light attenuation model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LightAttenuation {
    Linear,
    Quadratic,
    InverseSquare,
}

impl LightAttenuation {
    pub fn factor(&self, distance: f32) -> f32 {
        match self {
            LightAttenuation::Linear => 1.0 / distance.max(0.01),
            LightAttenuation::Quadratic => 1.0 / (distance * distance).max(0.01),
            LightAttenuation::InverseSquare => 1.0 / (distance * distance).max(0.01),
        }
    }
}

/// The complete lighting setup for a frame.
#[derive(Debug, Clone)]
pub struct LightingScene {
    pub ambient: AmbientSH,
    pub directional: DirectionalLight,
    pub point_lights: Vec<PointLight>,
    pub time_of_day: f32,
}

impl Default for LightingScene {
    fn default() -> Self {
        Self {
            ambient: AmbientSH::default(),
            directional: DirectionalLight::default(),
            point_lights: vec![
                // Crystal fill light.
                PointLight {
                    position: Vec3::new(0.0, 2.0, 1.0),
                    color: Vec3::new(0.4, 0.8, 1.0),
                    intensity: 0.5,
                    range: 5.0,
                    attenuation: LightAttenuation::InverseSquare,
                },
                // Rim light.
                PointLight {
                    position: Vec3::new(0.0, 0.0, -3.0),
                    color: Vec3::new(0.6, 0.7, 1.0),
                    intensity: 0.3,
                    range: 8.0,
                    attenuation: LightAttenuation::InverseSquare,
                },
            ],
            time_of_day: 0.5,
        }
    }
}

impl LightingScene {
    /// Build the GPU lighting uniform buffer.
    pub fn build_ubo(&self) -> LightingUBO {
        // Pack SH coefficients into vec4 array (padded for WGSL alignment).
        let mut ambient_sh = [[0.0f32; 4]; 7];
        ambient_sh[0] = [self.ambient.l0[0], self.ambient.l0[1], self.ambient.l0[2], 0.0];
        for i in 0..3 {
            ambient_sh[1 + i] = [
                self.ambient.l1[i][0],
                self.ambient.l1[i][1],
                self.ambient.l1[i][2],
                0.0,
            ];
        }

        let dir_light = DirectionalLightGPU {
            direction: [
                self.directional.direction.x,
                self.directional.direction.y,
                self.directional.direction.z,
                0.0,
            ],
            color: [
                self.directional.color.x * self.directional.intensity,
                self.directional.color.y * self.directional.intensity,
                self.directional.color.z * self.directional.intensity,
                0.0,
            ],
        };

        // Pack up to 4 point lights.
        let mut point_lights = [PointLightGPU::default(); 4];
        let count = self.point_lights.len().min(4);
        for i in 0..count {
            let pl = &self.point_lights[i];
            point_lights[i] = PointLightGPU {
                position: [pl.position.x, pl.position.y, pl.position.z, 1.0],
                color: [pl.color.x * pl.intensity, pl.color.y * pl.intensity, pl.color.z * pl.intensity, 0.0],
                range: pl.range,
                _pad1: 0.0,
                _pad2: 0.0,
                _pad3: 0.0,
            };
        }

        LightingUBO {
            ambient_sh,
            directional: [dir_light],
            point_lights,
            point_light_count: count as u32,
            _pad: [0u32; 3],
        }
    }
}

/// GPU uniform buffer layout for lighting (matches WGSL struct).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct LightingUBO {
    /// Spherical harmonics coefficients, padded to vec4 (7 slots).
    pub ambient_sh: [[f32; 4]; 7],
    /// Directional light (single).
    pub directional: [DirectionalLightGPU; 1],
    /// Point lights (up to 4).
    pub point_lights: [PointLightGPU; 4],
    /// Number of active point lights.
    pub point_light_count: u32,
    /// Padding.
    pub _pad: [u32; 3],
}

/// GPU representation of a directional light.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct DirectionalLightGPU {
    pub direction: [f32; 4],
    pub color: [f32; 4],
}

/// GPU representation of a point light.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct PointLightGPU {
    pub position: [f32; 4],
    pub color: [f32; 4],
    pub range: f32,
    pub _pad1: f32,
    pub _pad2: f32,
    pub _pad3: f32,
}

impl Default for PointLightGPU {
    fn default() -> Self {
        Self {
            position: [0.0; 4],
            color: [0.0; 4],
            range: 1.0,
            _pad1: 0.0,
            _pad2: 0.0,
            _pad3: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_lighting_scene() {
        let scene = LightingScene::default();
        assert_eq!(scene.point_lights.len(), 2);
        assert!(scene.directional.intensity > 0.0);
    }

    #[test]
    fn test_lighting_ubo_layout() {
        let scene = LightingScene::default();
        let ubo = scene.build_ubo();
        assert_eq!(ubo.point_light_count, 2);
    }

    #[test]
    fn test_time_of_day_light() {
        let noon = DirectionalLight::from_time_of_day(0.5);
        assert!(noon.intensity > 0.5);
        let midnight = DirectionalLight::from_time_of_day(0.0);
        assert!(midnight.intensity >= 0.1); // Never fully dark.
    }

    #[test]
    fn test_ubo_is_bytemuck_castable() {
        use std::mem;
        // LightingUBO layout:
        // ambient_sh: 7 * vec4<f32> = 112 bytes
        // directional: [DirectionalLightGPU; 1] = 32 bytes (2 * vec4<f32>)
        // point_lights: [PointLightGPU; 4] = 4 * 48 = 192 bytes
        // point_light_count: u32 = 4 bytes
        // _pad: [u32; 3] = 12 bytes
        // Total: 112 + 32 + 192 + 16 = 352 bytes
        // Note: WGSL std140 layout requires arrays to be 16-byte aligned.
        // DirectionalLightGPU is 32 bytes, PointLightGPU is 48 bytes.
        let expected_size = 7 * mem::size_of::<[f32; 4]>()   // ambient_sh
            + mem::size_of::<DirectionalLightGPU>()          // directional
            + 4 * mem::size_of::<PointLightGPU>()           // point_lights
            + mem::size_of::<u32>()                          // point_light_count
            + 3 * mem::size_of::<u32>();                     // _pad
        assert_eq!(mem::size_of::<LightingUBO>(), expected_size);
        assert_eq!(mem::size_of::<LightingUBO>(), 352);
    }
}
