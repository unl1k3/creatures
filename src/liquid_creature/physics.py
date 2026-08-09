"""Solver XPBD 2D minimale, indipendente dalla visualizzazione."""

from __future__ import annotations

from dataclasses import dataclass, field
from math import cos, pi, sin, sqrt


@dataclass(slots=True)
class Vec2:
    x: float = 0.0
    y: float = 0.0

    def __add__(self, other: Vec2) -> Vec2:
        return Vec2(self.x + other.x, self.y + other.y)

    def __sub__(self, other: Vec2) -> Vec2:
        return Vec2(self.x - other.x, self.y - other.y)

    def __mul__(self, scalar: float) -> Vec2:
        return Vec2(self.x * scalar, self.y * scalar)

    __rmul__ = __mul__

    def __truediv__(self, scalar: float) -> Vec2:
        return Vec2(self.x / scalar, self.y / scalar)

    def dot(self, other: Vec2) -> float:
        return self.x * other.x + self.y * other.y

    def length(self) -> float:
        return sqrt(self.dot(self))


@dataclass(slots=True)
class Particle:
    position: Vec2
    previous: Vec2
    inverse_mass: float = 1.0


@dataclass(slots=True)
class DistanceConstraint:
    a: int
    b: int
    rest_length: float
    compliance: float
    kind: str
    lagrange: float = 0.0
    active: bool = True
    equilibrium_length: float | None = None

    def __post_init__(self) -> None:
        if self.equilibrium_length is None:
            self.equilibrium_length = self.rest_length


@dataclass(slots=True)
class EdgeRefinement:
    edge: int
    particle: int
    original_constraint: int
    detail_constraints: tuple[int, int]
    active: bool = True


@dataclass(slots=True)
class TransientProtrusion:
    center: int
    half_width: int
    strength: float
    lifetime: float
    age: float = 0.0

    @property
    def envelope(self) -> float:
        phase = min(1.0, self.age / self.lifetime)
        return sin(pi * phase)


@dataclass(slots=True)
class SoftTarget:
    point: Vec2
    strength: float


@dataclass(slots=True)
class SoftBody:
    particles: list[Particle]
    constraints: list[DistanceConstraint]
    outer_count: int
    inner_count: int
    core_count: int
    module_count: int
    target_area: float
    area_compliance: float = 8e-6
    damping: float = 0.985
    solver_iterations: int = 20
    max_speed: float = 900.0
    area_lagrange: float = 0.0
    pinned: dict[int, Vec2] = field(default_factory=dict)
    pseudopod_index: int | None = None
    pseudopod_activation: float = 0.0
    pseudopod_target: float = 0.0
    pseudopod_half_width: int = 10
    pseudopod_extension: float = 2.0
    pseudopod_extend_speed: float = 1.6
    pseudopod_retract_speed: float = 1.1
    refinements: dict[int, EdgeRefinement] = field(default_factory=dict)
    transient_protrusions: list[TransientProtrusion] = field(default_factory=list)
    area_growth_goal: float | None = None
    stabilized_center: Vec2 | None = None
    stabilization_time: float = 0.0
    cortex_time: float = 0.0
    soft_targets: dict[int, SoftTarget] = field(default_factory=dict)
    squeeze_activation: float = 0.0
    squeeze_target: float = 0.0
    squeeze_response: float = 1.8
    squeeze_direction: Vec2 = field(default_factory=lambda: Vec2(1.0, 0.0))

    @classmethod
    def create(
        cls,
        center: Vec2 | None = None,
        radius: float = 82.0,
        outer_count: int = 64,
        inner_count: int = 32,
        core_count: int = 12,
        module_count: int = 4,
    ) -> SoftBody:
        if (
            outer_count < 12
            or inner_count % module_count
            or core_count % module_count
        ):
            raise ValueError(
                "inner_count e core_count devono essere multipli di module_count"
            )
        center = center or Vec2(450.0, 310.0)

        outer: list[Vec2] = []
        for i in range(outer_count):
            angle = 2 * pi * i / outer_count
            irregularity = (
                0.070 * sin(2.0 * angle + 0.4)
                + 0.045 * sin(3.0 * angle + 1.7)
                + 0.020 * sin(5.0 * angle + 2.8)
            )
            local_radius = radius * (1.0 + irregularity)
            outer.append(center + Vec2(cos(angle), sin(angle)) * local_radius)
        module_radius = radius * 0.28
        ring_radius = radius * 0.15
        core_radius = radius * 0.055
        ring_per_module = inner_count // module_count
        core_per_module = core_count // module_count
        angle_offsets = (-0.055, 0.035, -0.045, 0.065)
        radial_factors = (1.08, 0.91, 1.02, 0.87)
        module_centers = []
        for module in range(module_count):
            base_angle = 2 * pi * module / module_count
            angle = base_angle + angle_offsets[module % len(angle_offsets)]
            distance = module_radius * radial_factors[module % len(radial_factors)]
            module_centers.append(
                center + Vec2(cos(angle), sin(angle)) * distance
            )
        inner: list[Vec2] = []
        core: list[Vec2] = []
        for module, module_center in enumerate(module_centers):
            module_angle = (
                2 * pi * module / module_count
                + angle_offsets[module % len(angle_offsets)]
            )
            local_ring_radius = ring_radius * (0.92 + 0.05 * (module % 3))
            local_core_radius = core_radius * (0.90 + 0.07 * ((module + 1) % 3))
            for local in range(ring_per_module):
                angle = module_angle + 0.13 * module + 2 * pi * local / ring_per_module
                inner.append(
                    module_center
                    + Vec2(cos(angle), sin(angle)) * local_ring_radius
                )
            for local in range(core_per_module):
                angle = module_angle - 0.09 * module + 2 * pi * local / core_per_module
                core.append(
                    module_center
                    + Vec2(cos(angle), sin(angle)) * local_core_radius
                )
        positions = outer + inner + core
        particles = [Particle(p, Vec2(p.x, p.y)) for p in positions]
        constraints: list[DistanceConstraint] = []

        def connect(a: int, b: int, compliance: float, kind: str) -> None:
            constraints.append(
                DistanceConstraint(
                    a,
                    b,
                    (positions[b] - positions[a]).length(),
                    compliance,
                    kind,
                )
            )

        # Membrana: lati elastici e un secondo vicinato molto morbido che limita le pieghe acute.
        for i in range(outer_count):
            connect(i, (i + 1) % outer_count, 2e-7, "membrane")
            connect(i, (i + 2) % outer_count, 6e-5, "bend")

        inner_offset = outer_count
        core_offset = inner_offset + inner_count
        for module in range(module_count):
            ring_start = inner_offset + module * ring_per_module
            core_start = core_offset + module * core_per_module
            for local in range(ring_per_module):
                connect(
                    ring_start + local,
                    ring_start + (local + 1) % ring_per_module,
                    5e-5,
                    "inner",
                )
                core_local = round(local * core_per_module / ring_per_module) % core_per_module
                connect(
                    ring_start + local,
                    core_start + core_local,
                    2.8e-4,
                    "core_radial",
                )
            for local in range(core_per_module):
                connect(
                    core_start + local,
                    core_start + (local + 1) % core_per_module,
                    9e-5,
                    "core",
                )

        for module in range(module_count):
            following = (module + 1) % module_count
            first_core = core_offset + module * core_per_module
            second_core = core_offset + following * core_per_module
            pair = min(
                (
                    (a, b)
                    for a in range(first_core, first_core + core_per_module)
                    for b in range(second_core, second_core + core_per_module)
                ),
                key=lambda pair: (positions[pair[0]] - positions[pair[1]]).length(),
            )
            connect(pair[0], pair[1], 4.5e-4, "module_bridge")

        for outer_index, outer_point in enumerate(outer):
            nearest_modules = sorted(
                range(module_count),
                key=lambda module: (outer_point - module_centers[module]).length(),
            )[:2]
            for rank, module in enumerate(nearest_modules):
                ring_start = module * ring_per_module
                local = min(
                    range(ring_per_module),
                    key=lambda candidate: (
                        outer_point - inner[ring_start + candidate]
                    ).length(),
                )
                connect(
                    outer_index,
                    inner_offset + ring_start + local,
                    1.5e-4 if rank == 0 else 1.2e-3,
                    "radial" if rank == 0 else "radial_secondary",
                )

        body = cls(
            particles,
            constraints,
            outer_count,
            inner_count,
            core_count,
            module_count,
            abs(cls._signed_area(outer)),
        )
        body.pseudopod_half_width = max(4, round(outer_count / 10))
        return body

    @classmethod
    def create_lattice(
        cls,
        center: Vec2 | None = None,
        radius: float = 82.0,
        outer_count: int = 64,
        spacing: float | None = None,
    ) -> SoftBody:
        """Crea una membrana sostenuta da una rete triangolare distribuita."""
        if outer_count < 12:
            raise ValueError("outer_count deve essere almeno 12")
        center = center or Vec2(450.0, 310.0)
        spacing = spacing or radius * 0.22
        outer: list[Vec2] = []
        for i in range(outer_count):
            angle = 2 * pi * i / outer_count
            irregularity = (
                0.070 * sin(2.0 * angle + 0.4)
                + 0.045 * sin(3.0 * angle + 1.7)
                + 0.020 * sin(5.0 * angle + 2.8)
            )
            outer.append(
                center
                + Vec2(cos(angle), sin(angle)) * radius * (1.0 + irregularity)
            )

        lattice: list[Vec2] = []
        extent = max(2, round(radius / spacing)) + 1
        limit = radius * 0.66
        for row in range(-extent, extent + 1):
            for column in range(-extent, extent + 1):
                local = Vec2(
                    spacing * (column + 0.5 * row),
                    spacing * sqrt(3.0) * 0.5 * row,
                )
                if local.length() <= limit:
                    lattice.append(center + local)

        positions = outer + lattice
        particles = [Particle(point, Vec2(point.x, point.y)) for point in positions]
        constraints: list[DistanceConstraint] = []

        def connect(a: int, b: int, compliance: float, kind: str) -> None:
            constraints.append(
                DistanceConstraint(
                    a,
                    b,
                    (positions[b] - positions[a]).length(),
                    compliance,
                    kind,
                )
            )

        for i in range(outer_count):
            connect(i, (i + 1) % outer_count, 2e-7, "membrane")
            connect(i, (i + 2) % outer_count, 7e-5, "bend")

        lattice_offset = outer_count
        for first in range(len(lattice)):
            for second in range(first + 1, len(lattice)):
                distance = (lattice[second] - lattice[first]).length()
                if distance <= spacing * 1.06:
                    connect(
                        lattice_offset + first,
                        lattice_offset + second,
                        1.4e-4,
                        "lattice",
                    )

        for outer_index, outer_point in enumerate(outer):
            nearest = sorted(
                range(len(lattice)),
                key=lambda index: (outer_point - lattice[index]).length(),
            )[:3]
            for rank, lattice_index in enumerate(nearest):
                connect(
                    outer_index,
                    lattice_offset + lattice_index,
                    1.8e-4 if rank == 0 else 6.0e-4,
                    "attachment" if rank == 0 else "attachment_secondary",
                )

        body = cls(
            particles,
            constraints,
            outer_count,
            len(lattice),
            0,
            0,
            abs(cls._signed_area(outer)),
        )
        body.pseudopod_half_width = max(4, round(outer_count / 10))
        return body

    @staticmethod
    def _signed_area(points: list[Vec2]) -> float:
        return 0.5 * sum(
            point.x * points[(i + 1) % len(points)].y
            - points[(i + 1) % len(points)].x * point.y
            for i, point in enumerate(points)
        )

    @property
    def outer_positions(self) -> list[Vec2]:
        return [particle.position for particle in self.particles[: self.outer_count]]

    @property
    def area(self) -> float:
        return abs(self._signed_area(self.outer_positions))

    @property
    def center(self) -> Vec2:
        points = self.outer_positions
        return Vec2(
            sum(point.x for point in points) / len(points),
            sum(point.y for point in points) / len(points),
        )

    @property
    def inner_center(self) -> Vec2:
        particles = self.particles[self.outer_count : self.outer_count + self.inner_count]
        return Vec2(
            sum(particle.position.x for particle in particles) / len(particles),
            sum(particle.position.y for particle in particles) / len(particles),
        )

    @property
    def core_center(self) -> Vec2:
        if self.core_count == 0:
            return self.inner_center
        start = self.outer_count + self.inner_count
        particles = self.particles[start : start + self.core_count]
        return Vec2(
            sum(particle.position.x for particle in particles) / len(particles),
            sum(particle.position.y for particle in particles) / len(particles),
        )

    @property
    def base_particle_count(self) -> int:
        return self.outer_count + self.inner_count + self.core_count

    def contains_point(self, point: Vec2) -> bool:
        """Restituisce True se il punto è racchiuso dal contorno esterno."""
        points = self.outer_positions
        inside = False
        previous = points[-1]
        for current in points:
            crosses_height = (current.y > point.y) != (previous.y > point.y)
            if crosses_height:
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

    @property
    def active_refinement_indices(self) -> tuple[int, ...]:
        return tuple(
            refinement.particle for refinement in self.refinements.values() if refinement.active
        )

    @property
    def active_particle_count(self) -> int:
        return self.base_particle_count + len(self.active_refinement_indices)

    @property
    def collision_particle_indices(self) -> tuple[int, ...]:
        return tuple(range(self.outer_count)) + self.active_refinement_indices

    def is_refinement_particle(self, index: int) -> bool:
        return any(refinement.particle == index for refinement in self.refinements.values())

    def is_particle_active(self, index: int) -> bool:
        if index < self.base_particle_count:
            return True
        return any(
            refinement.particle == index and refinement.active
            for refinement in self.refinements.values()
        )

    def _membrane_constraint_index(self, edge: int) -> int:
        following = (edge + 1) % self.outer_count
        return next(
            index
            for index, constraint in enumerate(self.constraints)
            if constraint.kind == "membrane" and constraint.a == edge and constraint.b == following
        )

    def activate_refinement(self, edge: int) -> int:
        """Sostituisce un lato della membrana con due lati e un nodo fisico."""
        edge %= self.outer_count
        existing = self.refinements.get(edge)
        first = self.particles[edge].position
        second = self.particles[(edge + 1) % self.outer_count].position
        midpoint = (first + second) * 0.5
        if existing is not None:
            if existing.active:
                return existing.particle
            existing.active = True
            particle = self.particles[existing.particle]
            particle.position = Vec2(midpoint.x, midpoint.y)
            particle.previous = Vec2(midpoint.x, midpoint.y)
            self.constraints[existing.original_constraint].active = False
            for index in existing.detail_constraints:
                self.constraints[index].active = True
            return existing.particle

        original_index = self._membrane_constraint_index(edge)
        original = self.constraints[original_index]
        original.active = False
        particle_index = len(self.particles)
        self.particles.append(Particle(midpoint, Vec2(midpoint.x, midpoint.y)))
        detail_indices: list[int] = []
        for a, b in ((edge, particle_index), (particle_index, (edge + 1) % self.outer_count)):
            detail_indices.append(len(self.constraints))
            self.constraints.append(
                DistanceConstraint(
                    a,
                    b,
                    original.rest_length * 0.5,
                    original.compliance,
                    "refined_membrane",
                )
            )
        self.refinements[edge] = EdgeRefinement(
            edge,
            particle_index,
            original_index,
            (detail_indices[0], detail_indices[1]),
        )
        return particle_index

    def deactivate_refinement(self, edge: int) -> None:
        refinement = self.refinements.get(edge % self.outer_count)
        if refinement is None or not refinement.active:
            return
        refinement.active = False
        original = self.constraints[refinement.original_constraint]
        first = self.particles[original.a].position
        second = self.particles[original.b].position
        # Riparte dalla lunghezza corrente e recupera poi lentamente la tensione,
        # evitando il contraccolpo dovuto alla rimozione del nodo temporaneo.
        original.rest_length = (second - first).length()
        original.active = True
        for index in refinement.detail_constraints:
            self.constraints[index].active = False
        particle = self.particles[refinement.particle]
        particle.previous = Vec2(particle.position.x, particle.position.y)

    def update_adaptive_refinement(
        self,
        focuses: list[Vec2],
        radius: float = 52.0,
        max_active: int = 80,
    ) -> None:
        """Attiva nodi temporanei sui lati vicini alle zone di interesse."""
        radius_squared = radius * radius
        candidates: list[tuple[float, int]] = []
        for edge in range(self.outer_count):
            previous = self.particles[(edge - 1) % self.outer_count].position
            first = self.particles[edge].position
            second = self.particles[(edge + 1) % self.outer_count].position
            midpoint = (first + second) * 0.5
            nearest = min(
                ((midpoint - focus).dot(midpoint - focus) for focus in focuses),
                default=float("inf"),
            )
            if nearest <= radius_squared:
                candidates.append((nearest, edge))
                continue
            incoming = previous - first
            outgoing = second - first
            incoming_length = incoming.length()
            outgoing_length = outgoing.length()
            corner_cosine = (
                incoming.dot(outgoing) / (incoming_length * outgoing_length)
                if incoming_length > 1e-9 and outgoing_length > 1e-9
                else -1.0
            )
            membrane = self.constraints[self._membrane_constraint_index(edge)]
            equilibrium = membrane.equilibrium_length or membrane.rest_length
            stretched = outgoing_length > equilibrium * 1.35
            sharply_curved = corner_cosine > -0.94
            if stretched or sharply_curved:
                priority = radius_squared * (0.20 if sharply_curved else 0.35)
                candidates.append((priority, edge))
        desired = {edge for _, edge in sorted(candidates)[:max_active]}
        for edge, refinement in tuple(self.refinements.items()):
            if refinement.active and edge not in desired:
                self.deactivate_refinement(edge)
        for edge in desired:
            self.activate_refinement(edge)

    def pin(self, index: int, position: Vec2) -> None:
        if not 0 <= index < len(self.particles):
            raise IndexError(index)
        self.pinned[index] = position

    def move_pin(self, index: int, position: Vec2) -> None:
        if index in self.pinned:
            self.pinned[index] = position

    def approach_particle(
        self,
        index: int,
        target: Vec2,
        dt: float,
        response: float = 18.0,
        max_speed: float = 120.0,
    ) -> None:
        """Avvicina un punto a un bersaglio senza imporre una posizione rigida."""
        if not 0 <= index < len(self.particles):
            raise IndexError(index)
        particle = self.particles[index]
        movement = (target - particle.position) * min(1.0, response * dt)
        length = movement.length()
        maximum = max_speed * dt
        if length > maximum and length > 1e-9:
            movement = movement * (maximum / length)
        particle.position = particle.position + movement
        particle.previous = particle.previous + movement

    def set_soft_target(self, index: int, target: Vec2, strength: float = 1.6) -> None:
        if not 0 <= index < len(self.particles):
            raise IndexError(index)
        self.soft_targets[index] = SoftTarget(
            Vec2(target.x, target.y), min(3.0, max(0.0, strength))
        )

    def clear_soft_targets(self, indices: tuple[int, ...] | None = None) -> None:
        if indices is None:
            self.soft_targets.clear()
            return
        for index in indices:
            self.soft_targets.pop(index, None)

    def set_soft_target_strength(self, indices: tuple[int, ...], strength: float) -> None:
        bounded = min(3.0, max(0.0, strength))
        for index in indices:
            target = self.soft_targets.get(index)
            if target is not None:
                target.strength = bounded

    def translate_soft_targets(self, indices: tuple[int, ...], movement: Vec2) -> None:
        for index in indices:
            target = self.soft_targets.get(index)
            if target is not None:
                target.point = target.point + movement

    def translate_inner_structure(self, movement: Vec2) -> None:
        """Attua il nucleo interno; i vincoli radiali trascinano la membrana."""
        start = self.outer_count
        stop = self.base_particle_count
        for particle in self.particles[start:stop]:
            particle.position = particle.position + movement
            particle.previous = particle.previous + movement

    def unpin(self, index: int) -> None:
        self.pinned.pop(index, None)
        particle = self.particles[index]
        particle.previous = Vec2(particle.position.x, particle.position.y)

    def stop_motion(self) -> None:
        """Rimuove la velocità residua senza cambiare la forma corrente."""
        for particle in self.particles:
            particle.previous = Vec2(particle.position.x, particle.position.y)

    def request_area_growth(self, factor: float) -> None:
        if factor <= 0.0:
            raise ValueError("Il fattore di crescita deve essere positivo")
        self.area_growth_goal = self.target_area * factor

    def stabilize_after_internal_motion(self, duration: float = 0.9) -> None:
        """Assorbe energia elastica interna senza traslare l'intero corpo."""
        self.stabilized_center = self.center
        self.stabilization_time = max(0.0, duration)
        self.stop_motion()

    def cancel_stabilization(self) -> None:
        self.stabilized_center = None
        self.stabilization_time = 0.0

    def _update_area_growth(self, dt: float) -> None:
        if self.area_growth_goal is None:
            return
        difference = self.area_growth_goal - self.target_area
        maximum_change = max(self.target_area * 0.08 * dt, 1e-9)
        if abs(difference) <= maximum_change:
            self.target_area = self.area_growth_goal
            self.area_growth_goal = None
        else:
            self.target_area += maximum_change if difference > 0.0 else -maximum_change

    def _apply_center_stabilization(self, dt: float) -> None:
        if self.stabilized_center is None or self.stabilization_time <= 0.0:
            return
        correction = self.stabilized_center - self.center
        for index, particle in enumerate(self.particles):
            if not self.is_particle_active(index):
                continue
            particle.position = particle.position + correction
            particle.previous = Vec2(particle.position.x, particle.position.y)
        self.stabilization_time = max(0.0, self.stabilization_time - dt)
        if self.stabilization_time == 0.0:
            self.stabilized_center = None

    def select_pseudopod(self, index: int | None) -> None:
        """Seleziona il punto centrale della protrusione sulla membrana esterna."""
        if index is not None and not 0 <= index < self.outer_count:
            raise IndexError(index)
        self.pseudopod_index = index
        if index is None:
            self.pseudopod_target = 0.0

    def set_pseudopod_extending(self, extending: bool) -> None:
        """Richiede estensione o ritrazione; il cambiamento resta graduale."""
        self.pseudopod_target = 1.0 if extending and self.pseudopod_index is not None else 0.0

    def set_squeezing(self, squeezing: bool, direction: Vec2 | None = None) -> None:
        """Rende gradualmente cedevole la struttura interna nelle strettoie."""
        self.squeeze_target = 1.0 if squeezing else 0.0
        if direction is not None and direction.length() > 1e-9:
            self.squeeze_direction = direction / direction.length()

    def _update_squeeze(self, dt: float) -> None:
        difference = self.squeeze_target - self.squeeze_activation
        maximum_change = self.squeeze_response * dt
        if abs(difference) <= maximum_change:
            self.squeeze_activation = self.squeeze_target
        else:
            self.squeeze_activation += maximum_change if difference > 0.0 else -maximum_change

    def add_transient_protrusion(
        self,
        center: int,
        half_width: int,
        strength: float,
        lifetime: float,
    ) -> None:
        if not 0 <= center < self.outer_count:
            raise IndexError(center)
        self.transient_protrusions.append(
            TransientProtrusion(
                center,
                max(2, half_width),
                min(0.5, max(0.0, strength)),
                max(0.2, lifetime),
            )
        )

    def transient_activity_at(self, index: int) -> float:
        activity = 0.0
        for protrusion in self.transient_protrusions:
            direct = abs(index - protrusion.center)
            distance = min(direct, self.outer_count - direct)
            if distance > protrusion.half_width:
                continue
            phase = distance / (protrusion.half_width + 1)
            spatial_weight = 0.5 + 0.5 * cos(pi * phase)
            activity += protrusion.strength * protrusion.envelope * spatial_weight
        return min(0.65, activity)

    def _update_transient_protrusions(self, dt: float) -> None:
        for protrusion in self.transient_protrusions:
            protrusion.age += dt
        self.transient_protrusions = [
            protrusion
            for protrusion in self.transient_protrusions
            if protrusion.age < protrusion.lifetime
        ]

    def cortical_deformation_at(self, index: int) -> float:
        """Attività lenta che rompe la simmetria circolare del cortex."""
        angle = 2.0 * pi * index / self.outer_count
        wave = (
            0.105 * sin(2.0 * angle + 0.43 * self.cortex_time + 0.4)
            + 0.072 * sin(3.0 * angle - 0.31 * self.cortex_time + 1.7)
            + 0.038 * sin(5.0 * angle + 0.19 * self.cortex_time + 2.8)
        )
        if self.pseudopod_index is not None and self.pseudopod_activation > 0.0:
            rear = (self.pseudopod_index + self.outer_count // 2) % self.outer_count
            direct = abs(index - rear)
            distance = min(direct, self.outer_count - direct)
            rear_width = max(4, round(self.outer_count / 7))
            if distance <= rear_width:
                phase = distance / (rear_width + 1)
                weight = 0.5 + 0.5 * cos(pi * phase)
                asymmetry = 0.72 + 0.28 * sin(self.cortex_time * 1.3 + angle + 0.8)
                wave -= 0.21 * self.pseudopod_activation * weight * asymmetry
                side_lobe = sin(pi * phase) * sin(angle * 2.0 + self.cortex_time * 0.7)
                wave += 0.09 * self.pseudopod_activation * side_lobe
                signed = (index - rear + self.outer_count // 2) % self.outer_count
                signed -= self.outer_count // 2
                rear_skew = signed / rear_width
                wave += 0.15 * self.pseudopod_activation * weight * rear_skew
        return min(0.24, max(-0.27, wave))

    def cortical_edge_tension_at(self, index: int) -> float:
        angle = 2.0 * pi * index / self.outer_count
        tension = (
            0.050 * sin(3.0 * angle - 0.27 * self.cortex_time + 0.2)
            + 0.030 * sin(5.0 * angle + 0.41 * self.cortex_time + 2.1)
        )
        if self.pseudopod_index is not None and self.pseudopod_activation > 0.0:
            rear = (self.pseudopod_index + self.outer_count // 2) % self.outer_count
            direct = abs(index - rear)
            distance = min(direct, self.outer_count - direct)
            width = max(4, round(self.outer_count / 6))
            if distance <= width:
                weight = 0.5 + 0.5 * cos(pi * distance / (width + 1))
                tension -= 0.075 * self.pseudopod_activation * weight
        return min(0.09, max(-0.11, tension))

    def _relax_membrane_tension(self, dt: float) -> None:
        blend = min(1.0, 1.1 * dt)
        for constraint in self.constraints:
            if constraint.kind != "membrane" or constraint.equilibrium_length is None:
                continue
            constraint.rest_length += (
                constraint.equilibrium_length - constraint.rest_length
            ) * blend

    def pseudopod_weight(self, index: int) -> float:
        """Peso morbido 0..1 di un punto rispetto al centro dello pseudopodio."""
        if self.pseudopod_index is None or not 0 <= index < self.outer_count:
            return 0.0
        direct = abs(index - self.pseudopod_index)
        distance = min(direct, self.outer_count - direct)
        if distance > self.pseudopod_half_width:
            return 0.0
        phase = distance / (self.pseudopod_half_width + 1)
        return 0.5 + 0.5 * cos(pi * phase)

    def _update_pseudopod(self, dt: float) -> None:
        if self.pseudopod_activation < self.pseudopod_target:
            self.pseudopod_activation = min(
                self.pseudopod_target,
                self.pseudopod_activation + self.pseudopod_extend_speed * dt,
            )
        elif self.pseudopod_activation > self.pseudopod_target:
            self.pseudopod_activation = max(
                self.pseudopod_target,
                self.pseudopod_activation - self.pseudopod_retract_speed * dt,
            )

    def step(self, dt: float) -> None:
        if dt <= 0.0:
            raise ValueError("dt deve essere positivo")
        self._update_pseudopod(dt)
        self._update_squeeze(dt)
        self.cortex_time += dt
        self._update_transient_protrusions(dt)
        self._relax_membrane_tension(dt)
        self._update_area_growth(dt)

        for index, particle in enumerate(self.particles):
            if not self.is_particle_active(index):
                particle.previous = Vec2(particle.position.x, particle.position.y)
                continue
            if index in self.pinned:
                target = self.pinned[index]
                particle.position = Vec2(target.x, target.y)
                particle.previous = Vec2(target.x, target.y)
                continue
            velocity = (particle.position - particle.previous) * self.damping
            speed = velocity.length() / dt
            if speed > self.max_speed:
                velocity = velocity * (self.max_speed / speed)
            particle.previous = particle.position
            particle.position = particle.position + velocity

        self.area_lagrange = 0.0
        for constraint in self.constraints:
            constraint.lagrange = 0.0

        for _ in range(self.solver_iterations):
            self._solve_distances(dt)
            self._solve_area(dt)
            self._solve_pins()
            self._solve_soft_targets()
        self._apply_center_stabilization(dt)

    def _inverse_mass(self, index: int) -> float:
        if index in self.pinned or not self.is_particle_active(index):
            return 0.0
        return self.particles[index].inverse_mass

    def _solve_distances(self, dt: float) -> None:
        squeeze_center = self.center if self.squeeze_activation > 0.0 else Vec2()
        for constraint in self.constraints:
            if not constraint.active:
                continue
            first = self.particles[constraint.a]
            second = self.particles[constraint.b]
            delta = second.position - first.position
            length = delta.length()
            if length < 1e-9:
                continue
            first_mass = self._inverse_mass(constraint.a)
            second_mass = self._inverse_mass(constraint.b)
            compliance = constraint.compliance
            if constraint.kind in {
                "radial",
                "radial_secondary",
                "attachment",
                "attachment_secondary",
            }:
                compliance *= 1.0 + 4.0 * self.squeeze_activation
            elif constraint.kind == "bend":
                compliance *= 1.0 + 28.0 * self.squeeze_activation
            elif constraint.kind == "inner":
                compliance *= 1.0 + 10.0 * self.squeeze_activation
            elif constraint.kind in {"core", "core_radial", "module_bridge"}:
                compliance *= 1.0 + 18.0 * self.squeeze_activation
            elif constraint.kind == "lattice":
                compliance *= 1.0 + 14.0 * self.squeeze_activation
            alpha = compliance / (dt * dt)
            denominator = first_mass + second_mass + alpha
            if denominator <= 0.0:
                continue
            target_length = constraint.rest_length
            if constraint.kind == "membrane":
                target_length *= 1.0 + self.cortical_edge_tension_at(constraint.a)
            if constraint.kind in {
                "radial",
                "radial_secondary",
                "attachment",
                "attachment_secondary",
            }:
                activity = (
                    self.pseudopod_activation * self.pseudopod_weight(constraint.a)
                    + self.transient_activity_at(constraint.a)
                )
                target_length *= (
                    1.0
                    + self.pseudopod_extension * activity
                    + self.cortical_deformation_at(constraint.a)
                )
                radial = first.position - squeeze_center
                radial_length = radial.length()
                if radial_length > 1e-9 and self.squeeze_activation > 0.0:
                    axial = abs((radial / radial_length).dot(self.squeeze_direction))
                    squeezed_scale = 0.54 + 0.68 * axial * axial
                    target_length *= 1.0 + self.squeeze_activation * (
                        squeezed_scale - 1.0
                    )
            value = length - target_length
            change = (-value - alpha * constraint.lagrange) / denominator
            constraint.lagrange += change
            correction = delta * (change / length)
            first.position = first.position - correction * first_mass
            second.position = second.position + correction * second_mass

    def _solve_area(self, dt: float) -> None:
        points = self.outer_positions
        signed_area = self._signed_area(points)
        orientation = 1.0 if signed_area >= 0.0 else -1.0
        value = abs(signed_area) - self.target_area
        gradients: list[Vec2] = []
        denominator = 0.0

        for i in range(self.outer_count):
            previous = points[(i - 1) % self.outer_count]
            following = points[(i + 1) % self.outer_count]
            gradient = Vec2(
                (following.y - previous.y) * 0.5 * orientation,
                (previous.x - following.x) * 0.5 * orientation,
            )
            gradients.append(gradient)
            denominator += self._inverse_mass(i) * gradient.dot(gradient)

        effective_area_compliance = self.area_compliance * (
            1.0 + 12.0 * self.squeeze_activation
        )
        alpha = effective_area_compliance / (dt * dt)
        denominator += alpha
        if denominator <= 0.0:
            return
        change = (-value - alpha * self.area_lagrange) / denominator
        self.area_lagrange += change
        for i, gradient in enumerate(gradients):
            mass = self._inverse_mass(i)
            self.particles[i].position = self.particles[i].position + gradient * (change * mass)

    def _solve_pins(self) -> None:
        for index, target in self.pinned.items():
            particle = self.particles[index]
            particle.position = Vec2(target.x, target.y)

    def _solve_soft_targets(self) -> None:
        for index, target in self.soft_targets.items():
            particle = self.particles[index]
            correction = (target.point - particle.position) * (
                target.strength / self.solver_iterations
            )
            particle.position = particle.position + correction
            particle.previous = particle.previous + correction
