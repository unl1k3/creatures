from math import isfinite

from liquid_creature.controller import ProceduralNavigator, PseudopodStepController, StepPhase
from liquid_creature.phagocytosis import PhagocytosisController, PhagocytosisPhase
from liquid_creature.physics import SoftBody, Vec2
from liquid_creature.world import (
    Food,
    Obstacle,
    solve_food_collision,
    solve_obstacle_collisions,
)

DT = 1.0 / 120.0


def simulate(body: SoftBody, frames: int) -> None:
    for _ in range(frames):
        body.step(DT)


def test_default_topology() -> None:
    body = SoftBody.create(center=Vec2())
    assert body.outer_count == 64
    assert body.inner_count == 32
    assert body.core_count == 12
    assert body.module_count == 4
    assert len(body.particles) == 108
    assert sum(c.kind == "membrane" for c in body.constraints) == 64
    assert sum(c.kind == "radial" for c in body.constraints) == 64


def test_initial_shape_is_already_non_circular() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    radii = [(particle.position - body.center).length() for particle in body.particles[:64]]
    assert max(radii) - min(radii) > 8.0


def test_internal_modules_start_asymmetrically() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    core_start = body.outer_count + body.inner_count
    core_per_module = body.core_count // body.module_count
    module_centers: list[Vec2] = []
    for module in range(body.module_count):
        start = core_start + module * core_per_module
        points = body.particles[start : start + core_per_module]
        module_centers.append(
            Vec2(
                sum(point.position.x for point in points) / core_per_module,
                sum(point.position.y for point in points) / core_per_module,
            )
        )
    distances = [(center - body.core_center).length() for center in module_centers]
    assert max(distances) - min(distances) > 1.0


def test_lower_resolution_remains_available() -> None:
    body = SoftBody.create(center=Vec2(), outer_count=48, inner_count=12)
    assert body.outer_count == 48
    assert body.inner_count == 12
    assert body.core_count == 12
    assert body.module_count == 4
    assert len(body.particles) == 72


def test_lattice_topology_distributes_internal_support() -> None:
    body = SoftBody.create_lattice(center=Vec2(), radius=80.0)
    assert body.outer_count == 64
    assert body.inner_count > 20
    assert body.core_count == 0
    assert body.module_count == 0
    assert sum(c.kind == "lattice" for c in body.constraints) > body.inner_count
    assert sum(c.kind == "attachment" for c in body.constraints) == 64


def test_lattice_body_remains_stable_at_rest() -> None:
    body = SoftBody.create_lattice(center=Vec2(), radius=80.0)
    initial_area = body.area
    simulate(body, 600)
    assert abs(body.area - initial_area) / initial_area < 0.01
    assert body.center.length() < 3.0


def test_lattice_body_moves_with_a_pseudopod_step() -> None:
    body = SoftBody.create_lattice(center=Vec2(), radius=80.0)
    body.select_pseudopod(0)
    controller = PseudopodStepController()
    start = body.center
    assert controller.start(body)

    for _ in range(700):
        controller.update(body, DT)
        body.step(DT)
        if not controller.active:
            break

    assert not controller.active
    assert body.center.x > start.x + 4.0
    assert abs(body.center.y - start.y) < body.center.x - start.x


def test_resting_body_remains_stable() -> None:
    body = SoftBody.create(center=Vec2())
    initial_area = body.area
    simulate(body, 600)
    assert abs(body.area - initial_area) / initial_area < 0.005
    assert body.center.length() < 2.0


def test_cortical_activity_breaks_perfect_circular_symmetry() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    simulate(body, 240)
    radii = [(particle.position - body.center).length() for particle in body.particles[:64]]
    assert max(radii) - min(radii) > 3.0
    assert max(radii) - min(radii) < 50.0


def test_rear_shape_is_asymmetric_during_pseudopod_motion() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    body.select_pseudopod(0)
    body.set_pseudopod_extending(True)
    simulate(body, 150)
    rear = body.outer_count // 2
    left = (body.particles[rear - 5].position - body.center).length()
    right = (body.particles[rear + 5].position - body.center).length()
    assert abs(left - right) > 2.0


def test_body_deforms_while_pinned_and_recovers() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    initial_area = body.area
    body.pin(0, Vec2(135.0, 0.0))
    simulate(body, 180)
    assert body.particles[0].position.x == 135.0
    assert all(
        isfinite(value)
        for particle in body.particles
        for value in (particle.position.x, particle.position.y)
    )
    assert body.area > initial_area * 0.75

    body.unpin(0)
    simulate(body, 900)
    assert abs(body.area - initial_area) / initial_area < 0.03
    # Il trascinamento può traslare e ruotare liberamente tutto il corpo: ciò che
    # deve recuperare è la forma locale, non la coordinata assoluta del punto.
    recovered_radius = (body.particles[0].position - body.center).length()
    assert abs(recovered_radius - 80.0) < 6.0


def test_invalid_time_step_is_rejected() -> None:
    body = SoftBody.create()
    try:
        body.step(0.0)
    except ValueError:
        pass
    else:
        raise AssertionError("step avrebbe dovuto rifiutare dt=0")


def test_pseudopod_extends_and_retracts_smoothly() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    body.select_pseudopod(0)
    body.set_pseudopod_extending(True)
    simulate(body, 180)

    extended_radius = (body.particles[0].position - body.center).length()
    assert body.pseudopod_activation == 1.0
    assert extended_radius > 100.0
    assert body.area > body.target_area * 0.95

    body.set_pseudopod_extending(False)
    simulate(body, 300)
    recovered_radius = (body.particles[0].position - body.center).length()
    assert body.pseudopod_activation == 0.0
    assert recovered_radius < extended_radius - 10.0
    assert 60.0 < recovered_radius < 100.0


def test_pseudopod_selection_wraps_around_membrane() -> None:
    body = SoftBody.create(center=Vec2())
    body.select_pseudopod(0)
    assert body.pseudopod_weight(0) == 1.0
    assert body.pseudopod_weight(body.outer_count - 1) > 0.0
    assert body.pseudopod_weight(body.outer_count // 2) == 0.0


def test_procedural_step_moves_body_toward_pseudopod() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    body.select_pseudopod(0)
    controller = PseudopodStepController()
    start = body.center
    assert controller.start(body)

    for _ in range(600):
        controller.update(body, DT)
        body.step(DT)
        if not controller.active:
            break

    assert controller.phase is StepPhase.IDLE
    assert not body.pinned
    forward_motion = body.center.x - start.x
    lateral_motion = abs(body.center.y - start.y)
    assert forward_motion > 5.0
    assert lateral_motion < forward_motion * 0.6
    assert controller.last_displacement > 5.0


def test_progressive_release_does_not_leave_a_post_step_jolt() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    body.select_pseudopod(0)
    controller = PseudopodStepController()
    assert controller.start(body)

    for _ in range(800):
        controller.update(body, DT)
        body.step(DT)
        if not controller.active:
            break
    previous = body.center
    maximum_frame_motion = 0.0
    for _ in range(90):
        body.step(DT)
        current = body.center
        maximum_frame_motion = max(maximum_frame_motion, (current - previous).length())
        previous = current

    assert maximum_frame_motion < 1.5


def test_navigator_repeats_steps_toward_target_and_stops() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    navigator = ProceduralNavigator(random_seed=7)
    target = Vec2(130.0, 20.0)
    initial_distance = (target - body.center).length()
    navigator.set_target(target)

    for _ in range(4000):
        navigator.update(body, DT)
        body.step(DT)
        if not navigator.enabled:
            break

    final_distance = (target - body.center).length()
    assert not navigator.enabled
    assert navigator.completed_steps >= 2
    assert final_distance <= navigator.arrival_radius
    assert final_distance < initial_distance * 0.25


def test_navigator_varies_consecutive_pseudopods() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    navigator = ProceduralNavigator(random_seed=17)
    navigator.set_target(Vec2(300.0, 0.0))
    variations: list[tuple[int, float, float, int]] = []
    previous_steps = -1

    for _ in range(1800):
        navigator.update(body, DT)
        body.step(DT)
        if navigator.step_controller.active and navigator.completed_steps != previous_steps:
            previous_steps = navigator.completed_steps
            variations.append(
                (
                    body.pseudopod_half_width,
                    body.pseudopod_extension,
                    body.pseudopod_extend_speed,
                    navigator.step_controller.contact_half_width,
                )
            )
        if len(variations) >= 3:
            break

    assert len(variations) == 3
    assert len(set(variations)) == 3


def test_food_collision_keeps_membrane_outside_circle() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    food = Food(Vec2(92.0, 0.0), radius=20.0)

    contacts = 0
    for _ in range(8):
        contacts += solve_food_collision(body, food)
        body.step(DT)

    solve_food_collision(body, food)
    assert contacts > 0
    assert all(
        (particle.position - food.position).length() >= food.radius + 2.0 - 1e-6
        for particle in body.particles[: body.outer_count]
    )


def test_food_hit_testing() -> None:
    food = Food(Vec2(20.0, 30.0), radius=10.0)
    assert food.contains(Vec2(25.0, 30.0))
    assert not food.contains(Vec2(40.0, 30.0))


def test_obstacle_collision_expels_membrane_from_solid() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    obstacle = Obstacle(72.0, -24.0, 105.0, 24.0)

    contacts: list[Vec2] = []
    for _ in range(4):
        contacts.extend(solve_obstacle_collisions(body, [obstacle]))

    assert contacts
    for index in body.collision_particle_indices:
        point = body.particles[index].position
        assert not (
            obstacle.left - 2.0 + 1e-6 < point.x < obstacle.right + 2.0 - 1e-6
            and obstacle.top - 2.0 + 1e-6 < point.y < obstacle.bottom + 2.0 - 1e-6
        )


def test_obstacle_contact_reduces_tangential_sliding() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    particle = body.particles[0]
    particle.position = Vec2(80.0, 0.0)
    particle.previous = Vec2(80.0, -10.0)
    obstacle = Obstacle(70.0, -30.0, 110.0, 30.0)

    solve_obstacle_collisions(body, [obstacle], friction=0.25)

    retained_velocity = particle.position - particle.previous
    assert abs(retained_velocity.y) < 10.0
    assert particle.position.x <= obstacle.left - 2.0 + 1e-6


def test_obstacle_contacts_trigger_local_refinement() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    obstacle = Obstacle(72.0, -20.0, 105.0, 20.0)
    focuses = solve_obstacle_collisions(body, [obstacle])

    body.update_adaptive_refinement(focuses, radius=24.0, max_active=12)

    assert focuses
    assert body.active_refinement_indices


def test_nearby_prey_automatically_triggers_phagocytosis() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    food = Food(Vec2(145.0, 0.0), radius=16.0)
    navigator = ProceduralNavigator(random_seed=13)
    phagocytosis = PhagocytosisController()

    for _ in range(500):
        phagocytosis.sense_and_maybe_start(body, [food], navigator, DT)
        navigator.update(body, DT)
        phagocytosis.update(body, navigator, DT)
        body.step(DT)
        if phagocytosis.active:
            break

    assert food.adhesion >= phagocytosis.adhesion_threshold
    assert phagocytosis.active


def test_distant_prey_does_not_trigger_and_signal_decays() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    food = Food(Vec2(150.0, 0.0), radius=16.0, adhesion=0.5)
    navigator = ProceduralNavigator(random_seed=14)
    phagocytosis = PhagocytosisController(sensing_range=20.0)

    for _ in range(120):
        phagocytosis.sense_and_maybe_start(body, [food], navigator, DT)

    assert food.adhesion < 0.5
    assert not phagocytosis.active


def test_phagocytosis_wraps_and_consumes_food() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    food = Food(Vec2(165.0, 0.0), radius=16.0)
    navigator = ProceduralNavigator(random_seed=5)
    phagocytosis = PhagocytosisController()
    initial_area = body.target_area
    assert phagocytosis.start(body, food, navigator)

    saw_wrapping = False
    for _ in range(5000):
        navigator.update(body, DT)
        phagocytosis.update(body, navigator, DT)
        body.step(DT)
        for _ in range(3):
            solve_food_collision(body, food)
        saw_wrapping |= phagocytosis.phase is PhagocytosisPhase.WRAPPING
        if not phagocytosis.active:
            break

    assert saw_wrapping
    assert food.consumed
    assert phagocytosis.consumed_count == 1
    assert phagocytosis.phase is PhagocytosisPhase.IDLE
    assert not body.pinned
    assert body.target_area > initial_area


def test_phagocytosis_stabilizes_body_during_wrapping() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    food = Food(Vec2(108.0, 0.0), radius=16.0)
    navigator = ProceduralNavigator(random_seed=3)
    phagocytosis = PhagocytosisController()
    assert phagocytosis.start(body, food, navigator)

    initial_rear: Vec2 | None = None
    maximum_rear_motion = 0.0
    saw_wrap = False
    for _ in range(1200):
        navigator.update(body, DT)
        phagocytosis.update(body, navigator, DT)
        body.step(DT)
        solve_food_collision(body, food)
        if phagocytosis.phase is PhagocytosisPhase.WRAPPING:
            saw_wrap = True
            rear = body.particles[phagocytosis.stabilizing_particles[1]].position
            initial_rear = initial_rear or Vec2(rear.x, rear.y)
            maximum_rear_motion = max(maximum_rear_motion, (rear - initial_rear).length())
        if saw_wrap and phagocytosis.phase is PhagocytosisPhase.INTERNALIZING:
            break

    assert saw_wrap
    assert maximum_rear_motion < 0.1


def test_wrapping_starts_from_contact_without_instant_centering() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    food = Food(Vec2(108.0, 0.0), radius=16.0)
    navigator = ProceduralNavigator(random_seed=9)
    phagocytosis = PhagocytosisController()
    assert phagocytosis.start(body, food, navigator)

    while phagocytosis.phase is PhagocytosisPhase.APPROACHING:
        navigator.update(body, DT)
        phagocytosis.update(body, navigator, DT)
        body.step(DT)
    contact_distance = (body.center - food.position).length()

    for _ in range(45):
        navigator.update(body, DT)
        phagocytosis.update(body, navigator, DT)
        body.step(DT)
        solve_food_collision(body, food)

    assert phagocytosis.phase is PhagocytosisPhase.WRAPPING
    assert (body.center - food.position).length() > contact_distance * 0.80


def test_food_remains_enclosed_after_wrap_until_digestion() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    food = Food(Vec2(108.0, 0.0), radius=16.0)
    navigator = ProceduralNavigator(random_seed=8)
    phagocytosis = PhagocytosisController()
    assert phagocytosis.start(body, food, navigator)

    saw_internalization = False
    saw_digestion = False
    for _ in range(3000):
        navigator.update(body, DT)
        phagocytosis.update(body, navigator, DT)
        body.step(DT)
        solve_food_collision(body, food)
        if phagocytosis.phase is PhagocytosisPhase.INTERNALIZING:
            saw_internalization = True
            assert body.contains_point(food.position)
        elif phagocytosis.phase is PhagocytosisPhase.DIGESTING:
            saw_digestion = True
            assert body.contains_point(food.position)
        if not phagocytosis.active:
            break

    assert saw_internalization
    assert saw_digestion
    assert food.consumed


def test_membrane_is_not_frozen_during_internalization_and_digestion() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    food = Food(Vec2(108.0, 0.0), radius=16.0)
    navigator = ProceduralNavigator(random_seed=12)
    phagocytosis = PhagocytosisController()
    assert phagocytosis.start(body, food, navigator)

    for _ in range(1800):
        navigator.update(body, DT)
        phagocytosis.update(body, navigator, DT)
        body.step(DT)
        solve_food_collision(body, food)
        if phagocytosis.phase is PhagocytosisPhase.INTERNALIZING:
            break

    assert not body.pinned
    before = [Vec2(point.x, point.y) for point in body.outer_positions]
    for _ in range(45):
        phagocytosis.update(body, navigator, DT)
        body.step(DT)
        solve_food_collision(body, food)
    maximum_motion = max(
        (current - previous).length()
        for current, previous in zip(body.outer_positions, before, strict=True)
    )
    assert maximum_motion > 0.5
    assert body.contains_point(food.position)


def test_creature_advances_while_internalized_food_stays_fixed() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    food = Food(Vec2(108.0, 0.0), radius=16.0)
    navigator = ProceduralNavigator(random_seed=16)
    phagocytosis = PhagocytosisController()
    assert phagocytosis.start(body, food, navigator)

    for _ in range(1800):
        navigator.update(body, DT)
        phagocytosis.update(body, navigator, DT)
        body.step(DT)
        solve_food_collision(body, food)
        if phagocytosis.phase is PhagocytosisPhase.INTERNALIZING:
            break
    captured_position = Vec2(food.position.x, food.position.y)
    initial_inner_center = body.inner_center
    initial_distance = (food.position - initial_inner_center).length()

    for _ in range(160):
        phagocytosis.update(body, navigator, DT)
        body.step(DT)
        solve_food_collision(body, food)

    assert (food.position - captured_position).length() < 0.01
    assert (food.position - body.inner_center).length() < initial_distance - 5.0
    assert (body.inner_center - initial_inner_center).length() > 5.0
    assert body.contains_point(food.position)


def test_phagocytosis_interrupts_previous_navigation() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    navigator = ProceduralNavigator(random_seed=10)
    navigator.set_target(Vec2(-250.0, 0.0))
    for _ in range(100):
        navigator.update(body, DT)
        body.step(DT)
    assert navigator.step_controller.active

    food = Food(Vec2(150.0, 0.0), radius=16.0)
    phagocytosis = PhagocytosisController()
    assert phagocytosis.start(body, food, navigator)
    assert navigator.target is not None
    assert navigator.target.x > body.center.x
    assert phagocytosis.phase is PhagocytosisPhase.APPROACHING


def test_default_demo_scenario_completes_phagocytosis_with_refinement() -> None:
    body = SoftBody.create()
    food = Food(Vec2(620.0, 310.0), radius=20.0)
    navigator = ProceduralNavigator(random_seed=1)
    phagocytosis = PhagocytosisController()
    assert phagocytosis.start(body, food, navigator)

    for _ in range(2600):
        focuses = [] if food.consumed else [food.position]
        focuses.extend(
            body.particles[protrusion.center].position
            for protrusion in body.transient_protrusions
        )
        body.update_adaptive_refinement(focuses)
        navigator.update(body, DT)
        phagocytosis.update(body, navigator, DT)
        body.step(DT)
        for _ in range(3):
            solve_food_collision(body, food)
        if not phagocytosis.active:
            break

    assert food.consumed
    assert phagocytosis.phase is PhagocytosisPhase.IDLE


def test_adaptive_refinement_adds_and_deactivates_physical_points() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    base_particles = len(body.particles)
    base_active = body.active_particle_count
    focus = Vec2(80.0, 0.0)

    body.update_adaptive_refinement([focus], radius=20.0, max_active=8)
    active_details = body.active_refinement_indices
    assert active_details
    assert len(body.particles) > base_particles
    assert body.active_particle_count > base_active
    assert all(body.is_particle_active(index) for index in active_details)
    assert any(c.kind == "refined_membrane" and c.active for c in body.constraints)

    body.update_adaptive_refinement([], radius=20.0, max_active=8)
    assert not body.active_refinement_indices
    assert body.active_particle_count == base_active
    assert all(
        c.active for c in body.constraints if c.kind == "membrane"
    )


def test_refined_points_participate_in_food_collision() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    body.update_adaptive_refinement([Vec2(80.0, 0.0)], radius=18.0, max_active=4)
    detail_index = body.active_refinement_indices[0]
    detail = body.particles[detail_index]
    food = Food(Vec2(detail.position.x, detail.position.y), radius=8.0)

    assert solve_food_collision(body, food) > 0
    assert (body.particles[detail_index].position - food.position).length() >= 10.0 - 1e-6


def test_multiple_transient_protrusions_appear_and_expire() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    body.add_transient_protrusion(12, half_width=5, strength=0.35, lifetime=0.8)
    body.add_transient_protrusion(52, half_width=6, strength=0.30, lifetime=1.0)
    initial_first = (body.particles[12].position - body.center).length()
    initial_second = (body.particles[52].position - body.center).length()

    simulate(body, 55)
    assert (body.particles[12].position - body.center).length() > initial_first + 2.0
    assert (body.particles[52].position - body.center).length() > initial_second + 2.0
    assert len(body.transient_protrusions) == 2

    simulate(body, 100)
    assert not body.transient_protrusions


def test_refinement_deactivation_does_not_create_a_velocity_kick() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    body.update_adaptive_refinement([Vec2(80.0, 0.0)], radius=22.0, max_active=8)
    simulate(body, 30)
    body.update_adaptive_refinement([])
    before = body.center
    body.step(DT)
    assert (body.center - before).length() < 1.0


def segments_cross(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> bool:
    def orientation(first: Vec2, second: Vec2, third: Vec2) -> float:
        return (second.x - first.x) * (third.y - first.y) - (
            second.y - first.y
        ) * (third.x - first.x)

    return orientation(a, b, c) * orientation(a, b, d) < 0.0 and orientation(
        c, d, a
    ) * orientation(c, d, b) < 0.0


def test_wrapping_does_not_self_intersect_membrane() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    food = Food(Vec2(108.0, 0.0), radius=16.0)
    navigator = ProceduralNavigator(random_seed=4)
    phagocytosis = PhagocytosisController()
    assert phagocytosis.start(body, food, navigator)

    for _ in range(1600):
        navigator.update(body, DT)
        phagocytosis.update(body, navigator, DT)
        body.step(DT)
        solve_food_collision(body, food)
        if (
            phagocytosis.phase is PhagocytosisPhase.WRAPPING
            and phagocytosis.progress >= 0.95
        ):
            break

    points = body.outer_positions
    for first in range(body.outer_count):
        first_next = (first + 1) % body.outer_count
        for second in range(first + 2, body.outer_count):
            second_next = (second + 1) % body.outer_count
            if first == second_next or first_next == second:
                continue
            assert not segments_cross(
                points[first], points[first_next], points[second], points[second_next]
            )


def test_wrapping_tip_motion_is_speed_limited() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    food = Food(Vec2(108.0, 0.0), radius=16.0)
    navigator = ProceduralNavigator(random_seed=15)
    phagocytosis = PhagocytosisController()
    assert phagocytosis.start(body, food, navigator)
    previous_tip: Vec2 | None = None
    maximum_step = 0.0

    for _ in range(1200):
        navigator.update(body, DT)
        phagocytosis.update(body, navigator, DT)
        body.step(DT)
        solve_food_collision(body, food)
        if phagocytosis.phase is PhagocytosisPhase.WRAPPING:
            tip = body.particles[phagocytosis.tip_index or 0].position
            if previous_tip is not None:
                maximum_step = max(maximum_step, (tip - previous_tip).length())
            previous_tip = Vec2(tip.x, tip.y)
        elif previous_tip is not None:
            break

    assert maximum_step < 2.0


def test_completed_phagocytosis_does_not_push_body_backward() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    food = Food(Vec2(108.0, 0.0), radius=16.0)
    navigator = ProceduralNavigator(random_seed=2)
    phagocytosis = PhagocytosisController()
    assert phagocytosis.start(body, food, navigator)

    for _ in range(5000):
        navigator.update(body, DT)
        phagocytosis.update(body, navigator, DT)
        body.step(DT)
        solve_food_collision(body, food)
        if not phagocytosis.active:
            break
    completed_center = body.center
    body.update_adaptive_refinement([])
    simulate(body, 180)

    assert body.center.x >= completed_center.x - 3.0


def test_completed_phagocytosis_does_not_accelerate_toward_food() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    food = Food(Vec2(108.0, 0.0), radius=16.0)
    navigator = ProceduralNavigator(random_seed=6)
    phagocytosis = PhagocytosisController()
    assert phagocytosis.start(body, food, navigator)

    for _ in range(5000):
        navigator.update(body, DT)
        phagocytosis.update(body, navigator, DT)
        body.step(DT)
        solve_food_collision(body, food)
        if not phagocytosis.active:
            break
    completed_center = body.center
    maximum_displacement = 0.0
    for _ in range(100):
        body.update_adaptive_refinement([])
        body.step(DT)
        maximum_displacement = max(
            maximum_displacement, (body.center - completed_center).length()
        )

    assert maximum_displacement < 1.5


def test_new_step_cancels_post_phagocytosis_stabilization() -> None:
    body = SoftBody.create(center=Vec2(), radius=80.0)
    body.select_pseudopod(0)
    body.stabilize_after_internal_motion()
    controller = PseudopodStepController()

    assert controller.start(body)
    assert body.stabilized_center is None
