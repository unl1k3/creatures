"""Sequenza procedurale di avvicinamento, avvolgimento e ingestione."""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
from math import atan2, cos, pi, sin, sqrt

from .controller import ProceduralNavigator
from .physics import SoftBody, Vec2
from .world import Food


class PhagocytosisPhase(str, Enum):
    IDLE = "inattiva"
    APPROACHING = "avvicinamento"
    WRAPPING = "avvolgimento"
    INTERNALIZING = "internalizzazione"
    DIGESTING = "assorbimento"
    RECOVERING = "rilassamento"


@dataclass(slots=True)
class PhagocytosisController:
    phase: PhagocytosisPhase = PhagocytosisPhase.IDLE
    food: Food | None = None
    progress: float = 0.0
    wrap_duration: float = 2.2
    digest_duration: float = 1.5
    controlled_particles: tuple[int, ...] = ()
    stabilizing_particles: tuple[int, ...] = ()
    tip_index: int | None = None
    near_angle: float = 0.0
    original_food_radius: float = 0.0
    consumed_count: int = 0
    previous_arrival_radius: float = 24.0
    wrap_start_polar: dict[int, tuple[float, float]] | None = None
    leading_side: int = 1
    internalization_speed: float = 32.0
    internalization_start_distance: float = 0.0
    internalization_target: Vec2 | None = None
    internalization_command_center: Vec2 | None = None
    internal_controlled_particles: tuple[int, ...] = ()
    internalization_gait_time: float = 0.0
    internalization_gait_duration: float = 1.2
    recovery_duration: float = 1.8
    carried_body_center: Vec2 | None = None
    automatic_enabled: bool = True
    sensing_range: float = 90.0
    adhesion_threshold: float = 0.62
    adhesion_decay: float = 0.32

    @property
    def active(self) -> bool:
        return self.phase is not PhagocytosisPhase.IDLE

    def sense_and_maybe_start(
        self,
        body: SoftBody,
        foods: list[Food],
        navigator: ProceduralNavigator,
        dt: float,
    ) -> None:
        """Accumula adesione sensoriale e avvia automaticamente la cattura."""
        if not self.automatic_enabled:
            return
        candidates: list[Food] = []
        for food in foods:
            if food.consumed or food.internalized:
                continue
            surface_gap = min(
                (body.particles[index].position - food.position).length() - food.radius
                for index in body.collision_particle_indices
            )
            signal = max(0.0, 1.0 - max(0.0, surface_gap) / self.sensing_range)
            if signal > 0.0:
                adhesion_rate = 0.18 + 0.90 * signal
                food.adhesion = min(1.0, food.adhesion + adhesion_rate * dt)
            else:
                food.adhesion = max(0.0, food.adhesion - self.adhesion_decay * dt)
            if food.adhesion >= self.adhesion_threshold:
                candidates.append(food)
        if not self.active and candidates:
            prey = max(candidates, key=lambda food: food.adhesion)
            if self.start(body, prey, navigator):
                prey.adhesion = 1.0

    def start(self, body: SoftBody, food: Food, navigator: ProceduralNavigator) -> bool:
        if self.active or food.consumed:
            return False
        # Un click sul cibo ha priorità sulla navigazione precedente. In
        # particolare libera subito un'eventuale punta ancora aderente.
        if navigator.enabled or navigator.step_controller.active:
            navigator.stop(body)
        if body.pinned:
            return False
        body.cancel_stabilization()
        body.stop_motion()
        delta = body.center - food.position
        distance = delta.length()
        if distance < 1e-9:
            return False
        body_radius = sqrt(body.target_area / pi)
        approach_distance = body_radius + food.radius + 7.0
        approach_point = food.position + delta * (approach_distance / distance)
        self.previous_arrival_radius = navigator.arrival_radius
        navigator.arrival_radius = 6.0
        navigator.set_target(approach_point)
        self.food = food
        self.original_food_radius = food.radius
        self.progress = 0.0
        self.phase = PhagocytosisPhase.APPROACHING
        return True

    def cancel(self, body: SoftBody, navigator: ProceduralNavigator) -> None:
        navigator.stop(body)
        if self.food is not None and not self.food.consumed:
            self.food.internalized = False
        navigator.arrival_radius = self.previous_arrival_radius
        body.clear_soft_targets(self.controlled_particles)
        body.clear_soft_targets(self.internal_controlled_particles)
        for index in self.controlled_particles:
            if index in body.pinned:
                body.unpin(index)
        for index in self.stabilizing_particles:
            if index in body.pinned:
                body.unpin(index)
        self.controlled_particles = ()
        self.stabilizing_particles = ()
        self.wrap_start_polar = None
        self.carried_body_center = None
        self.internalization_target = None
        self.internalization_command_center = None
        self.internal_controlled_particles = ()
        self.internalization_gait_time = 0.0
        body.select_pseudopod(None)
        self.tip_index = None
        self.food = None
        self.progress = 0.0
        self.phase = PhagocytosisPhase.IDLE

    def _begin_wrap(self, body: SoftBody, navigator: ProceduralNavigator) -> None:
        if self.food is None:
            return
        navigator.stop(body)
        navigator.arrival_radius = self.previous_arrival_radius
        body.stop_motion()
        self.tip_index = min(
            range(body.outer_count),
            key=lambda index: (body.particles[index].position - self.food.position).length(),
        )
        near = body.center - self.food.position
        self.near_angle = atan2(near.y, near.x)
        tip = self.tip_index
        # Una base ampia distribuisce la curvatura sul corpo; lembi troppo corti
        # si ripiegano l'uno sull'altro vicino al punto di contatto.
        arm_length = max(6, round(body.outer_count / 6))
        self.controlled_particles = tuple(
            (tip + offset) % body.outer_count for offset in range(-arm_length, arm_length + 1)
        )
        rear = (tip + body.outer_count // 2) % body.outer_count
        rear_half_width = 1
        self.stabilizing_particles = tuple(
            (rear + offset) % body.outer_count
            for offset in range(-rear_half_width, rear_half_width + 1)
        )
        # Una breve adesione posteriore impedisce ai lembi anteriori di far
        # orbitare l'intero corpo intorno al cibo durante la chiusura.
        for index in self.stabilizing_particles:
            point = body.particles[index].position
            body.pin(index, Vec2(point.x, point.y))
        self.wrap_start_polar = {}
        for index in self.controlled_particles:
            delta = body.particles[index].position - self.food.position
            absolute_angle = atan2(delta.y, delta.x)
            relative_angle = atan2(
                sin(absolute_angle - self.near_angle),
                cos(absolute_angle - self.near_angle),
            )
            self.wrap_start_polar[index] = (relative_angle, delta.length())
            point = body.particles[index].position
            body.set_soft_target(index, point)
        self.leading_side = 1 if tip % 2 == 0 else -1
        self.progress = 0.0
        self.phase = PhagocytosisPhase.WRAPPING

    def _move_membrane_around_food(self, body: SoftBody, dt: float) -> None:
        if self.food is None or self.tip_index is None:
            return
        if self.wrap_start_polar is None:
            return
        surface_radius = self.food.radius + 3.0
        tip = self.tip_index
        eased = self.progress * self.progress * (3.0 - 2.0 * self.progress)
        tip_start_angle, tip_start_radius = self.wrap_start_polar[tip]
        tip_final_angle = self.leading_side * (pi - 0.18)
        tip_relative_angle = tip_start_angle + (tip_final_angle - tip_start_angle) * eased
        tip_angle = self.near_angle + tip_relative_angle
        tip_radius = tip_start_radius + (surface_radius - tip_start_radius) * eased
        body.set_soft_target(
            tip,
            self.food.position + Vec2(cos(tip_angle), sin(tip_angle)) * tip_radius,
        )
        maximum_arc = pi - 0.30
        arm_length = (len(self.controlled_particles) - 1) // 2
        for side in (-1, 1):
            for rank in range(1, arm_length + 1):
                index = (tip + side * rank) % body.outer_count
                # La porzione centrale arriva sul lato opposto del cibo; i punti
                # verso la base percorrono archi via via minori. L'ordine lungo
                # la membrana resta così monotono e non può formare un "8".
                travel = 1.0 - rank / arm_length
                start_angle, start_radius = self.wrap_start_polar[index]
                # Il segno geometrico viene dalla posizione reale, non dal verso
                # dell'indice: sul lato sinistro di un oggetto i due versi sono
                # invertiti e usarli direttamente scambierebbe i lembi.
                branch_sign = 1 if start_angle >= 0.0 else -1
                final_relative_angle = branch_sign * maximum_arc * travel
                local_progress = eased * travel
                relative_angle = start_angle + (
                    final_relative_angle - start_angle
                ) * local_progress
                radius = start_radius + (surface_radius - start_radius) * local_progress
                angle = self.near_angle + relative_angle
                target = self.food.position + Vec2(cos(angle), sin(angle)) * radius
                body.set_soft_target(index, target)

    def _release_membrane(self, body: SoftBody, keep_soft_pocket: bool = False) -> None:
        had_attachments = bool(self.stabilizing_particles)
        if not keep_soft_pocket:
            body.clear_soft_targets(self.controlled_particles)
        for index in self.controlled_particles + self.stabilizing_particles:
            if index in body.pinned:
                body.unpin(index)
        if not keep_soft_pocket:
            self.controlled_particles = ()
        self.stabilizing_particles = ()
        self.wrap_start_polar = None
        if had_attachments:
            body.stop_motion()

    def update(self, body: SoftBody, navigator: ProceduralNavigator, dt: float) -> None:
        if self.phase is PhagocytosisPhase.APPROACHING:
            if self.food is None or self.food.consumed:
                self.cancel(body, navigator)
                return
            body_radius = sqrt(body.target_area / pi)
            contact_distance = body_radius + self.food.radius + 12.0
            reached_approach_point = not navigator.enabled and not navigator.step_controller.active
            if (
                (body.center - self.food.position).length() <= contact_distance
                or reached_approach_point
            ):
                self._begin_wrap(body, navigator)

        elif self.phase is PhagocytosisPhase.WRAPPING:
            self.progress = min(1.0, self.progress + dt / self.wrap_duration)
            self._move_membrane_around_food(body, dt)
            if self.progress >= 1.0 and self.food is not None and body.contains_point(
                self.food.position
            ):
                self.food.internalized = True
                self._release_membrane(body, keep_soft_pocket=True)
                self.internalization_target = Vec2(
                    self.food.position.x, self.food.position.y
                )
                self.internalization_command_center = Vec2(
                    body.center.x, body.center.y
                )
                # La tasca già chiusa resta aderente alla preda; tutti gli altri
                # punti avanzano insieme. In questo modo trasla la massa intera,
                # non soltanto il nucleo, e la forma scorre attorno alla preda.
                pocket = set(self.controlled_particles)
                self.internal_controlled_particles = tuple(
                    index
                    for index in range(body.base_particle_count)
                    if index not in pocket
                )
                for index in self.internal_controlled_particles:
                    point = body.particles[index].position
                    body.set_soft_target(index, point, strength=0.85)
                self.internalization_start_distance = (
                    self.internalization_target - body.center
                ).length()
                self.internalization_gait_time = 0.0
                body.select_pseudopod(self.tip_index)
                body.set_pseudopod_extending(True)
                self.progress = 0.0
                self.phase = PhagocytosisPhase.INTERNALIZING

        elif self.phase is PhagocytosisPhase.INTERNALIZING:
            if self.food is None:
                self.cancel(body, navigator)
                return
            target = self.internalization_target or self.food.position
            command_center = self.internalization_command_center or body.center
            command_delta = target - command_center
            distance = (target - body.center).length()
            # L'avanzamento avviene a impulsi: prima il lembo si estende e
            # aderisce, poi la contrazione trascina in avanti la massa. La
            # velocità media resta simile, ma scompare l'effetto di scivolata.
            self.internalization_gait_time += dt
            gait_phase = (
                self.internalization_gait_time % self.internalization_gait_duration
            ) / self.internalization_gait_duration
            if gait_phase < 0.38:
                body.set_pseudopod_extending(True)
                phase_progress = gait_phase / 0.38
                gait_speed = 0.18 + 0.22 * phase_progress
            else:
                body.set_pseudopod_extending(False)
                phase_progress = (gait_phase - 0.38) / 0.62
                gait_speed = 0.35 + 1.65 * sin(pi * phase_progress)
            maximum_step = self.internalization_speed * gait_speed * dt
            if command_delta.length() > 1e-9:
                movement = command_delta * min(
                    1.0, maximum_step / command_delta.length()
                )
                body.translate_soft_targets(self.internal_controlled_particles, movement)
                self.internalization_command_center = command_center + movement
            start_distance = max(self.internalization_start_distance, 1e-9)
            self.progress = min(1.0, 1.0 - distance / start_distance)
            remaining_command = (
                target - (self.internalization_command_center or body.center)
            ).length()
            command_progress = min(1.0, 1.0 - remaining_command / start_distance)
            body.set_soft_target_strength(
                self.internal_controlled_particles,
                0.85 + 2.15 * command_progress * command_progress,
            )
            pocket_strength = 0.12 + 1.48 * (1.0 - command_progress) ** 2
            body.set_soft_target_strength(self.controlled_particles, pocket_strength)
            arrival_distance = max(10.0, self.original_food_radius * 0.6)
            if distance <= arrival_distance:
                body.set_pseudopod_extending(False)
                body.select_pseudopod(None)
                for index in self.controlled_particles:
                    body.set_soft_target(
                        index, body.particles[index].position, strength=0.28
                    )
                for index in self.internal_controlled_particles:
                    body.set_soft_target(
                        index, body.particles[index].position, strength=0.22
                    )
                body.stop_motion()
                self.progress = 0.0
                self.phase = PhagocytosisPhase.DIGESTING

        elif self.phase is PhagocytosisPhase.DIGESTING:
            if self.food is None:
                self.cancel(body, navigator)
                return
            self.progress = min(1.0, self.progress + dt / self.digest_duration)
            body.set_soft_target_strength(self.controlled_particles, 0.28)
            body.set_soft_target_strength(self.internal_controlled_particles, 0.22)
            self.food.radius = self.original_food_radius * (1.0 - self.progress)
            if self.progress >= 1.0:
                self.food.consumed = True
                self.food.internalized = False
                self.food.adhesion = 0.0
                self.progress = 0.0
                self.phase = PhagocytosisPhase.RECOVERING

        elif self.phase is PhagocytosisPhase.RECOVERING:
            self.progress = min(1.0, self.progress + dt / self.recovery_duration)
            release_strength = 0.28 * (1.0 - self.progress) ** 2
            body.set_soft_target_strength(self.controlled_particles, release_strength)
            body.set_soft_target_strength(
                self.internal_controlled_particles, release_strength * 0.8
            )
            if self.progress >= 1.0:
                body.request_area_growth(1.04)
                self._release_membrane(body)
                body.clear_soft_targets(self.internal_controlled_particles)
                # Il rilascio dei bersagli non deve convertire la tensione
                # elastica accumulata in una spinta verso la vecchia preda.
                body.stop_motion()
                self.carried_body_center = None
                self.internalization_target = None
                self.internalization_command_center = None
                self.internal_controlled_particles = ()
                self.internalization_gait_time = 0.0
                self.tip_index = None
                self.food = None
                self.consumed_count += 1
                self.phase = PhagocytosisPhase.IDLE
