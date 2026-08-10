from liquid_creature.pbf import PBFCreature
from liquid_creature.physics import Vec2
from liquid_creature.rendering import creature_contour, stabilize_contour


def test_creature_contour_surrounds_particle_body() -> None:
    creature = PBFCreature.create(center=Vec2(), radius=35.0)
    contour = creature_contour(creature.positions)

    assert len(contour) >= 12
    assert min(point.x for point in contour) < -25.0
    assert max(point.x for point in contour) > 25.0
    assert min(point.y for point in contour) < -25.0
    assert max(point.y for point in contour) > 25.0
    assert min(point.x for point in contour) > min(point.x for point in creature.positions) - 7.0
    assert max(point.x for point in contour) < max(point.x for point in creature.positions) + 7.0


def test_empty_particle_set_has_no_contour() -> None:
    assert creature_contour([]) == []


def test_distant_particle_remains_inside_single_external_contour() -> None:
    particles = [
        Vec2(-10.0, -10.0),
        Vec2(10.0, -10.0),
        Vec2(10.0, 10.0),
        Vec2(-10.0, 10.0),
        Vec2(35.0, 0.0),
    ]
    contour = creature_contour(particles)

    assert contour
    assert max(point.x for point in contour) > 35.0
    assert min(point.x for point in contour) < -10.0


def test_stabilized_contour_changes_progressively() -> None:
    previous = [Vec2(-10.0, -10.0), Vec2(10.0, -10.0), Vec2(10.0, 10.0), Vec2(-10.0, 10.0)]
    current = [Vec2(-20.0, -10.0), Vec2(20.0, -10.0), Vec2(20.0, 10.0), Vec2(-20.0, 10.0)]
    stable = stabilize_contour([], previous, vertex_count=16)
    changed = stabilize_contour(stable, current, response=0.2, vertex_count=16)

    assert len(stable) == len(changed) == 16
    assert max(point.x for point in stable) < max(point.x for point in changed)
    assert max(point.x for point in changed) < 20.0


def test_concavity_requires_adhesion_points() -> None:
    creature = PBFCreature.create(center=Vec2(), radius=35.0)
    without_adhesion = creature_contour(
        creature.positions,
        direction=Vec2(1.0, 0.0),
        anchor_points=[],
    )
    with_adhesion = creature_contour(
        creature.positions,
        direction=Vec2(1.0, 0.0),
        anchor_points=[Vec2(25.0, -12.0), Vec2(25.0, 12.0)],
    )

    assert without_adhesion
    assert with_adhesion
    assert len(with_adhesion) >= len(without_adhesion)
