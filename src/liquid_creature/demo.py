"""Visualizzazione diagnostica a punti della creatura semiliquida."""

from __future__ import annotations

from math import pi

import pygame

from .controller import ProceduralNavigator
from .phagocytosis import PhagocytosisController
from .physics import SoftBody, Vec2
from .world import Food, Obstacle, solve_food_collision, solve_obstacle_collisions

WIDTH, HEIGHT = 900, 620
FIXED_DT = 1.0 / 120.0


def screen_point(point: Vec2) -> tuple[float, float]:
    """Converte il vettore fisico nel formato accettato da Pygame."""
    return point.x, point.y


def nearest_particle(body: SoftBody, point: Vec2, maximum_distance: float = 18.0) -> int | None:
    candidates = [
        ((particle.position - point).length(), index)
        for index, particle in enumerate(body.particles)
        if body.is_particle_active(index)
    ]
    distance, index = min(candidates)
    return index if distance <= maximum_distance else None


def nearest_outer_particle(
    body: SoftBody, point: Vec2, maximum_distance: float = 24.0
) -> int | None:
    distances = [
        (particle.position - point).length()
        for particle in body.particles[: body.outer_count]
    ]
    index = min(range(len(distances)), key=distances.__getitem__)
    return index if distances[index] <= maximum_distance else None


def main() -> None:
    pygame.init()
    screen = pygame.display.set_mode((WIDTH, HEIGHT))
    pygame.display.set_caption("Creatura semiliquida — prototipo fisico")
    clock = pygame.time.Clock()
    font = pygame.font.Font(None, 24)
    lattice_mode = False
    body = SoftBody.create(center=Vec2(210.0, 310.0), radius=70.0)
    navigator = ProceduralNavigator()
    phagocytosis = PhagocytosisController()
    foods = [Food(Vec2(770.0, 310.0), radius=18.0)]
    obstacles = [
        Obstacle(390.0, 70.0, 650.0, 245.0),
        Obstacle(390.0, 375.0, 650.0, 550.0),
    ]
    obstacle_contact_focuses: list[Vec2] = []
    dragged: int | None = None
    dragged_food: Food | None = None
    food_contacts = 0
    accumulator = 0.0
    running = True

    while running:
        accumulator += min(clock.tick(60) / 1000.0, 0.05)
        for event in pygame.event.get():
            if event.type == pygame.QUIT:
                running = False
            elif event.type == pygame.KEYDOWN:
                if event.key == pygame.K_ESCAPE:
                    running = False
                elif event.key == pygame.K_r:
                    phagocytosis.cancel(body, navigator)
                    body = (
                        SoftBody.create_lattice(center=Vec2(210.0, 310.0), radius=70.0)
                        if lattice_mode
                        else SoftBody.create(center=Vec2(210.0, 310.0), radius=70.0)
                    )
                    navigator = ProceduralNavigator()
                    phagocytosis = PhagocytosisController()
                    foods = [Food(Vec2(770.0, 310.0), radius=18.0)]
                    obstacle_contact_focuses = []
                    dragged = None
                    dragged_food = None
                elif event.key == pygame.K_m:
                    phagocytosis.cancel(body, navigator)
                    lattice_mode = not lattice_mode
                    body = (
                        SoftBody.create_lattice(center=Vec2(210.0, 310.0), radius=70.0)
                        if lattice_mode
                        else SoftBody.create(center=Vec2(210.0, 310.0), radius=70.0)
                    )
                    navigator = ProceduralNavigator()
                    phagocytosis = PhagocytosisController()
                    foods = [Food(Vec2(770.0, 310.0), radius=18.0)]
                    obstacle_contact_focuses = []
                    dragged = None
                    dragged_food = None
                elif event.key == pygame.K_SPACE:
                    if not navigator.step_controller.active and not navigator.enabled:
                        body.set_pseudopod_extending(True)
                elif event.key == pygame.K_RETURN:
                    navigator.step_controller.start(body)
                elif event.key == pygame.K_s:
                    phagocytosis.automatic_enabled = False
                    phagocytosis.cancel(body, navigator)
                elif event.key == pygame.K_f:
                    phagocytosis.automatic_enabled = True
                    available_food = [food for food in foods if not food.consumed]
                    if available_food:
                        nearest_food = min(
                            available_food,
                            key=lambda food: (food.position - body.center).length(),
                        )
                        phagocytosis.start(body, nearest_food, navigator)
            elif event.type == pygame.KEYUP and event.key == pygame.K_SPACE:
                if not navigator.step_controller.active and not navigator.enabled:
                    body.set_pseudopod_extending(False)
            elif event.type == pygame.MOUSEBUTTONDOWN and event.button == 1:
                busy = navigator.step_controller.active or navigator.enabled
                point = Vec2(*event.pos)
                dragged_food = next(
                    (food for food in foods if not food.consumed and food.contains(point, 5.0)),
                    None,
                )
                dragged = (
                    None
                    if busy or dragged_food is not None
                    else nearest_particle(body, point)
                )
                if dragged_food is not None:
                    phagocytosis.automatic_enabled = False
                    phagocytosis.cancel(body, navigator)
                elif dragged is not None:
                    body.pin(dragged, Vec2(*event.pos))
            elif event.type == pygame.MOUSEMOTION:
                if dragged_food is not None:
                    dragged_food.position = Vec2(*event.pos)
                elif dragged is not None:
                    body.move_pin(dragged, Vec2(*event.pos))
            elif event.type == pygame.MOUSEBUTTONUP and event.button == 1:
                if dragged is not None:
                    body.unpin(dragged)
                dragged = None
                dragged_food = None
                phagocytosis.automatic_enabled = True
            elif event.type == pygame.MOUSEBUTTONDOWN and event.button == 3:
                point = Vec2(*event.pos)
                selected_food = next(
                    (food for food in foods if not food.consumed and food.contains(point, 8.0)),
                    None,
                )
                selected = nearest_outer_particle(body, point)
                if selected_food is not None:
                    phagocytosis.automatic_enabled = True
                    phagocytosis.start(body, selected_food, navigator)
                elif selected is not None and not navigator.enabled:
                    body.select_pseudopod(selected)
                else:
                    navigator.set_target(point)

        while accumulator >= FIXED_DT:
            target_beyond_passage = (
                navigator.enabled
                and navigator.target is not None
                and navigator.target.x > obstacles[0].right
            )
            near_or_inside_passage = 300.0 < body.center.x < obstacles[0].right + 80.0
            navigator.precision_mode = target_beyond_passage and near_or_inside_passage
            squeeze_direction = (
                navigator.target - body.center
                if navigator.target is not None
                else Vec2(1.0, 0.0)
            )
            body.set_squeezing(
                target_beyond_passage and near_or_inside_passage,
                squeeze_direction,
            )
            refinement_focuses = [food.position for food in foods if not food.consumed]
            refinement_focuses.extend(obstacle_contact_focuses)
            if body.pseudopod_index is not None and body.pseudopod_activation > 0.02:
                refinement_focuses.append(body.particles[body.pseudopod_index].position)
            refinement_focuses.extend(
                body.particles[protrusion.center].position
                for protrusion in body.transient_protrusions
            )
            body.update_adaptive_refinement(refinement_focuses)
            phagocytosis.sense_and_maybe_start(body, foods, navigator, FIXED_DT)
            navigator.update(body, FIXED_DT)
            phagocytosis.update(body, navigator, FIXED_DT)
            body.step(FIXED_DT)
            food_contacts = 0
            for _ in range(3):
                food_contacts += sum(solve_food_collision(body, food) for food in foods)
            obstacle_contact_focuses = []
            for _ in range(3):
                obstacle_contact_focuses.extend(
                    solve_obstacle_collisions(body, obstacles)
                )
            accumulator -= FIXED_DT

        screen.fill((17, 21, 29))
        for obstacle in obstacles:
            rect = pygame.Rect(
                obstacle.left,
                obstacle.top,
                obstacle.right - obstacle.left,
                obstacle.bottom - obstacle.top,
            )
            pygame.draw.rect(screen, (47, 55, 69), rect)
            pygame.draw.rect(screen, (103, 122, 142), rect, 3)
        for food in foods:
            if food.consumed:
                continue
            pygame.draw.circle(screen, (238, 129, 91), screen_point(food.position), food.radius)
            pygame.draw.circle(
                screen,
                (255, 205, 145),
                screen_point(food.position),
                food.radius,
                3,
            )
            pygame.draw.circle(screen, (116, 57, 50), screen_point(food.position), 4)
            if food.adhesion > 0.0:
                diameter = (food.radius + 7.0) * 2.0
                rect = pygame.Rect(0.0, 0.0, diameter, diameter)
                rect.center = screen_point(food.position)
                pygame.draw.arc(
                    screen,
                    (255, 235, 110),
                    rect,
                    -pi / 2,
                    -pi / 2 + 2 * pi * food.adhesion,
                    4,
                )
        for constraint in body.constraints:
            if not constraint.active:
                continue
            first = body.particles[constraint.a].position
            second = body.particles[constraint.b].position
            colors = {
                "membrane": (104, 220, 178),
                "bend": (45, 82, 78),
                "inner": (116, 132, 170),
                "core": (174, 132, 205),
                "core_radial": (91, 77, 125),
                "module_bridge": (132, 87, 145),
                "radial": (55, 68, 91),
                "radial_secondary": (46, 55, 75),
                "lattice": (67, 91, 124),
                "attachment": (60, 112, 120),
                "attachment_secondary": (48, 70, 88),
                "refined_membrane": (255, 178, 105),
            }
            pygame.draw.line(
                screen,
                colors[constraint.kind],
                screen_point(first),
                screen_point(second),
                1,
            )

        ring_start = body.outer_count
        core_start = ring_start + body.inner_count
        ring_per_module = (
            body.inner_count // body.module_count if body.module_count else 0
        )
        core_per_module = body.core_count // body.module_count if body.module_count else 0
        module_colors = (
            (115, 205, 255),
            (180, 135, 255),
            (255, 145, 190),
            (125, 230, 165),
        )
        for i, particle in enumerate(body.particles):
            if not body.is_particle_active(i):
                continue
            if i == dragged:
                color, radius = (255, 105, 105), 7
            elif i in navigator.step_controller.anchored_particles:
                color, radius = (255, 90, 105), 8
            elif i in phagocytosis.stabilizing_particles:
                color, radius = (105, 175, 255), 7
            elif i == body.pseudopod_index:
                color, radius = (255, 205, 90), 7
            elif i < body.outer_count and body.pseudopod_weight(i) > 0.0:
                intensity = body.pseudopod_weight(i) * body.pseudopod_activation
                color = (190 + int(65 * intensity), 255 - int(105 * intensity), 224)
                radius = 5
            elif i < body.outer_count and body.transient_activity_at(i) > 0.01:
                intensity = body.transient_activity_at(i)
                color = (180 + int(80 * intensity), 170, 255)
                radius = 5
            elif i < body.outer_count:
                color, radius = (190, 255, 224), 4
            elif body.is_refinement_particle(i):
                color, radius = (255, 190, 115), 3
            elif lattice_mode:
                color, radius = (105, 175, 220), 4
            elif i < core_start:
                module = (i - ring_start) // ring_per_module
                color, radius = module_colors[module], 4
            else:
                module = (i - core_start) // core_per_module
                color, radius = module_colors[module], 7
            pygame.draw.circle(screen, color, screen_point(particle.position), radius)

        for module in range(body.module_count):
            start = core_start + module * core_per_module
            points = body.particles[start : start + core_per_module]
            nucleus = Vec2(
                sum(point.position.x for point in points) / core_per_module,
                sum(point.position.y for point in points) / core_per_module,
            )
            color = module_colors[module]
            pygame.draw.circle(screen, (22, 25, 35), screen_point(nucleus), 10)
            pygame.draw.circle(screen, color, screen_point(nucleus), 9, 3)
            pygame.draw.circle(screen, color, screen_point(nucleus), 3)

        center = body.center
        if navigator.target is not None:
            target_color = (78, 205, 255) if navigator.enabled else (75, 105, 125)
            pygame.draw.line(
                screen,
                (48, 71, 86),
                screen_point(center),
                screen_point(navigator.target),
                1,
            )
            pygame.draw.circle(screen, target_color, screen_point(navigator.target), 16, 2)
            pygame.draw.circle(screen, target_color, screen_point(navigator.target), 4)
        pygame.draw.circle(screen, (255, 205, 90), screen_point(center), 5)
        area_percent = 100.0 * body.area / body.target_area
        prey_adhesion = max((food.adhesion for food in foods if not food.consumed), default=0.0)
        status = (
            f"modello: {'reticolo' if lattice_mode else 'moduli'}   "
            f"punti attivi: {body.active_particle_count}   area: {area_percent:5.1f}%   "
            f"pseudopodio: {body.pseudopod_activation * 100:4.0f}%   "
            f"compressione: {body.squeeze_activation * 100:3.0f}%   "
            f"fase: {navigator.step_controller.phase.value}   "
            f"passi: {navigator.completed_steps}   "
            f"ultimo: {navigator.step_controller.last_displacement:4.1f}px   "
            f"forma: {body.pseudopod_half_width}/{body.pseudopod_extension:.1f}   "
            f"fagocitosi: {phagocytosis.phase.value} {phagocytosis.progress * 100:3.0f}%   "
            f"sensore: {prey_adhesion * 100:3.0f}%   "
            f"parete: {len(obstacle_contact_focuses)}"
        )
        screen.blit(font.render(status, True, (230, 235, 242)), (18, 18))
        screen.blit(
            font.render(
                "M cambia modello · click destro oltre la strettoia · S stop · R reset",
                True,
                (155, 166, 181),
            ),
            (18, 44),
        )
        pygame.display.flip()

    pygame.quit()


if __name__ == "__main__":
    main()
