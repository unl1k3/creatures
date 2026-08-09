"""Controllore procedurale, senza addestramento, per un passo locomotorio."""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from random import Random

from .physics import SoftBody, Vec2


class StepPhase(str, Enum):
    IDLE = "pronto"
    EXTENDING = "estensione"
    PULLING = "trazione"
    RELEASING = "rilascio"


@dataclass(slots=True)
class PseudopodStepController:
    phase: StepPhase = StepPhase.IDLE
    anchored_particles: tuple[int, ...] = ()
    release_timer: float = 0.0
    start_center: Vec2 | None = None
    last_displacement: float = 0.0
    contact_half_width: int | None = None
    release_interval: float = 0.055

    @property
    def active(self) -> bool:
        return self.phase is not StepPhase.IDLE

    def start(self, body: SoftBody) -> bool:
        """Avvia un passo nella zona già selezionata."""
        if self.active or body.pseudopod_index is None or body.pinned:
            return False
        body.cancel_stabilization()
        self.start_center = body.center
        self.last_displacement = 0.0
        if self.contact_half_width is None:
            self.contact_half_width = max(2, round(body.outer_count / 24))
        self.phase = StepPhase.EXTENDING
        body.set_pseudopod_extending(True)
        return True

    def cancel(self, body: SoftBody) -> None:
        for particle in self.anchored_particles:
            body.unpin(particle)
        self.anchored_particles = ()
        body.set_pseudopod_extending(False)
        self.phase = StepPhase.IDLE
        self.release_timer = 0.0

    def update(self, body: SoftBody, dt: float) -> None:
        if self.phase is StepPhase.EXTENDING and body.pseudopod_activation >= 0.995:
            tip = body.pseudopod_index
            if tip is None:
                self.cancel(body)
                return
            half_width = self.contact_half_width or 2
            contact = tuple(
                (tip + offset) % body.outer_count
                for offset in range(-half_width, half_width + 1)
            )
            for particle_index in contact:
                point = body.particles[particle_index].position
                body.pin(particle_index, Vec2(point.x, point.y))
            self.anchored_particles = contact
            body.set_pseudopod_extending(False)
            self.phase = StepPhase.PULLING

        elif self.phase is StepPhase.PULLING and body.pseudopod_activation <= 0.005:
            self.release_timer = self.release_interval
            self.phase = StepPhase.RELEASING

        elif self.phase is StepPhase.RELEASING:
            self.release_timer -= dt
            if self.release_timer <= 0.0:
                anchored = list(self.anchored_particles)
                releasing = anchored[:1]
                if len(anchored) > 1:
                    releasing += anchored[-1:]
                for particle in releasing:
                    body.unpin(particle)
                self.anchored_particles = tuple(anchored[1:-1]) if len(anchored) > 1 else ()
                if self.anchored_particles:
                    self.release_timer = self.release_interval
                else:
                    body.stop_motion()
                    if self.start_center is not None:
                        self.last_displacement = (body.center - self.start_center).length()
                    self.phase = StepPhase.IDLE


@dataclass(slots=True)
class ProceduralNavigator:
    """Ripete passi discreti scegliendo ogni volta il fronte verso il bersaglio."""

    step_controller: PseudopodStepController = field(default_factory=PseudopodStepController)
    target: Vec2 | None = None
    enabled: bool = False
    arrival_radius: float = 24.0
    pause_between_steps: float = 0.18
    pause_timer: float = 0.0
    completed_steps: int = 0
    random_seed: int | None = None
    precision_mode: bool = False
    rng: Random = field(init=False, repr=False)

    def __post_init__(self) -> None:
        self.rng = Random(self.random_seed)

    def set_target(self, target: Vec2) -> None:
        self.target = Vec2(target.x, target.y)
        self.enabled = True
        self.pause_timer = 0.0

    def stop(self, body: SoftBody) -> None:
        self.enabled = False
        self.step_controller.cancel(body)

    def _select_front(self, body: SoftBody) -> bool:
        if self.target is None:
            return False
        delta = self.target - body.center
        distance = delta.length()
        if distance <= self.arrival_radius:
            return False
        direction = delta / distance
        center = body.center
        ideal_front = max(
            range(body.outer_count),
            key=lambda index: (body.particles[index].position - center).dot(direction),
        )
        # Piccole deviazioni rendono il percorso organico senza perdere il bersaglio.
        maximum_offset = max(2, round(body.outer_count / 24))
        offset = (
            0
            if self.precision_mode
            else round(self.rng.triangular(-maximum_offset, maximum_offset, 0))
        )
        front = (ideal_front + offset) % body.outer_count
        body.select_pseudopod(front)
        return True

    def _vary_next_step(self, body: SoftBody) -> None:
        """Varia morfologia e ritmo entro intervalli deliberatamente sicuri."""
        if self.precision_mode:
            body.pseudopod_half_width = max(3, round(body.outer_count / 22))
            body.pseudopod_extension = 2.75
            body.pseudopod_extend_speed = 1.45
            body.pseudopod_retract_speed = 0.92
            self.step_controller.contact_half_width = 2
            self.pause_between_steps = 0.10
            return
        base_width = max(4, round(body.outer_count / 10))
        body.pseudopod_half_width = max(3, base_width + self.rng.choice((-2, -1, 0, 0, 1, 2)))
        body.pseudopod_extension = self.rng.uniform(1.7, 2.3)
        body.pseudopod_extend_speed = self.rng.uniform(1.25, 1.9)
        body.pseudopod_retract_speed = self.rng.uniform(0.9, 1.35)
        base_contact = max(2, round(body.outer_count / 24))
        self.step_controller.contact_half_width = max(
            2, base_contact + self.rng.choice((-1, 0, 0, 1, 2))
        )
        self.pause_between_steps = self.rng.uniform(0.10, 0.32)
        if body.pseudopod_index is not None:
            count = self.rng.choice((1, 2, 2, 3))
            base_offset = max(6, round(body.outer_count / 14))
            for _ in range(count):
                side = self.rng.choice((-1, 1))
                offset = side * self.rng.randint(base_offset, base_offset * 2)
                center = (body.pseudopod_index + offset) % body.outer_count
                body.add_transient_protrusion(
                    center=center,
                    half_width=self.rng.randint(4, max(5, round(body.outer_count / 14))),
                    strength=self.rng.uniform(0.14, 0.32),
                    lifetime=self.rng.uniform(0.65, 1.35),
                )

    def update(self, body: SoftBody, dt: float) -> None:
        was_active = self.step_controller.active
        self.step_controller.update(body, dt)
        if was_active and not self.step_controller.active:
            self.completed_steps += 1
            self.pause_timer = self.pause_between_steps

        if not self.enabled or self.step_controller.active or self.target is None:
            return
        if (self.target - body.center).length() <= self.arrival_radius:
            self.enabled = False
            return
        self.pause_timer = max(0.0, self.pause_timer - dt)
        if self.pause_timer == 0.0 and self._select_front(body):
            self._vary_next_step(body)
            self.step_controller.start(body)
