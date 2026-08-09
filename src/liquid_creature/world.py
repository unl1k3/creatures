"""Oggetti dell'ambiente e contatti con la creatura."""

from __future__ import annotations

from dataclasses import dataclass

from .physics import SoftBody, Vec2


@dataclass(slots=True)
class Food:
    position: Vec2
    radius: float = 18.0
    energy: float = 1.0
    consumed: bool = False
    internalized: bool = False
    adhesion: float = 0.0

    def contains(self, point: Vec2, margin: float = 0.0) -> bool:
        return (point - self.position).length() <= self.radius + margin


@dataclass(slots=True, frozen=True)
class Obstacle:
    """Rettangolo solido statico usato per pareti e strettoie."""

    left: float
    top: float
    right: float
    bottom: float

    def __post_init__(self) -> None:
        if self.right <= self.left or self.bottom <= self.top:
            raise ValueError("L'ostacolo deve avere area positiva")

    def contains(self, point: Vec2, margin: float = 0.0) -> bool:
        return (
            self.left - margin <= point.x <= self.right + margin
            and self.top - margin <= point.y <= self.bottom + margin
        )

    def contact_correction(self, point: Vec2, margin: float) -> tuple[Vec2, Vec2] | None:
        """Restituisce correzione e normale verso l'esterno del rettangolo."""
        if not self.contains(point, margin):
            return None
        candidates = (
            (point.x - (self.left - margin), Vec2(-1.0, 0.0)),
            ((self.right + margin) - point.x, Vec2(1.0, 0.0)),
            (point.y - (self.top - margin), Vec2(0.0, -1.0)),
            ((self.bottom + margin) - point.y, Vec2(0.0, 1.0)),
        )
        distance, normal = min(candidates, key=lambda candidate: candidate[0])
        return normal * max(0.0, distance), normal


def solve_food_collision(body: SoftBody, food: Food, margin: float = 2.0) -> int:
    """Impedisce ai punti della membrana di attraversare un cibo solido."""
    if food.consumed:
        return 0
    if food.internalized:
        return 0
    minimum_distance = food.radius + margin
    contacts = 0
    center_direction = body.center - food.position

    for index in body.collision_particle_indices:
        particle = body.particles[index]
        delta = particle.position - food.position
        distance = delta.length()
        if distance >= minimum_distance:
            continue
        if distance < 1e-9:
            normal = center_direction
            normal_length = normal.length()
            normal = normal / normal_length if normal_length > 1e-9 else Vec2(1.0, 0.0)
        else:
            normal = delta / distance
        correction = normal * (minimum_distance - distance)
        particle.position = particle.position + correction
        # Corregge anche la posizione precedente per non creare energia artificiale.
        particle.previous = particle.previous + correction
        contacts += 1
    return contacts


def _apply_obstacle_contact(
    body: SoftBody,
    index: int,
    correction: Vec2,
    normal: Vec2,
    friction: float,
) -> None:
    particle = body.particles[index]
    velocity = particle.position - particle.previous
    particle.position = particle.position + correction
    normal_speed = velocity.dot(normal)
    tangent = velocity - normal * normal_speed
    retained_velocity = normal * max(0.0, normal_speed) + tangent * (1.0 - friction)
    particle.previous = particle.position - retained_velocity
    if index in body.pinned:
        # Una punta aderente deve ancorarsi alla superficie corretta, non alla
        # posizione penetrata che il solver ripristinerebbe al frame seguente.
        body.move_pin(index, Vec2(particle.position.x, particle.position.y))


def solve_obstacle_collisions(
    body: SoftBody,
    obstacles: list[Obstacle],
    margin: float = 2.0,
    friction: float = 0.18,
) -> list[Vec2]:
    """Espelle la membrana dai solidi e restituisce i punti di contatto.

    Oltre ai nodi controlla il centro di ogni segmento fisico della membrana:
    questo riduce la possibilità che uno spigolo sottile passi tra due punti.
    """
    bounded_friction = min(1.0, max(0.0, friction))
    focuses: list[Vec2] = []
    for obstacle in obstacles:
        for index in body.collision_particle_indices:
            particle = body.particles[index]
            contact = obstacle.contact_correction(particle.position, margin)
            if contact is None:
                continue
            correction, normal = contact
            if correction.length() <= 1e-9:
                continue
            _apply_obstacle_contact(
                body, index, correction, normal, bounded_friction
            )
            focuses.append(Vec2(particle.position.x, particle.position.y))

        membrane_kinds = {"membrane", "refined_membrane"}
        for constraint in body.constraints:
            if not constraint.active or constraint.kind not in membrane_kinds:
                continue
            first = body.particles[constraint.a].position
            second = body.particles[constraint.b].position
            midpoint = (first + second) * 0.5
            contact = obstacle.contact_correction(midpoint, margin)
            if contact is None:
                continue
            correction, normal = contact
            if correction.length() <= 1e-9:
                continue
            half = correction * 0.5
            _apply_obstacle_contact(
                body, constraint.a, half, normal, bounded_friction
            )
            _apply_obstacle_contact(
                body, constraint.b, half, normal, bounded_friction
            )
            focuses.append(midpoint + correction)
    return focuses
