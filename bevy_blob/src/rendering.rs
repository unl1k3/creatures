use super::*;

pub(super) fn blob_family_color(parent_id: Option<u64>) -> Color {
    const FAMILY_COLORS: [(f32, f32, f32); 6] = [
        (0.30, 0.82, 0.72),
        (0.42, 0.68, 1.00),
        (0.88, 0.48, 0.82),
        (1.00, 0.58, 0.34),
        (0.62, 0.82, 0.34),
        (0.65, 0.52, 0.96),
    ];
    let family_index = parent_id
        .map(|id| (id as usize).wrapping_mul(5).wrapping_add(1) % FAMILY_COLORS.len())
        .unwrap_or(0);
    let (red, green, blue) = FAMILY_COLORS[family_index];
    Color::srgba(red, green, blue, 0.88)
}

pub(super) fn draw_world(mut gizmos: Gizmos, blobs: Res<BlobWorld>, level: Res<Level>) {
    for platform in &level.platforms {
        gizmos.rect_2d(
            platform.center,
            platform.half_size * 2.0,
            Color::srgb(0.18, 0.27, 0.38),
        );
    }

    for (index, active_blob) in blobs.active.iter().enumerate() {
        let blob = &active_blob.body;
        let is_selected = index == blobs.selected;
        let color = blob_family_color(active_blob.parent_id);
        let outline = blob.particles.iter().map(|particle| particle.position);
        gizmos.lineloop_2d(outline, color);
        let center = blob.center();
        if is_selected {
            let outer_outline = blob
                .particles
                .iter()
                .map(|particle| center + (particle.position - center) * 1.045);
            gizmos.lineloop_2d(outer_outline, Color::srgba(1.0, 0.72, 0.08, 0.72));
        }
        let size_scale = blob.size_scale();
        for particle in &blob.particles {
            gizmos.line_2d(
                center,
                particle.position,
                Color::srgba(0.12, 0.55, 0.48, 0.22),
            );
        }
        gizmos.circle_2d(center, 9.0 * size_scale, Color::srgb(0.72, 0.42, 0.95));

        if blob.charge > 0.0 {
            gizmos.arc_2d(
                center,
                std::f32::consts::TAU * blob.charge,
                16.0 * size_scale,
                Color::srgb(1.0, 0.78, 0.24),
            );
        }
    }
}
