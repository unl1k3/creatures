from liquid_creature.cloud import (
    PointCloudCreature,
    _polygon_contains,
    concave_hull,
    implicit_contour,
)
from liquid_creature.physics import Vec2
from liquid_creature.world import Obstacle

DT = 1.0 / 120.0


def test_cloud_builds_a_closed_boundary_from_external_points() -> None:
    creature = PointCloudCreature.create(center=Vec2(), radius=70.0)
    assert len(creature.points) > 50
    assert len(creature.outline) == creature.outline_samples
    assert all((point - creature.center).length() < 100.0 for point in creature.outline)


def test_concave_hull_keeps_an_indentation() -> None:
    points = [
        Vec2(0.0, 0.0),
        Vec2(20.0, 0.0),
        Vec2(40.0, 0.0),
        Vec2(40.0, 40.0),
        Vec2(20.0, 18.0),
        Vec2(0.0, 40.0),
        Vec2(10.0, 10.0),
        Vec2(30.0, 10.0),
    ]
    hull = concave_hull(points, alpha_radius=20.0)
    assert len(hull) >= 5
    assert any((point - Vec2(20.0, 18.0)).length() < 1e-6 for point in hull)


def test_implicit_contour_is_closed_and_contains_source_points() -> None:
    points = [
        Vec2(0.0, 0.0),
        Vec2(12.0, 0.0),
        Vec2(6.0, 10.0),
        Vec2(18.0, 10.0),
    ]
    contour = implicit_contour(points, influence_radius=18.0, grid_step=2.5)
    assert len(contour) > 8
    assert all(_polygon_contains(point, contour, margin=0.5) for point in points)


def test_cloud_locomotion_moves_front_before_rear() -> None:
    creature = PointCloudCreature.create(center=Vec2(), radius=70.0)
    initial_center = creature.center
    initial = [Vec2(point.x, point.y) for point in creature.points]
    creature.set_target(Vec2(400.0, 0.0))

    for _ in range(35):
        creature.update(DT, [])

    front = max(range(len(initial)), key=lambda index: initial[index].x)
    rear = min(range(len(initial)), key=lambda index: initial[index].x)
    front_motion = creature.points[front].x - initial[front].x
    rear_motion = creature.points[rear].x - initial[rear].x
    assert front_motion > rear_motion + 1.0
    assert creature.center.x > initial_center.x


def test_cloud_extension_forms_multiple_separate_front_lobes() -> None:
    creature = PointCloudCreature.create(center=Vec2(), radius=70.0)
    initial = [Vec2(point.x, point.y) for point in creature.points]
    creature.set_target(Vec2(500.0, 0.0))
    for _ in range(24):
        creature.update(DT, [])

    lateral_extent = max(abs(point.y) for point in initial)
    lobe_motion: list[float] = []
    gap_motion: list[float] = []
    for index, point in enumerate(initial):
        frontness = (point.x + 70.0) / 140.0
        if frontness < 0.62:
            continue
        lateral = point.y / lateral_extent
        motion = creature.points[index].x - point.x
        if abs(lateral) < 0.10 or 0.38 < abs(lateral) < 0.70:
            lobe_motion.append(motion)
        elif 0.16 < abs(lateral) < 0.34:
            gap_motion.append(motion)
    assert creature.pseudopod_count == 3
    assert lobe_motion and gap_motion
    assert sum(lobe_motion) / len(lobe_motion) > sum(gap_motion) / len(gap_motion)


def test_wall_adhesion_is_local_and_does_not_pin_points() -> None:
    creature = PointCloudCreature.create(center=Vec2(), radius=45.0)
    wall = Obstacle(40.0, -80.0, 80.0, 80.0)
    near_vector, near_weight = creature._wall_adhesion(Vec2(25.0, 0.0), [wall])
    far_vector, far_weight = creature._wall_adhesion(Vec2(0.0, 0.0), [wall])
    assert near_weight > 0.0
    assert near_vector.x > 0.0
    assert far_weight == 0.0
    assert far_vector.length() == 0.0


def test_point_rebalancing_conserves_total_mass() -> None:
    creature = PointCloudCreature.create(center=Vec2(), radius=45.0)
    initial_mass = sum(creature.masses)
    creature.points.append(Vec2(creature.points[0].x, creature.points[0].y))
    creature.masses.append(0.65)
    expected_mass = sum(creature.masses)
    creature._rebalance_points()
    assert abs(sum(creature.masses) - expected_mass) < 1e-9
    assert sum(creature.masses) > initial_mass


def test_free_locomotion_does_not_turn_cloud_into_a_filament() -> None:
    creature = PointCloudCreature.create(center=Vec2(), radius=70.0)
    creature.set_target(Vec2(500.0, 0.0))
    for _ in range(240):
        creature.update(DT, [])
    width = max(point.x for point in creature.points) - min(
        point.x for point in creature.points
    )
    height = max(point.y for point in creature.points) - min(
        point.y for point in creature.points
    )
    assert width / height < 1.55


def test_rebuilt_outline_contains_every_cloud_point() -> None:
    creature = PointCloudCreature.create(center=Vec2(), radius=70.0)
    creature.set_target(Vec2(500.0, 40.0))
    for _ in range(90):
        creature.update(DT, [])
    assert all(
        _polygon_contains(point, creature.outline, margin=creature.spacing * 0.1)
        for point in creature.points
    )


def test_cloud_points_do_not_enter_obstacles() -> None:
    creature = PointCloudCreature.create(center=Vec2(), radius=45.0)
    obstacle = Obstacle(35.0, -30.0, 80.0, 30.0)

    for _ in range(8):
        creature.update(DT, [obstacle])

    assert all(
        not (
            obstacle.left - 2.0 < point.x < obstacle.right + 2.0
            and obstacle.top - 2.0 < point.y < obstacle.bottom + 2.0
        )
        for point in creature.points
    )
