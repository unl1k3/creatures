//! Static Avian collider construction from authored level geometry.

use super::*;

pub(super) fn spawn_level_colliders(commands: &mut Commands, level: &Level) {
    for (platform_index, platform) in level.platforms.iter().copied().enumerate() {
        let mut entity = commands.spawn((
            EnvironmentCollider {
                platform_index: Some(platform_index),
                fixture_index: None,
            },
            RigidBody::Static,
            Collider::rectangle(platform.half_size.x * 2.0, platform.half_size.y * 2.0),
            CollisionLayers::new(
                [GameLayer::Environment],
                [
                    GameLayer::LivingBlob,
                    GameLayer::Corpse,
                    GameLayer::Projectile,
                ],
            ),
            Transform::from_xyz(platform.center.x, platform.center.y, 0.0),
        ));
        if platform_index <= 3 {
            entity.insert(AvianMigratedSurface);
        }
        if level
            .counterbalances
            .iter()
            .any(|balance| balance.gate_platform == platform_index)
        {
            entity.insert(CounterbalanceGate {
                platform_index,
                closed_center: platform.center,
            });
        }
        if level
            .counterbalances
            .iter()
            .any(|balance| balance.plate_platform == platform_index)
        {
            entity.insert(CounterbalancePlate {
                platform_index,
                closed_center: platform.center,
            });
        }
    }

    for (fixture_index, vertices) in level.fixtures.iter().enumerate() {
        if let Some(collider) = Collider::convex_hull(vertices.clone()) {
            commands.spawn((
                EnvironmentCollider {
                    platform_index: None,
                    fixture_index: Some(fixture_index),
                },
                AvianMigratedSurface,
                RigidBody::Static,
                collider,
                CollisionLayers::new(
                    [GameLayer::Environment],
                    [
                        GameLayer::LivingBlob,
                        GameLayer::Corpse,
                        GameLayer::Projectile,
                    ],
                ),
            ));
        }
    }
}
