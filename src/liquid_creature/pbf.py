"""Position Based Fluids 2D minimale per il corpo della creatura."""

from __future__ import annotations

from dataclasses import dataclass, field
from math import cos, exp, pi, sin, sqrt

from .physics import Vec2
from .world import Obstacle


@dataclass(slots=True)
class PBFConfig:
    particle_spacing: float = 10.0
    smoothing_radius: float = 20.0
    solver_iterations: int = 4
    # Il termine epsilon e espresso nella scala dei gradienti normalizzati.
    # Un valore alto renderebbe di fatto inattivo il vincolo di densita.
    relaxation: float = 0.03
    damping: float = 0.997
    viscosity: float = 0.085
    artificial_pressure: float = 0.002
    pressure_power: int = 4
    collision_margin: float = 4.0
    wall_friction: float = 0.10
    wall_slide_speed: float = 38.0
    cohesion_strength: float = 2.0
    maximum_cohesion_acceleration: float = 24.0
    fragment_recovery_acceleration: float = 125.0
    max_speed: float = 95.0
    drive_speed: float = 52.0
    drive_response: float = 9.0
    extension_duration: float = 0.52
    grip_duration: float = 0.20
    pull_duration: float = 0.58
    release_duration: float = 0.28
    adhesion_strength: float = 0.86
    rear_contraction_speed: float = 31.0
    outward_tail_damping: float = 0.90
    release_substrate_drag: float = 0.42
    rest_substrate_drag: float = 0.18
    shape_recovery_strength: float = 7.5
    cortical_recovery_strength: float = 3.8
    moving_shape_memory: float = 0.20
    maximum_shape_acceleration: float = 85.0
    turn_recovery_duration: float = 0.80
    locomotion_speed_multiplier: float = 1.0
    manual_pseudopod_acceleration: float = 650.0
    manual_pseudopod_reach_ratio: float = 0.45
    manual_pseudopod_mass_fraction: float = 0.42
    manual_pseudopod_limit_recovery: float = 18.0
    maximum_perimeter_ratio: float = 1.48
    perimeter_recovery_strength: float = 52.0
    nucleus_radius_ratio: float = 0.23
    nucleus_maximum_aspect: float = 1.65


@dataclass(slots=True)
class PBFDiagnostics:
    average_density_ratio: float = 1.0
    maximum_density_error: float = 0.0
    contacts: int = 0
    connected_particles: int = 0
    locomotion_phase: str = "riposo"
    pseudopod_count: int = 0
    perimeter_ratio: float = 1.0
    nucleus_aspect: float = 1.0


@dataclass(slots=True)
class PBFCreature:
    positions: list[Vec2]
    velocities: list[Vec2]
    predicted: list[Vec2]
    config: PBFConfig = field(default_factory=PBFConfig)
    rest_density: float = 1.0
    target: Vec2 | None = None
    locomotion_time: float = 0.0
    locomotion_phase: str = "riposo"
    pseudopod_count: int = 0
    locomotion_cycle: int = -1
    reference_variance: float = 1.0
    reference_radius: float = 1.0
    reference_perimeter: float = 1.0
    travel_direction: Vec2 = field(default_factory=Vec2)
    turn_recovery_time: float = 0.0
    adhesion_weights: list[float] = field(default_factory=list)
    manual_pseudopod_count: int = 0
    manual_pseudopod_extension: float = 0.0
    diagnostics: PBFDiagnostics = field(default_factory=PBFDiagnostics)
    _neighbors: list[list[int]] = field(default_factory=list)
    _densities: list[float] = field(default_factory=list)
    _lambdas: list[float] = field(default_factory=list)

    @classmethod
    def create(
        cls,
        center: Vec2 | None = None,
        radius: float = 57.0,
        config: PBFConfig | None = None,
    ) -> PBFCreature:
        center = center or Vec2(215.0, 310.0)
        config = config or PBFConfig()
        spacing = config.particle_spacing
        row_height = spacing * sqrt(3.0) * 0.5
        points: list[Vec2] = []
        row = 0
        y = -radius
        while y <= radius:
            offset = 0.5 * spacing if row % 2 else 0.0
            x = -radius
            while x <= radius:
                local = Vec2(x + offset, y)
                if local.length() <= radius:
                    angle = 0.17 * x + 0.11 * y
                    local = local + Vec2(cos(angle), sin(angle)) * (0.035 * spacing)
                    points.append(center + local)
                x += spacing
            y += row_height
            row += 1
        body = cls(
            positions=points,
            velocities=[Vec2() for _ in points],
            predicted=[Vec2(point.x, point.y) for point in points],
            config=config,
        )
        body._resize_work_buffers()
        body.adhesion_weights = [0.0 for _ in points]
        body._rebuild_neighbors()
        initial_densities = body._compute_densities()
        ordered = sorted(initial_densities)
        body.rest_density = ordered[len(ordered) // 2]
        body.reference_variance = body._mean_axis_variance()
        body.reference_radius = 2.0 * sqrt(body.reference_variance)
        body.reference_perimeter = body._particle_hull_perimeter(body.positions)
        return body

    @property
    def particle_count(self) -> int:
        return len(self.positions)

    @property
    def center(self) -> Vec2:
        return sum(self.positions, Vec2()) / len(self.positions) if self.positions else Vec2()

    @property
    def perimeter_ratio(self) -> float:
        if self.reference_perimeter <= 1e-9:
            return 1.0
        return self._particle_hull_perimeter(self.positions) / self.reference_perimeter

    @property
    def nucleus_axis(self) -> Vec2:
        length = self.travel_direction.length()
        return self.travel_direction / length if length > 1e-9 else Vec2(1.0, 0.0)

    @property
    def nucleus_axes(self) -> tuple[float, float]:
        """Semiassi del nucleo; l'area resta costante durante la deformazione."""
        axis = self.nucleus_axis
        perpendicular = Vec2(-axis.y, axis.x)
        forward = [point.dot(axis) for point in self.positions]
        lateral = [point.dot(perpendicular) for point in self.positions]
        forward_span = max(forward) - min(forward)
        lateral_span = max(lateral) - min(lateral)
        body_aspect = forward_span / max(lateral_span, 1e-9)
        aspect_ratio = min(
            self.config.nucleus_maximum_aspect,
            max(1.0, body_aspect),
        )
        stretch = sqrt(aspect_ratio)
        radius = self.reference_radius * self.config.nucleus_radius_ratio
        return radius * stretch, radius / stretch

    def set_target(self, target: Vec2 | None) -> None:
        if target is not None and self.travel_direction.length() > 1e-9:
            delta = target - self.center
            distance = delta.length()
            if distance > 1e-9:
                new_direction = delta / distance
                if new_direction.dot(self.travel_direction) < 0.78:
                    self.turn_recovery_time = self.config.turn_recovery_duration
        self.target = target
        if target is None:
            self.locomotion_time = 0.0
            self.locomotion_phase = "riposo"
            self.pseudopod_count = 0
            self.locomotion_cycle = -1
            self.adhesion_weights = [0.0 for _ in self.positions]

    def extend_pseudopod_towards(self, target: Vec2, dt: float) -> int:
        """Spinge solo un piccolo settore periferico, senza traslare il corpo."""
        if dt <= 0.0 or not self.positions:
            self.manual_pseudopod_count = 0
            self.manual_pseudopod_extension = 0.0
            return 0
        delta = target - self.center
        distance = delta.length()
        if distance <= 1e-9:
            self.manual_pseudopod_count = 0
            self.manual_pseudopod_extension = 0.0
            return 0
        direction = delta / distance
        perpendicular = Vec2(-direction.y, direction.x)
        scale = max(
            self.config.particle_spacing * 3.0,
            max((point - self.center).length() for point in self.positions),
        )
        weights: list[float] = []
        for point in self.positions:
            local = point - self.center
            forward = local.dot(direction) / scale
            lateral = abs(local.dot(perpendicular)) / scale
            front = min(1.0, max(0.0, (forward - 0.05) / 0.90))
            narrowness = exp(-((lateral / 0.28) ** 2))
            weights.append(front**1.4 * narrowness)
        candidate_indices = sorted(
            range(len(weights)), key=weights.__getitem__, reverse=True
        )
        mass_limit = max(
            3,
            round(len(self.positions) * self.config.manual_pseudopod_mass_fraction),
        )
        active = [
            index
            for index in candidate_indices[:mass_limit]
            if weights[index] > 0.08
        ]
        if not active:
            self.manual_pseudopod_count = 0
            self.manual_pseudopod_extension = 0.0
            return 0
        active_set = set(active)
        normal_front = self.reference_radius
        maximum_front = normal_front * (1.0 + self.config.manual_pseudopod_reach_ratio)
        impulses: list[Vec2] = []
        maximum_projection = normal_front
        for index, point in enumerate(self.positions):
            if index not in active_set:
                impulses.append(Vec2())
                continue
            projection = (point - self.center).dot(direction)
            maximum_projection = max(maximum_projection, projection)
            remaining = maximum_front - projection
            limit_fade = min(1.0, max(0.0, remaining / (normal_front * 0.24)))
            outward = self.config.manual_pseudopod_acceleration * weights[index] * limit_fade
            impulse = direction * (outward * dt)
            if remaining <= 0.0:
                outward_speed = self.velocities[index].dot(direction)
                impulse = impulse - direction * (
                    max(0.0, outward_speed)
                    + (-remaining) * self.config.manual_pseudopod_limit_recovery * dt
                )
            impulses.append(impulse)
        mean_impulse = sum(impulses, Vec2()) / len(impulses)
        for index, impulse in enumerate(impulses):
            self.velocities[index] = self.velocities[index] + impulse - mean_impulse
        self.manual_pseudopod_count = len(active)
        self.manual_pseudopod_extension = max(0.0, maximum_projection - normal_front)
        return len(active)

    def release_manual_pseudopod(self) -> None:
        """Interrompe la spinta; il recupero di forma richiama gradualmente il lobo."""
        self.manual_pseudopod_count = 0

    def step(self, dt: float, obstacles: list[Obstacle]) -> None:
        if dt <= 0.0 or not self.positions:
            return
        self._resize_work_buffers()
        self.predicted = [Vec2(point.x, point.y) for point in self.positions]
        self._rebuild_neighbors()
        cohesion = self._cohesion_accelerations()
        recovery = self._fragment_recovery_accelerations()
        shape_recovery = self._shape_recovery_accelerations()
        membrane_recovery = self._perimeter_recovery_accelerations()
        direction = Vec2()
        target_distance = 0.0
        if self.target is not None:
            delta = self.target - self.center
            target_distance = delta.length()
            if target_distance > 10.0:
                direction = delta / target_distance
                self.travel_direction = direction
                self.locomotion_time += dt
                self._update_locomotion_phase()
            else:
                self.set_target(None)

        desired_velocities, adhesion = self._locomotion_field(
            direction, target_distance
        )
        self.adhesion_weights = adhesion
        if self.turn_recovery_time > 0.0:
            recovery_ratio = min(
                1.0,
                self.turn_recovery_time / self.config.turn_recovery_duration,
            )
            desired_velocities = [
                velocity * (1.0 - 0.72 * recovery_ratio)
                for velocity in desired_velocities
            ]
            self.turn_recovery_time = max(0.0, self.turn_recovery_time - dt)

        response = min(1.0, self.config.drive_response * dt)
        previous_mean_velocity = sum(self.velocities, Vec2()) / len(self.velocities)
        desired_mean_velocity = (
            sum(desired_velocities, Vec2()) / len(desired_velocities)
        )
        expected_mean_velocity = (
            previous_mean_velocity
            + (desired_mean_velocity - previous_mean_velocity) * response
        ) * self.config.damping
        for index, velocity in enumerate(self.velocities):
            # Servo di velocita: le particelle libere non accelerano senza
            # limite mentre quelle all'imbocco sono trattenute dal muro.
            new_velocity = (
                velocity
                + (desired_velocities[index] - velocity) * response
                + (
                    cohesion[index]
                    + recovery[index]
                    + shape_recovery[index]
                    + membrane_recovery[index]
                )
                * dt
            ) * self.config.damping
            new_velocity = new_velocity * (
                1.0 - adhesion[index] * self.config.adhesion_strength
            )
            speed = new_velocity.length()
            if speed > self.config.max_speed:
                new_velocity = new_velocity * (self.config.max_speed / speed)
            self.velocities[index] = new_velocity
        actual_mean_velocity = sum(self.velocities, Vec2()) / len(self.velocities)
        mean_error = actual_mean_velocity - expected_mean_velocity
        self.velocities = [velocity - mean_error for velocity in self.velocities]
        self.predicted = [
            position + velocity * dt
            for position, velocity in zip(self.positions, self.velocities, strict=True)
        ]
        integrated_center = sum(self.predicted, Vec2()) / len(self.predicted)
        integrated_mean_velocity = sum(self.velocities, Vec2()) / len(self.velocities)

        contacts = 0
        for _ in range(self.config.solver_iterations):
            self._rebuild_neighbors()
            self._densities = self._compute_densities()
            self._compute_lambdas()
            for index, correction in enumerate(self._position_corrections()):
                self.predicted[index] = self.predicted[index] + correction
            contacts += self._solve_collisions(obstacles, dt)
            contacts += self._solve_membrane_segment_collisions(obstacles)
            contacts += self._solve_nucleus_collisions(obstacles)

        # Le correzioni interne di densita e coesione non devono modificare
        # la quantita di moto complessiva. Le collisioni, invece, sono esterne.
        if contacts == 0:
            solved_center = sum(self.predicted, Vec2()) / len(self.predicted)
            center_error = solved_center - integrated_center
            self.predicted = [point - center_error for point in self.predicted]

        inverse_dt = 1.0 / dt
        raw_velocities = [
            (predicted - position) * inverse_dt
            for position, predicted in zip(self.positions, self.predicted, strict=True)
        ]
        smoothed: list[Vec2] = []
        for index, velocity in enumerate(raw_velocities):
            correction = Vec2()
            weight_sum = 0.0
            for neighbor in self._neighbors[index]:
                if neighbor == index:
                    continue
                weight = self._kernel(self.predicted[index] - self.predicted[neighbor])
                correction = correction + (raw_velocities[neighbor] - velocity) * weight
                weight_sum += weight
            if weight_sum > 1e-12:
                velocity = velocity + correction * (self.config.viscosity / weight_sum)
            smoothed.append(velocity)
        if contacts == 0:
            smoothed_mean = sum(smoothed, Vec2()) / len(smoothed)
            velocity_error = smoothed_mean - integrated_mean_velocity
            smoothed = [velocity - velocity_error for velocity in smoothed]
        if direction.length() > 1e-9:
            smoothed = self._damp_outward_tail_motion(smoothed, direction)
        smoothed = self._apply_substrate_drag(smoothed)

        self.positions = [Vec2(point.x, point.y) for point in self.predicted]
        self.velocities = smoothed
        density_ratios = [density / self.rest_density for density in self._densities]
        self.diagnostics = PBFDiagnostics(
            average_density_ratio=sum(density_ratios) / len(density_ratios),
            maximum_density_error=max(abs(ratio - 1.0) for ratio in density_ratios),
            contacts=contacts,
            connected_particles=self._largest_connected_component(),
            locomotion_phase=self.locomotion_phase,
            pseudopod_count=self.pseudopod_count,
            perimeter_ratio=self.perimeter_ratio,
            nucleus_aspect=self.nucleus_axes[0] / self.nucleus_axes[1],
        )

    def _update_locomotion_phase(self) -> None:
        config = self.config
        cycle_duration = (
            config.extension_duration
            + config.grip_duration
            + config.pull_duration
            + config.release_duration
        )
        cycle = int(self.locomotion_time / cycle_duration)
        local_time = self.locomotion_time - cycle * cycle_duration
        if cycle != self.locomotion_cycle:
            self.locomotion_cycle = cycle
            self.pseudopod_count = self._select_pseudopod_count(
                self.travel_direction
            )
        if local_time < config.extension_duration:
            self.locomotion_phase = "estensione"
        elif local_time < config.extension_duration + config.grip_duration:
            self.locomotion_phase = "adesione"
        elif local_time < (
            config.extension_duration + config.grip_duration + config.pull_duration
        ):
            self.locomotion_phase = "trazione"
        else:
            self.locomotion_phase = "rilascio"

    def _select_pseudopod_count(self, direction: Vec2) -> int:
        """Sceglie quanti lobi possono occupare il fronte senza sovrapporsi."""
        if direction.length() <= 1e-9:
            return 1
        center = self.center
        perpendicular = Vec2(-direction.y, direction.x)
        front = [
            (point - center).dot(perpendicular)
            for index, point in enumerate(self.positions)
            if len(self._neighbors[index]) <= 13
            and (point - center).dot(direction) > -self.reference_radius * 0.08
        ]
        if len(front) < 2:
            return 1
        usable_width = max(front) - min(front)
        count = round(usable_width / (self.config.particle_spacing * 2.8))
        size_limit = max(1, min(4, round(self.reference_radius / 18.0)))
        return min(size_limit, max(1, count))

    def _locomotion_field(
        self, direction: Vec2, target_distance: float
    ) -> tuple[list[Vec2], list[float]]:
        count = len(self.positions)
        if direction.length() <= 1e-9 or target_distance <= 10.0:
            return [Vec2() for _ in range(count)], [0.0 for _ in range(count)]
        center = self.center
        perpendicular = Vec2(-direction.y, direction.x)
        scale = max((point - center).length() for point in self.positions)
        scale = max(scale, self.config.particle_spacing * 3.0)
        cycle_duration = (
            self.config.extension_duration
            + self.config.grip_duration
            + self.config.pull_duration
            + self.config.release_duration
        )
        cycle = int(self.locomotion_time / cycle_duration)
        wobble = 0.045 * sin(cycle * 2.17 + 0.8)
        if self.pseudopod_count <= 1:
            offsets = (wobble,)
        else:
            span = min(0.48, 0.20 + 0.12 * (self.pseudopod_count - 1))
            offsets = tuple(
                -span
                + 2.0 * span * index / (self.pseudopod_count - 1)
                + wobble * sin(index * 1.71 + cycle)
                for index in range(self.pseudopod_count)
            )

        velocities: list[Vec2] = []
        adhesion: list[float] = []
        for point in self.positions:
            local = point - center
            forward = local.dot(direction) / scale
            lateral = local.dot(perpendicular) / scale
            frontness = min(1.0, max(0.0, (forward + 0.18) / 1.05))
            best_activity = 0.0
            best_offset = 0.0
            for offset in offsets:
                lateral_activity = exp(-((lateral - offset) / 0.19) ** 2)
                activity = lateral_activity * frontness**1.7
                if activity > best_activity:
                    best_activity = activity
                    best_offset = offset

            phase = self.locomotion_phase
            if phase == "estensione":
                lateral_push = perpendicular * (best_offset * 0.10 * best_activity)
                heading = direction + lateral_push
                heading_length = heading.length()
                if heading_length > 1e-9:
                    heading = heading / heading_length
                speed = self.config.drive_speed * (0.10 + 0.90 * best_activity)
                velocity = heading * speed
                grip = 0.0
            elif phase == "adesione":
                velocity = direction * (8.0 * (1.0 - best_activity))
                grip = best_activity
            elif phase == "trazione":
                rear_weight = (1.0 - frontness) ** 1.3
                # La coda non viene semplicemente spinta: converge verso
                # l'asse di avanzamento e segue l'onda senza raggiungere una
                # zona centrale gia piena. Il moto in avanti cresce dal retro
                # verso il centro/fronte, non il contrario.
                inward = -lateral * self.config.rear_contraction_speed * rear_weight
                forward_speed = 10.0 + 24.0 * frontness
                velocity = (
                    direction * forward_speed
                    + perpendicular * inward
                )
                grip = best_activity * 0.92
            else:
                local_release = (
                    self.locomotion_time % cycle_duration
                    - self.config.extension_duration
                    - self.config.grip_duration
                    - self.config.pull_duration
                ) / self.config.release_duration
                release = min(1.0, max(0.0, local_release))
                rear_weight = (1.0 - frontness) ** 1.3
                inward = (
                    -lateral
                    * self.config.rear_contraction_speed
                    * rear_weight
                    * (1.0 - release)
                )
                # Il rilascio non e una traslazione: la massa termina il
                # richiamo e scarica velocita sul substrato.
                follow = 0.35 + 0.65 * frontness
                velocity = (
                    direction * (7.0 * (1.0 - release) * follow)
                    + perpendicular * inward
                )
                grip = best_activity * (1.0 - release) ** 2
            velocity = velocity * self.config.locomotion_speed_multiplier
            velocities.append(velocity)
            adhesion.append(grip)
        return velocities, adhesion

    @staticmethod
    def _particle_hull_indices(points: list[Vec2]) -> list[int]:
        if len(points) <= 2:
            return list(range(len(points)))
        ordered = sorted(range(len(points)), key=lambda index: (points[index].x, points[index].y))

        def cross(first: int, second: int, third: int) -> float:
            a = points[second] - points[first]
            b = points[third] - points[first]
            return a.x * b.y - a.y * b.x

        lower: list[int] = []
        for index in ordered:
            while len(lower) >= 2 and cross(lower[-2], lower[-1], index) <= 0.0:
                lower.pop()
            lower.append(index)
        upper: list[int] = []
        for index in reversed(ordered):
            while len(upper) >= 2 and cross(upper[-2], upper[-1], index) <= 0.0:
                upper.pop()
            upper.append(index)
        return lower[:-1] + upper[:-1]

    @classmethod
    def _particle_hull_perimeter(cls, points: list[Vec2]) -> float:
        hull = cls._particle_hull_indices(points)
        if len(hull) < 2:
            return 0.0
        return sum(
            (points[hull[(index + 1) % len(hull)]] - points[hull[index]]).length()
            for index in range(len(hull))
        )

    def _perimeter_recovery_accelerations(self) -> list[Vec2]:
        """Simula una membrana finita senza introdurre legami permanenti."""
        accelerations = [Vec2() for _ in self.positions]
        ratio = self.perimeter_ratio
        excess = ratio - self.config.maximum_perimeter_ratio
        if excess <= 0.0:
            return accelerations
        center = self.center
        hull = self._particle_hull_indices(self.positions)
        for index in hull:
            inward = center - self.positions[index]
            distance = inward.length()
            if distance > 1e-9:
                accelerations[index] = inward * (
                    self.config.perimeter_recovery_strength * excess / distance
                )
        mean = sum(accelerations, Vec2()) / len(accelerations)
        return [acceleration - mean for acceleration in accelerations]

    def _mean_axis_variance(self) -> float:
        center = self.center
        return (
            sum((point - center).dot(point - center) for point in self.positions)
            / (2.0 * len(self.positions))
        )

    def _shape_recovery_accelerations(self) -> list[Vec2]:
        """Recupero affine della distribuzione, senza corrispondenze tra punti."""
        count = len(self.positions)
        if count == 0:
            return []
        if self.target is None:
            activation = 1.0
        elif self.turn_recovery_time > 0.0:
            activation = min(
                1.0,
                self.turn_recovery_time / self.config.turn_recovery_duration,
            )
        elif self.diagnostics.contacts > 0:
            # Nella strettoia la memoria non deve combattere le pareti.
            activation = 0.015
        else:
            activation = self.config.moving_shape_memory
        center = self.center
        xx = yy = xy = 0.0
        for point in self.positions:
            local = point - center
            xx += local.x * local.x
            yy += local.y * local.y
            xy += local.x * local.y
        xx /= count
        yy /= count
        xy /= count
        trace_half = 0.5 * (xx + yy)
        difference_half = 0.5 * (xx - yy)
        radius = sqrt(max(0.0, difference_half * difference_half + xy * xy))
        first_variance = trace_half + radius
        second_variance = trace_half - radius
        if abs(xy) > 1e-9 or abs(first_variance - xx) > 1e-9:
            first_axis = Vec2(xy, first_variance - xx)
            axis_length = first_axis.length()
            first_axis = first_axis / axis_length if axis_length > 1e-9 else Vec2(1.0, 0.0)
        else:
            first_axis = Vec2(1.0, 0.0)
        second_axis = Vec2(-first_axis.y, first_axis.x)
        reference = max(self.reference_variance, 1e-6)
        first_error = first_variance / reference - 1.0
        second_error = second_variance / reference - 1.0
        result: list[Vec2] = []
        for point in self.positions:
            local = point - center
            acceleration = (
                first_axis * (-first_error * local.dot(first_axis))
                + second_axis * (-second_error * local.dot(second_axis))
            ) * (self.config.shape_recovery_strength * activation)
            magnitude = acceleration.length()
            if magnitude > self.config.maximum_shape_acceleration:
                acceleration = acceleration * (
                    self.config.maximum_shape_acceleration / magnitude
                )
            result.append(acceleration)
        # La covarianza corregge soltanto gli assi globali. Questa componente
        # corticale agisce sui punti esterni e impedisce profili triangolari.
        surface_minimum_radius = self.reference_radius * 0.68
        for index, point in enumerate(self.positions):
            local = point - center
            distance = local.length()
            is_surface = (
                distance >= surface_minimum_radius
                and len(self._neighbors[index]) <= 13
            )
            if not is_surface or distance <= 1e-9:
                continue
            radial_error = self.reference_radius - distance
            cortical = local * (
                radial_error
                * self.config.cortical_recovery_strength
                * activation
                / distance
            )
            result[index] = result[index] + cortical
        return result

    def _damp_outward_tail_motion(
        self, velocities: list[Vec2], direction: Vec2
    ) -> list[Vec2]:
        """Assorbe inerzia laterale della coda senza imporre una forma."""
        center = self.center
        perpendicular = Vec2(-direction.y, direction.x)
        scale = max((point - center).length() for point in self.positions)
        scale = max(scale, self.config.particle_spacing)
        result: list[Vec2] = []
        for point, velocity in zip(self.positions, velocities, strict=True):
            local = point - center
            forward = local.dot(direction) / scale
            lateral = local.dot(perpendicular)
            transverse_speed = velocity.dot(perpendicular)
            is_tail = min(1.0, max(0.0, -forward + 0.10))
            is_outward = lateral * transverse_speed > 0.0
            if is_outward and is_tail > 0.0:
                retained = 1.0 - self.config.outward_tail_damping * is_tail
                velocity = (
                    velocity
                    - perpendicular * transverse_speed
                    + perpendicular * (transverse_speed * retained)
                )
            result.append(velocity)
        return result

    def _apply_substrate_drag(self, velocities: list[Vec2]) -> list[Vec2]:
        if self.target is None:
            return [
                velocity * (1.0 - self.config.rest_substrate_drag)
                for velocity in velocities
            ]
        if self.locomotion_phase != "rilascio":
            return velocities
        cycle_duration = (
            self.config.extension_duration
            + self.config.grip_duration
            + self.config.pull_duration
            + self.config.release_duration
        )
        local_release = (
            self.locomotion_time % cycle_duration
            - self.config.extension_duration
            - self.config.grip_duration
            - self.config.pull_duration
        ) / self.config.release_duration
        progress = min(1.0, max(0.0, local_release))
        retained = 1.0 - self.config.release_substrate_drag * (0.35 + 0.65 * progress)
        return [velocity * retained for velocity in velocities]

    def _resize_work_buffers(self) -> None:
        count = len(self.positions)
        self._neighbors = [[] for _ in range(count)]
        self._densities = [self.rest_density for _ in range(count)]
        self._lambdas = [0.0 for _ in range(count)]

    def _rebuild_neighbors(self) -> None:
        cell_size = self.config.smoothing_radius
        grid: dict[tuple[int, int], list[int]] = {}
        for index, point in enumerate(self.predicted):
            cell = (int(point.x // cell_size), int(point.y // cell_size))
            grid.setdefault(cell, []).append(index)
        radius_squared = cell_size * cell_size
        for index, point in enumerate(self.predicted):
            cell_x = int(point.x // cell_size)
            cell_y = int(point.y // cell_size)
            neighbors: list[int] = []
            for offset_y in (-1, 0, 1):
                for offset_x in (-1, 0, 1):
                    for candidate in grid.get((cell_x + offset_x, cell_y + offset_y), []):
                        delta = point - self.predicted[candidate]
                        if delta.dot(delta) < radius_squared:
                            neighbors.append(candidate)
            self._neighbors[index] = neighbors

    def _kernel(self, delta: Vec2) -> float:
        h = self.config.smoothing_radius
        radius_squared = delta.dot(delta)
        if radius_squared >= h * h:
            return 0.0
        term = h * h - radius_squared
        return 4.0 / (pi * h**8) * term**3

    def _kernel_gradient(self, delta: Vec2) -> Vec2:
        h = self.config.smoothing_radius
        distance = delta.length()
        if distance <= 1e-9 or distance >= h:
            return Vec2()
        magnitude = -30.0 / (pi * h**5) * (h - distance) ** 2
        return delta * (magnitude / distance)

    def _compute_densities(self) -> list[float]:
        return [
            sum(
                self._kernel(self.predicted[index] - self.predicted[neighbor])
                for neighbor in neighbors
            )
            for index, neighbors in enumerate(self._neighbors)
        ]

    def _compute_lambdas(self) -> None:
        for index, neighbors in enumerate(self._neighbors):
            constraint = self._densities[index] / self.rest_density - 1.0
            gradient_sum = Vec2()
            squared_norms = 0.0
            for neighbor in neighbors:
                if neighbor == index:
                    continue
                gradient = self._kernel_gradient(
                    self.predicted[index] - self.predicted[neighbor]
                ) / self.rest_density
                squared_norms += gradient.dot(gradient)
                gradient_sum = gradient_sum + gradient
            squared_norms += gradient_sum.dot(gradient_sum)
            self._lambdas[index] = -constraint / (
                squared_norms + self.config.relaxation
            )

    def _position_corrections(self) -> list[Vec2]:
        reference = self._kernel(Vec2(self.config.smoothing_radius * 0.3, 0.0))
        corrections: list[Vec2] = []
        for index, neighbors in enumerate(self._neighbors):
            correction = Vec2()
            for neighbor in neighbors:
                if neighbor == index:
                    continue
                delta = self.predicted[index] - self.predicted[neighbor]
                ratio = self._kernel(delta) / reference if reference > 0.0 else 0.0
                artificial = -self.config.artificial_pressure * ratio ** self.config.pressure_power
                correction = correction + self._kernel_gradient(delta) * (
                    self._lambdas[index] + self._lambdas[neighbor] + artificial
                )
            corrections.append(correction / self.rest_density)
        return corrections

    def _cohesion_accelerations(self) -> list[Vec2]:
        """Tensione superficiale dinamica, senza coppie permanenti."""
        accelerations = [Vec2() for _ in self.predicted]
        rest_distance = self.config.particle_spacing * 1.04
        for index, neighbors in enumerate(self._neighbors):
            for neighbor in neighbors:
                if neighbor <= index:
                    continue
                delta = self.predicted[neighbor] - self.predicted[index]
                distance = delta.length()
                if distance <= rest_distance or distance <= 1e-9:
                    continue
                acceleration = min(
                    self.config.maximum_cohesion_acceleration,
                    (distance - rest_distance) * self.config.cohesion_strength,
                )
                pull = delta * (0.5 * acceleration / distance)
                accelerations[index] = accelerations[index] + pull
                accelerations[neighbor] = accelerations[neighbor] - pull
        return accelerations

    def _largest_connected_component(self) -> int:
        return max((len(component) for component in self._connected_components()), default=0)

    def _connected_components(self) -> list[list[int]]:
        remaining = set(range(len(self.predicted)))
        components: list[list[int]] = []
        while remaining:
            start = remaining.pop()
            stack = [start]
            component = [start]
            while stack:
                current = stack.pop()
                for neighbor in self._neighbors[current]:
                    if neighbor in remaining:
                        remaining.remove(neighbor)
                        stack.append(neighbor)
                        component.append(neighbor)
            components.append(component)
        return components

    def _fragment_recovery_accelerations(self) -> list[Vec2]:
        """Richiama frammenti accidentali senza creare legami persistenti."""
        accelerations = [Vec2() for _ in self.predicted]
        components = self._connected_components()
        if len(components) <= 1:
            return accelerations
        main = max(components, key=len)
        main_set = set(main)
        for component in components:
            if component is main:
                continue
            for index in component:
                nearest = min(
                    main_set,
                    key=lambda candidate: (
                        self.predicted[candidate] - self.predicted[index]
                    ).dot(self.predicted[candidate] - self.predicted[index]),
                )
                delta = self.predicted[nearest] - self.predicted[index]
                distance = delta.length()
                if distance > 1e-9:
                    pull = delta * (
                        self.config.fragment_recovery_acceleration / distance
                    )
                    accelerations[index] = accelerations[index] + pull
                    accelerations[nearest] = accelerations[nearest] - pull
        return accelerations

    def _solve_collisions(self, obstacles: list[Obstacle], dt: float) -> int:
        contacts = 0
        for index, point in enumerate(self.predicted):
            for obstacle in obstacles:
                contact = obstacle.contact_correction(point, self.config.collision_margin)
                if contact is None:
                    continue
                correction, normal = contact
                if correction.length() <= 1e-9:
                    continue
                corrected = point + correction
                displacement = corrected - self.positions[index]
                normal_motion = normal * displacement.dot(normal)
                tangent_motion = displacement - normal_motion
                # Se una particella urta frontalmente l'imbocco, la pressione
                # da sola puo lasciarla appoggiata allo spigolo. La componente
                # tangenziale la fa scorrere verso l'asse corrente del corpo.
                toward_center = self.center - point
                tangent_guide = toward_center - normal * toward_center.dot(normal)
                guide_length = tangent_guide.length()
                guide = Vec2()
                if guide_length > 1e-9:
                    maximum_slide = (
                        self.config.wall_slide_speed
                        * dt
                        / self.config.solver_iterations
                    )
                    guide = tangent_guide * (
                        min(guide_length, maximum_slide) / guide_length
                    )
                self.predicted[index] = (
                    self.positions[index]
                    + normal_motion
                    + tangent_motion * (1.0 - self.config.wall_friction)
                    + guide
                )
                point = self.predicted[index]
                contacts += 1
        return contacts

    def _solve_nucleus_collisions(self, obstacles: list[Obstacle]) -> int:
        """Impedisce al nucleo deformabile di attraversare aperture troppo piccole."""
        if not self.predicted:
            return 0
        axis = self.nucleus_axis
        perpendicular = Vec2(-axis.y, axis.x)
        major, minor = self.nucleus_axes
        extent_x = sqrt((major * axis.x) ** 2 + (minor * perpendicular.x) ** 2)
        extent_y = sqrt((major * axis.y) ** 2 + (minor * perpendicular.y) ** 2)
        contacts = 0
        for obstacle in obstacles:
            center = sum(self.predicted, Vec2()) / len(self.predicted)
            left = obstacle.left - extent_x
            right = obstacle.right + extent_x
            top = obstacle.top - extent_y
            bottom = obstacle.bottom + extent_y
            if not (left <= center.x <= right and top <= center.y <= bottom):
                continue
            candidates = (
                (center.x - left, Vec2(left - center.x, 0.0)),
                (right - center.x, Vec2(right - center.x, 0.0)),
                (center.y - top, Vec2(0.0, top - center.y)),
                (bottom - center.y, Vec2(0.0, bottom - center.y)),
            )
            _, correction = min(candidates, key=lambda candidate: candidate[0])
            if correction.length() <= 1e-9:
                continue
            self.predicted = [point + correction for point in self.predicted]
            contacts += 1
        return contacts

    def _solve_membrane_segment_collisions(
        self, obstacles: list[Obstacle]
    ) -> int:
        """Controlla la pelle tra vicini senza introdurre legami permanenti."""
        contacts = 0
        maximum_span = self.config.particle_spacing * 1.7
        maximum_span_squared = maximum_span * maximum_span
        for first, neighbors in enumerate(self._neighbors):
            for second in neighbors:
                if second <= first:
                    continue
                edge = self.predicted[second] - self.predicted[first]
                if edge.dot(edge) > maximum_span_squared:
                    continue
                for obstacle in obstacles:
                    deepest: tuple[float, Vec2, Vec2, float] | None = None
                    for ratio in (0.25, 0.5, 0.75):
                        sample = self.predicted[first] + edge * ratio
                        contact = obstacle.contact_correction(
                            sample,
                            self.config.collision_margin,
                        )
                        if contact is None:
                            continue
                        correction, normal = contact
                        depth = correction.length()
                        if depth <= 1e-9:
                            continue
                        if deepest is None or depth > deepest[0]:
                            deepest = (depth, correction, normal, ratio)
                    if deepest is None:
                        continue
                    _, correction, _normal, ratio = deepest
                    first_weight = 1.0 - ratio
                    second_weight = ratio
                    denominator = first_weight**2 + second_weight**2
                    self.predicted[first] = (
                        self.predicted[first]
                        + correction * (first_weight / denominator)
                    )
                    self.predicted[second] = (
                        self.predicted[second]
                        + correction * (second_weight / denominator)
                    )
                    contacts += 1
        return contacts
