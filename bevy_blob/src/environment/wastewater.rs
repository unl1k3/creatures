//! Visual and gameplay-neutral state for wastewater impacts and ripples.

use super::*;

impl WastewaterEffects {
    pub(crate) fn emit(
        &mut self,
        area_index: usize,
        position: Vec2,
        source_radius: f32,
        strength: f32,
    ) {
        let variation = self.next_variation();
        let strength = strength.clamp(0.2, 1.8);
        self.pending.push(WastewaterImpact {
            area_index,
            position,
            source_radius,
            variation,
        });
        self.push_ripple(area_index, position.x, source_radius, strength);
    }

    pub(crate) fn emit_ripple(
        &mut self,
        area_index: usize,
        position: Vec2,
        source_radius: f32,
        strength: f32,
    ) -> f32 {
        let variation = self.next_variation();
        self.push_ripple(
            area_index,
            position.x,
            source_radius,
            strength.clamp(0.2, 1.8),
        );
        variation
    }

    pub(crate) fn advance(&mut self, dt: f32) {
        for ripple in &mut self.ripples {
            ripple.age += dt;
        }
        self.ripples.retain(|ripple| ripple.age < ripple.duration);
    }

    pub(crate) fn surface_offset(&self, area_index: usize, world_x: f32) -> f32 {
        self.ripples
            .iter()
            .filter(|ripple| ripple.area_index == area_index)
            .map(|ripple| {
                let distance = (world_x - ripple.center_x).abs();
                let front = ripple.age * 105.0;
                let band = (1.0 - (distance - front).abs() / 52.0).max(0.0);
                let decay = (1.0 - ripple.age / ripple.duration).powi(2);
                let wave = ((distance - front) * 0.15).sin() * band * decay * ripple.amplitude;
                let initial_dip = (1.0 - ripple.age / 0.22).max(0.0)
                    * (-ripple.amplitude * 0.55)
                    * (1.0 - distance / 36.0).max(0.0);
                wave + initial_dip
            })
            .sum()
    }

    fn push_ripple(&mut self, area_index: usize, center_x: f32, source_radius: f32, strength: f32) {
        self.ripples.push(WastewaterRipple {
            area_index,
            center_x,
            age: 0.0,
            duration: 1.45,
            amplitude: (source_radius * 0.42 * strength).clamp(2.0, 13.0),
        });
    }

    fn next_variation(&mut self) -> f32 {
        self.variation_serial = self.variation_serial.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.variation_serial;
        value ^= value >> 30;
        value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value ^= value >> 27;
        ((value >> 40) & 0xffff) as f32 / 65_535.0
    }
}
