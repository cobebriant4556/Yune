use glam::Vec3;

#[derive(Debug, Clone, Copy)]
pub struct AmbientState {
    pub base: Vec3,
    pub outdoor_delta: Vec3,
}

impl AmbientState {
    // RenderView::presetLighting, 0x1051e828f..0x1051e8306. LightingProp
    // accessors confirm Ambient at +208 and OutdoorAmbient at +256.
    pub fn prepare(ambient: Vec3, outdoor: Vec3) -> Self {
        let base = ambient * ambient;
        Self { base, outdoor_delta: (outdoor * outdoor - base).max(Vec3::ZERO) }
    }

    pub fn unoccluded(self) -> Vec3 {
        self.base + self.outdoor_delta
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FogState {
    pub color: Vec3,
    pub parameters: [f32; 4],
    pub end: f32,
    pub inverse_span: f32,
}

impl FogState {
    // SceneManager::setFog, 0x10520f1e8. Preserve the two separate products
    // used by its shader constants, including the equal-endpoint mask.
    pub fn prepare(color: Vec3, start: f32, end: f32) -> Self {
        let reciprocal = 1.0 / (end - start);
        Self {
            color,
            parameters: [reciprocal * end, 0.0, -reciprocal, 1.0],
            end,
            inverse_span: if start == end { 0.0 } else { reciprocal },
        }
    }

    pub fn amount(self, distance: f32) -> f32 {
        // Yune raster adapter: avoid 0*infinity at a zero-width fog boundary.
        // This guard is not a reconstruction of the missing fragment shader.
        if self.inverse_span == 0.0 {
            return if distance >= self.end { 1.0 } else { 0.0 };
        }
        let visibility = self.parameters[0] + distance * self.parameters[2];
        1.0 - visibility.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambient_squares_before_subtracting_and_clamps_per_channel() {
        let state = AmbientState::prepare(Vec3::new(0.5, 0.75, 0.0), Vec3::new(0.75, 0.5, 1.0));
        assert_eq!(state.base, Vec3::new(0.25, 0.5625, 0.0));
        assert_eq!(state.outdoor_delta, Vec3::new(0.3125, 0.0, 1.0));
        assert_eq!(state.unoccluded(), Vec3::new(0.5625, 0.5625, 1.0));
    }

    #[test]
    fn equal_ambient_has_no_outdoor_delta() {
        let color = Vec3::new(0.25, 0.5, 0.75);
        let state = AmbientState::prepare(color, color);
        assert_eq!(state.outdoor_delta, Vec3::ZERO);
        assert_eq!(state.unoccluded(), color * color);
    }

    #[test]
    fn fog_constants_and_raster_endpoints() {
        let fog = FogState::prepare(Vec3::new(0.1, 0.2, 0.3), 4.0, 12.0);
        assert_eq!(fog.parameters, [1.5, 0.0, -0.125, 1.0]);
        assert_eq!(fog.inverse_span, 0.125);
        assert_eq!(fog.color, Vec3::new(0.1, 0.2, 0.3));
        for (distance, expected) in [(0.0, 0.0), (4.0, 0.0), (8.0, 0.5), (12.0, 1.0), (20.0, 1.0)] {
            assert_eq!(fog.amount(distance), expected);
        }
    }

    #[test]
    fn equal_fog_endpoints_keep_source_mask_and_finite_raster_output() {
        let fog = FogState::prepare(Vec3::ONE, 4.0, 4.0);
        assert_eq!(fog.inverse_span.to_bits(), 0);
        assert_eq!(fog.parameters[0], f32::INFINITY);
        assert_eq!(fog.parameters[2], f32::NEG_INFINITY);
        assert_eq!(fog.amount(3.0), 0.0);
        assert_eq!(fog.amount(4.0), 1.0);
        assert_eq!(FogState::prepare(Vec3::ONE, 0.0, 0.0).amount(0.0), 1.0);
    }
}
