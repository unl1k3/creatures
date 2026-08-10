"""Membrana robusta costruita dall'inviluppo esterno delle particelle."""

from __future__ import annotations

from math import cos, pi, sin

from .physics import Vec2


def creature_contour(
    particles: list[Vec2],
    particle_radius: float = 4.0,
    circle_samples: int = 10,
    direction: Vec2 | None = None,
    anchor_points: list[Vec2] | None = None,
) -> list[Vec2]:
    """Calcola l'inviluppo convesso dell'unione delle superfici circolari."""
    if len(particles) < 3:
        return []
    surface_points = [
        particle
        + Vec2(
            cos(2.0 * pi * sample / circle_samples),
            sin(2.0 * pi * sample / circle_samples),
        )
        * particle_radius
        for particle in particles
        for sample in range(circle_samples)
    ]
    hull = _convex_hull(surface_points)
    if (
        direction is not None
        and direction.length() > 1e-9
        and anchor_points is not None
        and len(anchor_points) >= 2
    ):
        hull = _add_supported_front_concavities(
            hull,
            particles,
            direction / direction.length(),
            particle_radius,
            anchor_points,
        )
    return _chaikin(hull, iterations=2) if len(hull) >= 3 else []


def stabilize_contour(
    previous: list[Vec2],
    current: list[Vec2],
    response: float = 0.22,
    vertex_count: int = 96,
) -> list[Vec2]:
    """Mantiene topologia e movimento continui tra fotogrammi."""
    if len(current) < 3:
        return previous
    target = _resample_closed(current, vertex_count)
    if len(previous) != vertex_count:
        return target
    response = min(1.0, max(0.0, response))
    return [old + (new - old) * response for old, new in zip(previous, target, strict=True)]


def _add_supported_front_concavities(
    hull: list[Vec2],
    particles: list[Vec2],
    direction: Vec2,
    particle_radius: float,
    anchor_points: list[Vec2],
) -> list[Vec2]:
    """Scava valli poco profonde tra punte frontali senza dividere il bordo."""
    result: list[Vec2] = []
    minimum_edge = particle_radius * 4.5
    maximum_depth = particle_radius * 2.2
    support_distance_squared = (particle_radius * 4.25) ** 2
    for index, first in enumerate(hull):
        second = hull[(index + 1) % len(hull)]
        result.append(first)
        edge = second - first
        edge_length = edge.length()
        if edge_length < minimum_edge:
            continue
        outward = Vec2(edge.y, -edge.x) / edge_length
        if outward.dot(direction) < 0.35:
            continue
        anchor_projections = sorted(
            (anchor - first).dot(edge) / (edge_length * edge_length)
            for anchor in anchor_points
            if 0.0 < (anchor - first).dot(edge) / (edge_length * edge_length) < 1.0
        )
        if len(anchor_projections) < 2:
            continue
        anchor_span = anchor_projections[-1] - anchor_projections[0]
        if anchor_span < 0.18:
            continue
        inward = outward * -1.0
        best: tuple[float, Vec2] | None = None
        for particle_index, particle in enumerate(particles):
            projection = (particle - first).dot(edge) / (edge_length * edge_length)
            if not 0.18 < projection < 0.82:
                continue
            edge_point = first + edge * projection
            depth = (particle - edge_point).dot(inward) - particle_radius
            if depth <= particle_radius * 0.45:
                continue
            support = sum(
                (neighbor - particle).dot(neighbor - particle) <= support_distance_squared
                for neighbor_index, neighbor in enumerate(particles)
                if neighbor_index != particle_index
            )
            if support < 3:
                continue
            score = depth - abs(projection - 0.5) * particle_radius * 1.5
            if best is None or score > best[0]:
                valley_depth = min(maximum_depth, depth)
                best = (score, edge_point + inward * valley_depth)
        if best is not None:
            result.append(best[1])
    return result


def _convex_hull(points: list[Vec2]) -> list[Vec2]:
    ordered = sorted({(point.x, point.y) for point in points})
    if len(ordered) <= 1:
        return [Vec2(*point) for point in ordered]

    def cross(
        origin: tuple[float, float],
        first: tuple[float, float],
        second: tuple[float, float],
    ) -> float:
        return (first[0] - origin[0]) * (second[1] - origin[1]) - (first[1] - origin[1]) * (
            second[0] - origin[0]
        )

    lower: list[tuple[float, float]] = []
    for point in ordered:
        while len(lower) >= 2 and cross(lower[-2], lower[-1], point) <= 0.0:
            lower.pop()
        lower.append(point)

    upper: list[tuple[float, float]] = []
    for point in reversed(ordered):
        while len(upper) >= 2 and cross(upper[-2], upper[-1], point) <= 0.0:
            upper.pop()
        upper.append(point)
    return [Vec2(*point) for point in lower[:-1] + upper[:-1]]


def _chaikin(points: list[Vec2], iterations: int) -> list[Vec2]:
    result = points
    for _ in range(iterations):
        result = [
            mixed
            for index, point in enumerate(result)
            for mixed in (
                point * 0.75 + result[(index + 1) % len(result)] * 0.25,
                point * 0.25 + result[(index + 1) % len(result)] * 0.75,
            )
        ]
    return result


def _resample_closed(points: list[Vec2], count: int) -> list[Vec2]:
    lengths = [
        (points[(index + 1) % len(points)] - point).length() for index, point in enumerate(points)
    ]
    perimeter = sum(lengths)
    if perimeter <= 1e-9:
        return [points[0] for _ in range(count)]
    result: list[Vec2] = []
    edge_index = 0
    edge_start_distance = 0.0
    for sample in range(count):
        distance = perimeter * sample / count
        while (
            edge_index < len(lengths) - 1 and edge_start_distance + lengths[edge_index] < distance
        ):
            edge_start_distance += lengths[edge_index]
            edge_index += 1
        edge_length = max(lengths[edge_index], 1e-9)
        ratio = (distance - edge_start_distance) / edge_length
        first = points[edge_index]
        second = points[(edge_index + 1) % len(points)]
        result.append(first + (second - first) * ratio)
    return result
