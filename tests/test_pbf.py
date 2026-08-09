from liquid_creature.pbf import PBFCreature
from liquid_creature.physics import Vec2
from liquid_creature.world import Obstacle

DT = 1.0 / 120.0


def test_pbf_creation_has_stable_particle_mass() -> None:
    creature = PBFCreature.create(center=Vec2())
    initial_count = creature.particle_count
    for _ in range(120):
        creature.step(DT, [])
    assert creature.particle_count == initial_count
    assert 0.75 < creature.diagnostics.average_density_ratio < 1.15
    assert creature.diagnostics.connected_particles == initial_count


def test_pbf_body_does_not_drift_without_input() -> None:
    creature = PBFCreature.create(center=Vec2())
    initial = creature.center
    for _ in range(120):
        creature.step(DT, [])
    assert (creature.center - initial).length() < 0.5


def test_pbf_particles_do_not_enter_obstacle() -> None:
    creature = PBFCreature.create(center=Vec2(100.0, 100.0), radius=35.0)
    obstacle = Obstacle(125.0, 55.0, 180.0, 145.0)
    creature.set_target(Vec2(250.0, 100.0))
    for _ in range(180):
        creature.step(DT, [obstacle])
    margin = creature.config.collision_margin - 0.1
    assert all(not obstacle.contains(point, margin) for point in creature.positions)


def test_pseudopod_locomotion_moves_the_body_towards_target() -> None:
    creature = PBFCreature.create(center=Vec2())
    creature.set_target(Vec2(300.0, 0.0))
    for _ in range(90):
        creature.step(DT, [])
    assert creature.center.x > 3.0


def test_dynamic_cohesion_does_not_use_permanent_pairs() -> None:
    creature = PBFCreature.create(center=Vec2(), radius=35.0)
    creature.set_target(Vec2(300.0, 0.0))
    for _ in range(180):
        creature.step(DT, [])
    assert creature.diagnostics.connected_particles == creature.particle_count
    assert not hasattr(creature, "constraints")


def test_locomotion_cycles_through_pseudopod_phases() -> None:
    creature = PBFCreature.create(center=Vec2(), radius=35.0)
    creature.set_target(Vec2(300.0, 0.0))
    phases: set[str] = set()
    pseudopod_counts: set[int] = set()
    for _ in range(400):
        creature.step(DT, [])
        phases.add(creature.locomotion_phase)
        pseudopod_counts.add(creature.pseudopod_count)
    assert {"estensione", "adesione", "trazione", "rilascio"} <= phases
    assert pseudopod_counts == {2, 3}


def test_rear_does_not_spread_sideways_during_locomotion() -> None:
    creature = PBFCreature.create(center=Vec2(), radius=35.0)
    initial_height = max(point.y for point in creature.positions) - min(
        point.y for point in creature.positions
    )
    creature.set_target(Vec2(400.0, 0.0))
    maximum_height = initial_height
    for _ in range(360):
        creature.step(DT, [])
        height = max(point.y for point in creature.positions) - min(
            point.y for point in creature.positions
        )
        maximum_height = max(maximum_height, height)
    assert maximum_height < initial_height * 1.20


def test_release_dissipates_gliding_velocity() -> None:
    creature = PBFCreature.create(center=Vec2(), radius=35.0)
    creature.set_target(Vec2(400.0, 0.0))
    release_speeds: list[float] = []
    for _ in range(360):
        creature.step(DT, [])
        if creature.locomotion_phase == "rilascio":
            mean_speed = (
                sum(velocity.length() for velocity in creature.velocities)
                / creature.particle_count
            )
            release_speeds.append(mean_speed)
    assert len(release_speeds) > 5
    assert min(release_speeds[-10:]) < release_speeds[0] * 0.55


def test_resting_body_recovers_from_an_elongated_shape() -> None:
    creature = PBFCreature.create(center=Vec2(), radius=35.0)
    center = creature.center
    for index, point in enumerate(creature.positions):
        local = point - center
        creature.positions[index] = center + Vec2(local.x * 1.38, local.y * 0.76)
        creature.predicted[index] = Vec2(
            creature.positions[index].x, creature.positions[index].y
        )
    initial_width = max(point.x for point in creature.positions) - min(
        point.x for point in creature.positions
    )
    initial_height = max(point.y for point in creature.positions) - min(
        point.y for point in creature.positions
    )
    initial_aspect = initial_width / initial_height
    for _ in range(240):
        creature.step(DT, [])
    width = max(point.x for point in creature.positions) - min(
        point.x for point in creature.positions
    )
    height = max(point.y for point in creature.positions) - min(
        point.y for point in creature.positions
    )
    assert abs(width / height - 1.0) < abs(initial_aspect - 1.0) * 0.45


def test_large_direction_change_activates_shape_recovery() -> None:
    creature = PBFCreature.create(center=Vec2(), radius=35.0)
    creature.set_target(Vec2(300.0, 0.0))
    for _ in range(30):
        creature.step(DT, [])
    creature.set_target(Vec2(-300.0, 0.0))
    assert creature.turn_recovery_time == creature.config.turn_recovery_duration


def test_free_locomotion_does_not_create_a_long_triangular_front() -> None:
    creature = PBFCreature.create(center=Vec2(), radius=35.0)
    creature.set_target(Vec2(500.0, 0.0))
    for _ in range(480):
        creature.step(DT, [])
    center = creature.center
    direction = Vec2(1.0, 0.0)
    perpendicular = Vec2(0.0, 1.0)
    front = [
        point
        for point in creature.positions
        if (point - center).dot(direction) > creature.reference_radius * 0.35
    ]
    front_length = max((point - center).dot(direction) for point in front)
    front_half_width = max(abs((point - center).dot(perpendicular)) for point in front)
    assert front_length < front_half_width * 2.15


def test_traction_wave_keeps_rear_slower_than_middle() -> None:
    creature = PBFCreature.create(center=Vec2(), radius=35.0)
    creature.target = Vec2(400.0, 0.0)
    creature.locomotion_phase = "trazione"
    creature.pseudopod_count = 2
    velocities, _ = creature._locomotion_field(Vec2(1.0, 0.0), 400.0)
    rear = min(range(creature.particle_count), key=lambda index: creature.positions[index].x)
    middle = min(
        range(creature.particle_count),
        key=lambda index: abs(creature.positions[index].x - creature.center.x),
    )
    assert velocities[rear].x < velocities[middle].x


def test_detached_particle_is_recalled_towards_main_body() -> None:
    creature = PBFCreature.create(center=Vec2(), radius=35.0)
    detached = max(range(creature.particle_count), key=lambda index: creature.positions[index].x)
    creature.positions[detached] = creature.positions[detached] + Vec2(22.0, 0.0)
    creature.predicted[detached] = Vec2(
        creature.positions[detached].x, creature.positions[detached].y
    )
    initial_distance = (creature.positions[detached] - creature.center).length()
    for _ in range(30):
        creature.step(DT, [])
    final_distance = (creature.positions[detached] - creature.center).length()
    assert final_distance < initial_distance
