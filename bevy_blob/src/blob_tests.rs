#[cfg(test)]
mod tests {
    use super::*;

    mod collisions;
    mod corpse;
    mod idle;
    mod jump;
    mod liquid;
    mod locomotion;
    mod splitting;

    fn polygon_self_intersects(particles: &[Particle]) -> bool {
        has_self_intersections(particles)
    }

    #[test]
    fn initial_blob_has_expected_area() {
        let blob = Blob::new(Vec2::ZERO, 50.0);
        let expected = std::f32::consts::PI * 50.0 * 50.0;
        assert!((blob.rest_area - expected).abs() / expected < 0.02);
    }

}
