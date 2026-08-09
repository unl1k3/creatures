"""Creatura a nuvola di punti senza connessioni permanenti."""

from __future__ import annotations

from dataclasses import dataclass, field
from math import sin, sqrt

from .physics import Vec2
from .world import Obstacle


def _cross(first: Vec2, second: Vec2, third: Vec2) -> float:
    return (second.x - first.x) * (third.y - first.y) - (
        second.y - first.y
    ) * (third.x - first.x)


def _circumcircle(
    first: Vec2, second: Vec2, third: Vec2
) -> tuple[Vec2, float] | None:
    denominator = 2.0 * (
        first.x * (second.y - third.y)
        + second.x * (third.y - first.y)
        + third.x * (first.y - second.y)
    )
    if abs(denominator) < 1e-9:
        return None
    first_norm = first.dot(first)
    second_norm = second.dot(second)
    third_norm = third.dot(third)
    center = Vec2(
        (
            first_norm * (second.y - third.y)
            + second_norm * (third.y - first.y)
            + third_norm * (first.y - second.y)
        )
        / denominator,
        (
            first_norm * (third.x - second.x)
            + second_norm * (first.x - third.x)
            + third_norm * (second.x - first.x)
        )
        / denominator,
    )
    return center, (first - center).length()


def _convex_hull(points: list[Vec2]) -> list[Vec2]:
    ordered = sorted(points, key=lambda point: (point.x, point.y))
    if len(ordered) <= 2:
        return ordered
    lower: list[Vec2] = []
    for point in ordered:
        while len(lower) >= 2 and _cross(lower[-2], lower[-1], point) <= 0.0:
            lower.pop()
        lower.append(point)
    upper: list[Vec2] = []
    for point in reversed(ordered):
        while len(upper) >= 2 and _cross(upper[-2], upper[-1], point) <= 0.0:
            upper.pop()
        upper.append(point)
    return lower[:-1] + upper[:-1]


def _delaunay(points: list[Vec2]) -> list[tuple[int, int, int]]:
    """Triangolazione Bowyer-Watson sufficiente per piccole nuvole 2D."""
    if len(points) < 3:
        return []
    minimum_x = min(point.x for point in points)
    maximum_x = max(point.x for point in points)
    minimum_y = min(point.y for point in points)
    maximum_y = max(point.y for point in points)
    span = max(maximum_x - minimum_x, maximum_y - minimum_y, 1.0)
    middle = Vec2((minimum_x + maximum_x) * 0.5, (minimum_y + maximum_y) * 0.5)
    work = points + [
        middle + Vec2(-20.0 * span, -10.0 * span),
        middle + Vec2(0.0, 20.0 * span),
        middle + Vec2(20.0 * span, -10.0 * span),
    ]
    original_count = len(points)
    triangles: list[tuple[int, int, int]] = [
        (original_count, original_count + 1, original_count + 2)
    ]
    for point_index in range(original_count):
        bad: list[tuple[int, int, int]] = []
        for triangle in triangles:
            circle = _circumcircle(*(work[index] for index in triangle))
            if circle is None:
                continue
            center, radius = circle
            if (work[point_index] - center).length() <= radius + 1e-7:
                bad.append(triangle)
        edge_counts: dict[tuple[int, int], int] = {}
        for triangle in bad:
            for first, second in (
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ):
                edge = tuple(sorted((first, second)))
                edge_counts[edge] = edge_counts.get(edge, 0) + 1
        triangles = [triangle for triangle in triangles if triangle not in bad]
        for edge, count in edge_counts.items():
            if count == 1:
                triangles.append((edge[0], edge[1], point_index))
    return [
        triangle
        for triangle in triangles
        if all(index < original_count for index in triangle)
    ]


def _polygon_area(points: list[Vec2]) -> float:
    return 0.5 * sum(
        point.x * points[(index + 1) % len(points)].y
        - points[(index + 1) % len(points)].x * point.y
        for index, point in enumerate(points)
    )


def _point_segment_distance(point: Vec2, first: Vec2, second: Vec2) -> float:
    segment = second - first
    length_squared = segment.dot(segment)
    if length_squared < 1e-9:
        return (point - first).length()
    fraction = min(1.0, max(0.0, (point - first).dot(segment) / length_squared))
    return (point - (first + segment * fraction)).length()


def _polygon_contains(point: Vec2, polygon: list[Vec2], margin: float = 0.0) -> bool:
    if len(polygon) < 3:
        return False
    if any(
        _point_segment_distance(point, first, polygon[(index + 1) % len(polygon)])
        <= margin
        for index, first in enumerate(polygon)
    ):
        return True
    inside = False
    previous = polygon[-1]
    for current in polygon:
        if (current.y > point.y) != (previous.y > point.y):
            crossing_x = (
                (previous.x - current.x)
                * (point.y - current.y)
                / (previous.y - current.y)
                + current.x
            )
            if point.x < crossing_x:
                inside = not inside
        previous = current
    return inside


def concave_hull(points: list[Vec2], alpha_radius: float) -> list[Vec2]:
    triangles = []
    for triangle in _delaunay(points):
        circle = _circumcircle(*(points[index] for index in triangle))
        if circle is not None and circle[1] <= alpha_radius:
            triangles.append(triangle)
    edge_counts: dict[tuple[int, int], int] = {}
    for triangle in triangles:
        for first, second in (
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ):
            edge = tuple(sorted((first, second)))
            edge_counts[edge] = edge_counts.get(edge, 0) + 1
    boundary_edges = [edge for edge, count in edge_counts.items() if count == 1]
    adjacency: dict[int, list[int]] = {}
    for first, second in boundary_edges:
        adjacency.setdefault(first, []).append(second)
        adjacency.setdefault(second, []).append(first)
    loops: list[list[Vec2]] = []
    unused = set(boundary_edges)
    while unused:
        first_edge = next(iter(unused))
        start, current = first_edge
        loop = [start, current]
        unused.discard(tuple(sorted(first_edge)))
        while current != start:
            candidates = [
                candidate
                for candidate in adjacency.get(current, [])
                if tuple(sorted((current, candidate))) in unused
            ]
            if not candidates:
                break
            following = candidates[0]
            unused.discard(tuple(sorted((current, following))))
            current = following
            if current != start:
                loop.append(current)
        if current == start and len(loop) >= 3:
            loops.append([points[index] for index in loop])
    if not loops:
        return _convex_hull(points)
    result = max(loops, key=lambda loop: abs(_polygon_area(loop)))
    if _polygon_area(result) < 0.0:
        result.reverse()
    return result


def implicit_contour(
    points: list[Vec2],
    influence_radius: float,
    threshold: float = 0.28,
    grid_step: float = 5.0,
    weights: list[float] | None = None,
) -> list[Vec2]:
    """Estrae una linea di livello metaball mediante marching squares."""
    if not points:
        return []
    minimum_x = min(point.x for point in points) - influence_radius
    maximum_x = max(point.x for point in points) + influence_radius
    minimum_y = min(point.y for point in points) - influence_radius
    maximum_y = max(point.y for point in points) + influence_radius
    columns = max(2, int((maximum_x - minimum_x) / grid_step) + 2)
    rows = max(2, int((maximum_y - minimum_y) / grid_step) + 2)
    radius_squared = influence_radius * influence_radius
    point_weights = weights or [1.0] * len(points)

    def density(position: Vec2) -> float:
        value = 0.0
        for point, weight in zip(points, point_weights, strict=True):
            delta = position - point
            distance_squared = delta.dot(delta)
            if distance_squared >= radius_squared:
                continue
            kernel = 1.0 - distance_squared / radius_squared
            value += weight * kernel * kernel
        return value

    coordinates = [
        [Vec2(minimum_x + column * grid_step, minimum_y + row * grid_step) for column in range(columns)]
        for row in range(rows)
    ]
    values = [[density(position) for position in row] for row in coordinates]
    segments: list[tuple[Vec2, Vec2]] = []

    def interpolate(first: Vec2, second: Vec2, a: float, b: float) -> Vec2:
        difference = b - a
        fraction = 0.5 if abs(difference) < 1e-9 else (threshold - a) / difference
        return first + (second - first) * min(1.0, max(0.0, fraction))

    for row in range(rows - 1):
        for column in range(columns - 1):
            corners = (
                coordinates[row][column],
                coordinates[row][column + 1],
                coordinates[row + 1][column + 1],
                coordinates[row + 1][column],
            )
            corner_values = (
                values[row][column],
                values[row][column + 1],
                values[row + 1][column + 1],
                values[row + 1][column],
            )
            case = sum(
                (1 << index) if value >= threshold else 0
                for index, value in enumerate(corner_values)
            )
            if case in (0, 15):
                continue
            crossings: dict[int, Vec2] = {}
            for edge, (first, second) in enumerate(((0, 1), (1, 2), (2, 3), (3, 0))):
                if (corner_values[first] >= threshold) == (
                    corner_values[second] >= threshold
                ):
                    continue
                crossings[edge] = interpolate(
                    corners[first],
                    corners[second],
                    corner_values[first],
                    corner_values[second],
                )
            edges = list(crossings)
            if len(edges) == 2:
                segments.append((crossings[edges[0]], crossings[edges[1]]))
            elif len(edges) == 4:
                center_value = sum(corner_values) * 0.25
                if (case == 5 and center_value >= threshold) or (
                    case == 10 and center_value < threshold
                ):
                    pairs = ((0, 1), (2, 3))
                else:
                    pairs = ((0, 3), (1, 2))
                segments.extend((crossings[first], crossings[second]) for first, second in pairs)

    def key(point: Vec2) -> tuple[int, int]:
        precision = max(grid_step * 1e-4, 1e-6)
        return round(point.x / precision), round(point.y / precision)

    positions: dict[tuple[int, int], Vec2] = {}
    adjacency: dict[tuple[int, int], list[tuple[int, int]]] = {}
    unused: set[tuple[tuple[int, int], tuple[int, int]]] = set()
    for first, second in segments:
        first_key = key(first)
        second_key = key(second)
        positions[first_key] = first
        positions[second_key] = second
        adjacency.setdefault(first_key, []).append(second_key)
        adjacency.setdefault(second_key, []).append(first_key)
        unused.add(tuple(sorted((first_key, second_key))))

    loops: list[list[Vec2]] = []
    while unused:
        start, current = next(iter(unused))
        loop = [start, current]
        unused.discard(tuple(sorted((start, current))))
        while current != start:
            candidates = [
                candidate
                for candidate in adjacency.get(current, [])
                if tuple(sorted((current, candidate))) in unused
            ]
            if not candidates:
                break
            following = candidates[0]
            unused.discard(tuple(sorted((current, following))))
            current = following
            if current != start:
                loop.append(current)
        if current == start and len(loop) >= 3:
            loops.append([positions[item] for item in loop])
    if not loops:
        return _convex_hull(points)
    result = max(loops, key=lambda loop: abs(_polygon_area(loop)))
    if _polygon_area(result) < 0.0:
        result.reverse()
    return result


def _resample_closed(points: list[Vec2], count: int) -> list[Vec2]:
    if len(points) < 2:
        return points
    lengths = [
        (points[(index + 1) % len(points)] - point).length()
        for index, point in enumerate(points)
    ]
    perimeter = sum(lengths)
    if perimeter < 1e-9:
        return [points[0] for _ in range(count)]
    result: list[Vec2] = []
    edge = 0
    accumulated = 0.0
    for sample in range(count):
        target = perimeter * sample / count
        while accumulated + lengths[edge] < target:
            accumulated += lengths[edge]
            edge = (edge + 1) % len(points)
        fraction = (target - accumulated) / max(lengths[edge], 1e-9)
        first = points[edge]
        second = points[(edge + 1) % len(points)]
        result.append(first + (second - first) * fraction)
    return result


@dataclass(slots=True)
class PointCloudCreature:
    points: list[Vec2]
    spacing: float
    masses: list[float] = field(default_factory=list)
    target: Vec2 | None = None
    gait_time: float = 0.0
    gait_duration: float = 1.45
    speed: float = 46.0
    outline: list[Vec2] = field(default_factory=list)
    outline_samples: int = 96
    rebalance_timer: float = 0.0
    rest_diameter: float = 0.0
    outline_timer: float = 0.0
    gait_cycle: int = 0
    pseudopod_count: int = 3
    target_outline_area: float = 0.0
    contour_threshold: float = 0.28

    @classmethod
    def create(
        cls,
        center: Vec2 | None = None,
        radius: float = 70.0,
        spacing: float = 13.0,
    ) -> PointCloudCreature:
        center = center or Vec2(210.0, 310.0)
        points: list[Vec2] = []
        extent = round(radius / spacing) + 1
        for row in range(-extent, extent + 1):
            for column in range(-extent, extent + 1):
                local = Vec2(
                    spacing * (column + 0.5 * row),
                    spacing * sqrt(3.0) * 0.5 * row,
                )
                deformation = 1.0 + 0.045 * sin(column * 1.7 + row * 0.8)
                if local.length() <= radius * deformation:
                    points.append(center + local)
        creature = cls(points, spacing, [1.0 for _ in points])
        creature.rest_diameter = max(
            (first - second).length()
            for first in points
            for second in points
        )
        creature._rebuild_outline(immediate=True)
        creature.target_outline_area = abs(_polygon_area(creature.outline))
        return creature

    @property
    def center(self) -> Vec2:
        return Vec2(
            sum(point.x for point in self.points) / len(self.points),
            sum(point.y for point in self.points) / len(self.points),
        )

    @property
    def moving(self) -> bool:
        return self.target is not None

    def set_target(self, target: Vec2) -> None:
        self.target = Vec2(target.x, target.y)

    def _density_relaxation(self, dt: float) -> None:
        corrections = [Vec2() for _ in self.points]
        counts = [0 for _ in self.points]
        influence = self.spacing * 1.55
        for first in range(len(self.points)):
            for second in range(first + 1, len(self.points)):
                delta = self.points[second] - self.points[first]
                distance = delta.length()
                if distance < 1e-9 or distance > influence:
                    continue
                direction = delta / distance
                # Il vicinato impedisce soltanto la sovrapposizione. Non esiste
                # alcuna attrazione verso una distanza di riposo: sarebbe una
                # molla temporanea e ricompatterebbe la massa nelle strettoie.
                difference = min(0.0, distance - self.spacing)
                correction = direction * (difference * 0.16)
                corrections[first] = corrections[first] + correction
                corrections[second] = corrections[second] - correction
                counts[first] += 1
                counts[second] += 1
        response = min(1.0, dt * 8.0)
        for index in range(len(self.points)):
            if counts[index]:
                self.points[index] = self.points[index] + corrections[index] * response

    def _rebalance_points(self) -> None:
        if len(self.points) < 4:
            return
        closest_pair: tuple[int, int] | None = None
        closest_distance = float("inf")
        for first in range(len(self.points)):
            for second in range(first + 1, len(self.points)):
                distance = (self.points[first] - self.points[second]).length()
                if distance < closest_distance:
                    closest_distance = distance
                    closest_pair = (first, second)
        if (
            closest_pair is not None
            and len(self.points) > 84
            and closest_distance < self.spacing * 0.38
        ):
            first, second = closest_pair
            combined_mass = self.masses[first] + self.masses[second]
            self.points[first] = (
                self.points[first] * self.masses[first]
                + self.points[second] * self.masses[second]
            ) / combined_mass
            self.masses[first] = combined_mass
            self.points.pop(second)
            self.masses.pop(second)

        triangles = _delaunay(self.points)
        candidate: tuple[float, Vec2, tuple[int, int, int]] | None = None
        for triangle in triangles:
            vertices = [self.points[index] for index in triangle]
            longest = max(
                (vertices[1] - vertices[0]).length(),
                (vertices[2] - vertices[1]).length(),
                (vertices[0] - vertices[2]).length(),
            )
            if longest > self.spacing * 1.85:
                centroid = (vertices[0] + vertices[1] + vertices[2]) / 3.0
                if candidate is None or longest > candidate[0]:
                    candidate = (longest, centroid, triangle)
        if candidate is not None and len(self.points) < 180:
            available = min(self.masses[index] for index in candidate[2])
            new_mass = min(0.75, max(0.0, (available - 0.25) * 1.5))
            if new_mass > 0.15:
                transfer = new_mass / 3.0
                for index in candidate[2]:
                    self.masses[index] -= transfer
                self.points.append(candidate[1])
                self.masses.append(new_mass)

    def _rebuild_outline(self, immediate: bool = False) -> None:
        def build(threshold: float) -> list[Vec2]:
            for factor in (1.05, 1.20, 1.40, 1.70, 2.00):
                candidate = implicit_contour(
                    self.points,
                    influence_radius=self.spacing * factor,
                    threshold=threshold,
                    grid_step=self.spacing * 0.36,
                    weights=self.masses,
                )
                if all(
                    _polygon_contains(
                        point, candidate, margin=self.spacing * 0.05
                    )
                    for point in self.points
                ):
                    return candidate
            return []

        raw = build(self.contour_threshold)
        if not raw:
            raw = _convex_hull(self.points)
        if self.target_outline_area > 0.0:
            current_area = abs(_polygon_area(raw))
            area_ratio = current_area / self.target_outline_area
            if area_ratio < 0.90 or area_ratio > 1.10:
                adjustment = min(1.45, max(0.55, area_ratio))
                adjusted_threshold = min(
                    0.44, max(0.045, self.contour_threshold * adjustment)
                )
                adjusted = build(adjusted_threshold)
                if adjusted:
                    raw = adjusted
                    self.contour_threshold = adjusted_threshold
        sampled = _resample_closed(raw, self.outline_samples)
        if immediate or len(self.outline) != len(sampled):
            self.outline = sampled
            return
        start = min(
            range(len(sampled)),
            key=lambda index: (sampled[index] - self.outline[0]).length(),
        )
        aligned = sampled[start:] + sampled[:start]
        filtered = [
            previous * 0.62 + current * 0.38
            for previous, current in zip(self.outline, aligned, strict=True)
        ]
        self.outline = (
            filtered
            if all(
                _polygon_contains(point, filtered, margin=self.spacing * 0.08)
                for point in self.points
            )
            else sampled
        )

    def _solve_obstacles(
        self, obstacles: list[Obstacle], margin: float | None = None
    ) -> int:
        margin = self.spacing * 0.62 if margin is None else margin
        contacts = 0
        for index, point in enumerate(self.points):
            for obstacle in obstacles:
                contact = obstacle.contact_correction(point, margin)
                if contact is None:
                    continue
                correction, _ = contact
                if correction.length() > 1e-9:
                    self.points[index] = point + correction
                    point = self.points[index]
                    contacts += 1
        return contacts

    def _wall_adhesion(
        self,
        point: Vec2,
        obstacles: list[Obstacle],
        travel_direction: Vec2 | None = None,
    ) -> tuple[Vec2, float]:
        """Attrazione locale verso una superficie, senza fissare il punto."""
        adhesion_range = self.spacing * 1.25
        best_vector = Vec2()
        best_weight = 0.0
        for obstacle in obstacles:
            closest = Vec2(
                min(obstacle.right, max(obstacle.left, point.x)),
                min(obstacle.bottom, max(obstacle.top, point.y)),
            )
            delta = point - closest
            distance = delta.length()
            if distance < 1e-9 or distance >= adhesion_range:
                continue
            normal = delta / distance
            if (
                travel_direction is not None
                and abs(normal.dot(travel_direction)) > 0.55
            ):
                # Una parete frontale deve deviare il flusso verso l'apertura,
                # non comportarsi come una superficie adesiva.
                continue
            target = closest + normal * (self.spacing * 0.62)
            weight = 1.0 - distance / adhesion_range
            if weight > best_weight:
                best_vector = target - point
                best_weight = weight
        return best_vector, best_weight

    def _passage(
        self, obstacles: list[Obstacle], target: Vec2
    ) -> tuple[float, float, float, float] | None:
        """Individua una fessura orizzontale fra due rettangoli sovrapposti."""
        for first in obstacles:
            for second in obstacles:
                if first is second:
                    continue
                upper, lower = (
                    (first, second) if first.bottom <= second.top else (second, first)
                )
                overlap_left = max(upper.left, lower.left)
                overlap_right = min(upper.right, lower.right)
                if (
                    upper.bottom < lower.top
                    and overlap_left < overlap_right
                    and target.x > overlap_right
                ):
                    return (
                        overlap_left,
                        overlap_right,
                        upper.bottom,
                        lower.top,
                    )
        return None

    def _clip_outline(self, obstacles: list[Obstacle]) -> None:
        for index, point in enumerate(self.outline):
            corrected = point
            for obstacle in obstacles:
                contact = obstacle.contact_correction(corrected, 0.0)
                if contact is not None and contact[0].length() > 1e-9:
                    corrected = corrected + contact[0]
            self.outline[index] = corrected

    def update(self, dt: float, obstacles: list[Obstacle]) -> int:
        if dt <= 0.0:
            raise ValueError("dt deve essere positivo")
        if self.target is not None:
            target_distance = (self.target - self.center).length()
            if target_distance <= 18.0:
                self.target = None
                self._density_relaxation(dt)
                contacts = self._solve_obstacles(obstacles)
                self._rebuild_outline()
                self._clip_outline(obstacles)
                return contacts
            passage = self._passage(obstacles, self.target)
            movement_target = self.target
            if passage is not None:
                left, right, top, bottom = passage
                if self.center.x < right + self.spacing:
                    movement_target = Vec2(
                        right + self.spacing * 1.5, (top + bottom) * 0.5
                    )
            delta = movement_target - self.center
            distance = delta.length()
            if distance > 1e-9:
                direction = delta / distance
                projections = [(point - self.center).dot(direction) for point in self.points]
                minimum = min(projections)
                maximum = max(projections)
                span = max(maximum - minimum, 1e-9)
                stretch = span / max(self.rest_diameter, 1e-9)
                adhesion = [
                    self._wall_adhesion(point, obstacles, direction)
                    for point in self.points
                ]
                compressed = any(weight > 0.0 for _, weight in adhesion)
                self.gait_time += dt
                if self.gait_time >= self.gait_duration:
                    self.gait_time %= self.gait_duration
                    self.gait_cycle += 1
                    pattern = (3, 4, 3, 5)
                    self.pseudopod_count = pattern[self.gait_cycle % len(pattern)]
                phase = self.gait_time / self.gait_duration
                center = self.center
                lateral_values: list[float] = []
                perpendicular = Vec2(-direction.y, direction.x)
                for point in self.points:
                    lateral_values.append((point - center).dot(perpendicular))
                lateral_extent = max(max(abs(value) for value in lateral_values), 1e-9)
                lobe_centers = tuple(
                    -0.54 + 1.08 * index / (self.pseudopod_count - 1)
                    for index in range(self.pseudopod_count)
                )
                lobe_width = 0.22 + 0.20 / self.pseudopod_count
                for index, point in enumerate(self.points):
                    frontness = (projections[index] - minimum) / span
                    extension_limit = max(0.12, 1.0 - max(0.0, stretch - 1.08) * 5.0)
                    lateral_position = lateral_values[index] / lateral_extent
                    lobe = max(
                        max(0.0, 1.0 - abs(lateral_position - lobe_center) / lobe_width)
                        for lobe_center in lobe_centers
                    )
                    if phase < 0.30:
                        front_gate = min(1.0, max(0.0, (frontness - 0.58) / 0.30))
                        movement_weight = 0.02 + 1.48 * front_gate * lobe * extension_limit
                        active_shape = front_gate * lobe
                    elif phase < 0.72:
                        transfer = (phase - 0.30) / 0.42
                        wave_center = 0.88 - 0.78 * transfer
                        band = max(0.0, 1.0 - abs(frontness - wave_center) / 0.34)
                        movement_weight = 0.10 + 1.18 * band
                        active_shape = band
                    else:
                        recovery = (phase - 0.72) / 0.28
                        rear_gate = (1.0 - frontness) * (0.55 + 0.45 * recovery)
                        movement_weight = 0.12 + 1.42 * rear_gate
                        active_shape = rear_gate
                    movement_weight += (
                        max(0.0, stretch - 1.04) * (1.0 - frontness) * 4.2
                    )
                    adhesion_vector, adhesion_weight = adhesion[index]
                    movement_weight *= 1.0 - 0.45 * adhesion_weight
                    forward = direction * (self.speed * dt * movement_weight)
                    relative = point - center
                    lateral = relative - direction * relative.dot(direction)
                    narrowing = (
                        lateral * (-0.045 * active_shape * dt)
                        if compressed
                        else Vec2()
                    )
                    adhesion_motion = adhesion_vector * min(
                        1.0, 5.5 * adhesion_weight * dt
                    )
                    steering = Vec2()
                    if passage is not None:
                        left, right, top, bottom = passage
                        if left - self.spacing * 2.5 < point.x < right:
                            safe_top = top + self.spacing * 0.70
                            safe_bottom = bottom - self.spacing * 0.70
                            if point.y < safe_top or point.y > safe_bottom:
                                gap_center = (top + bottom) * 0.5
                                steering = Vec2(
                                    0.0,
                                    (gap_center - point.y)
                                    * min(1.0, 3.8 * dt),
                                )
                    self.points[index] = (
                        point + forward + narrowing + adhesion_motion + steering
                    )
                if compressed:
                    redistributed_center = self.center
                    axial_coordinates = [
                        (point - redistributed_center).dot(direction)
                        for point in self.points
                    ]
                    lateral_coordinates = [
                        (point - redistributed_center).dot(perpendicular)
                        for point in self.points
                    ]
                    axial_span = max(axial_coordinates) - min(axial_coordinates)
                    lateral_span = max(lateral_coordinates) - min(lateral_coordinates)
                    desired_axial_span = min(
                        self.rest_diameter * 2.2,
                        self.rest_diameter * self.rest_diameter
                        / max(lateral_span, self.rest_diameter * 0.42),
                    )
                    if desired_axial_span > axial_span > 1e-9:
                        expansion = desired_axial_span / axial_span - 1.0
                        response = min(1.0, 6.0 * dt)
                        for index, axial in enumerate(axial_coordinates):
                            self.points[index] = self.points[index] + direction * (
                                axial * expansion * response
                            )
        self._density_relaxation(dt)
        contacts = self._solve_obstacles(obstacles)
        self.rebalance_timer += dt
        if self.rebalance_timer >= 0.24:
            self.rebalance_timer = 0.0
            self._rebalance_points()
            contacts += self._solve_obstacles(obstacles)
        self.outline_timer += dt
        if self.outline_timer >= 1.0 / 30.0:
            self.outline_timer = 0.0
            self._rebuild_outline()
            self._clip_outline(obstacles)
        return contacts
