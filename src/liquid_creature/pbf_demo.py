"""Banco di prova visuale del corpo PBF."""

from __future__ import annotations

from math import cos, pi, sin

import pygame

from .pbf import PBFConfig, PBFCreature
from .physics import Vec2
from .rendering import creature_contour, stabilize_contour
from .world import Obstacle

WIDTH, HEIGHT = 960, 620
FIXED_DT = 1.0 / 60.0
TUNNEL_CENTER_Y = 310.0
TUNNEL_WIDTHS = (88.0, 64.0, 48.0, 36.0)
ROOM_RIGHT = 820.0
ROOM_START = Vec2(105.0, 310.0)
ROOM_EXIT = pygame.Rect(738, 270, 48, 80)
ROOM_ACID = pygame.Rect(490, 420, 92, 130)
ROOM_NUTRIENTS = (Vec2(390.0, 155.0), Vec2(535.0, 475.0), Vec2(680.0, 310.0))
LAB_GAPS = ((92, 162, "facile"), (288, 332, "deformabile"), (501, 507, "impossibile"))
CREATURE_SIZES = {
    pygame.K_p: ("piccola", 35.0),
    pygame.K_m: ("media", 57.0),
    pygame.K_g: ("grande", 78.0),
}


def tunnel_obstacles(width: float) -> list[Obstacle]:
    half_width = width * 0.5
    return [
        Obstacle(390.0, 50.0, 680.0, TUNNEL_CENTER_Y - half_width),
        Obstacle(390.0, TUNNEL_CENTER_Y + half_width, 680.0, 570.0),
    ]


def gameplay_room_obstacles() -> list[Obstacle]:
    """Prima stanza: due varchi, una deviazione e una camera finale."""
    return [
        Obstacle(0.0, 0.0, ROOM_RIGHT, 24.0),
        Obstacle(0.0, HEIGHT - 24.0, ROOM_RIGHT, HEIGHT),
        Obstacle(0.0, 0.0, 24.0, HEIGHT),
        Obstacle(ROOM_RIGHT - 24.0, 0.0, ROOM_RIGHT, HEIGHT),
        Obstacle(250.0, 24.0, 286.0, 260.0),
        Obstacle(250.0, 360.0, 286.0, HEIGHT - 24.0),
        Obstacle(380.0, 225.0, 475.0, 395.0),
        Obstacle(600.0, 24.0, 634.0, 278.0),
        Obstacle(600.0, 342.0, 634.0, HEIGHT - 24.0),
    ]


def deformation_room_obstacles() -> list[Obstacle]:
    """Laboratorio con tre aperture di ampiezza progressivamente minore."""
    return [
        Obstacle(0.0, 0.0, ROOM_RIGHT, 24.0),
        Obstacle(0.0, HEIGHT - 24.0, ROOM_RIGHT, HEIGHT),
        Obstacle(0.0, 0.0, 24.0, HEIGHT),
        Obstacle(ROOM_RIGHT - 24.0, 0.0, ROOM_RIGHT, HEIGHT),
        Obstacle(360.0, 24.0, 610.0, 92.0),
        Obstacle(360.0, 162.0, 610.0, 288.0),
        Obstacle(360.0, 332.0, 610.0, 501.0),
        Obstacle(360.0, 507.0, 610.0, HEIGHT - 24.0),
    ]


def create_creature(
    size_key: int,
    high_detail: bool = False,
    center: Vec2 | None = None,
) -> PBFCreature:
    _, radius = CREATURE_SIZES[size_key]
    config = None
    if high_detail:
        config = PBFConfig(
            particle_spacing=5.0,
            smoothing_radius=10.0,
        )
    return PBFCreature.create(center=center, radius=radius, config=config)


def clip_contour_to_obstacles(
    contour: list[Vec2], obstacles: list[Obstacle]
) -> list[list[tuple[int, int]]]:
    """Sottrae i solidi e riconverte la parte visibile in poligoni."""
    if len(contour) < 3:
        return []
    origin_x = int(min(point.x for point in contour)) - 2
    origin_y = int(min(point.y for point in contour)) - 2
    width = int(max(point.x for point in contour)) - origin_x + 4
    height = int(max(point.y for point in contour)) - origin_y + 4
    canvas = pygame.Surface((width, height), pygame.SRCALPHA)
    local_polygon = [(round(point.x - origin_x), round(point.y - origin_y)) for point in contour]
    pygame.draw.polygon(canvas, (255, 255, 255, 255), local_polygon)
    mask = pygame.mask.from_surface(canvas)
    bounds = pygame.Rect(origin_x, origin_y, width, height)
    for obstacle in obstacles:
        solid = pygame.Rect(
            round(obstacle.left),
            round(obstacle.top),
            round(obstacle.right - obstacle.left),
            round(obstacle.bottom - obstacle.top),
        ).clip(bounds)
        if solid.width <= 0 or solid.height <= 0:
            continue
        blocker = pygame.Mask((solid.width, solid.height), fill=True)
        mask.erase(blocker, (solid.x - origin_x, solid.y - origin_y))
    polygons: list[list[tuple[int, int]]] = []
    for component in mask.connected_components(4):
        outline = component.outline(every=2)
        if len(outline) >= 3:
            polygons.append([(x + origin_x, y + origin_y) for x, y in outline])
    return polygons


def main() -> None:
    pygame.init()
    screen = pygame.display.set_mode((WIDTH, HEIGHT))
    pygame.display.set_caption("Creatura — banco di prova PBF")
    clock = pygame.time.Clock()
    font = pygame.font.Font(None, 24)
    tunnel_width = TUNNEL_WIDTHS[1]
    size_key = pygame.K_m
    high_detail = False
    room_mode = True
    deformation_mode = False
    obstacles = gameplay_room_obstacles()
    creature = create_creature(size_key, center=ROOM_START)
    nutrients = list(ROOM_NUTRIENTS)
    boost_energy = 1.0
    room_complete = False
    accumulator = 0.0
    render_mode = 2
    displayed_contour: list[Vec2] = []
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
                    start = ROOM_START if room_mode else None
                    creature = create_creature(size_key, high_detail, start)
                    if deformation_mode:
                        obstacles = deformation_room_obstacles()
                        nutrients = [Vec2(690.0, 310.0)]
                    else:
                        nutrients = list(ROOM_NUTRIENTS) if room_mode else []
                    room_complete = False
                    boost_energy = 1.0
                    displayed_contour = []
                elif event.key == pygame.K_l:
                    room_mode = True
                    deformation_mode = False
                    obstacles = gameplay_room_obstacles()
                    creature = create_creature(size_key, high_detail, ROOM_START)
                    nutrients = list(ROOM_NUTRIENTS)
                    room_complete = False
                    boost_energy = 1.0
                    displayed_contour = []
                elif event.key == pygame.K_c:
                    room_mode = True
                    deformation_mode = True
                    obstacles = deformation_room_obstacles()
                    creature = create_creature(pygame.K_m, high_detail, ROOM_START)
                    size_key = pygame.K_m
                    nutrients = [Vec2(690.0, 310.0)]
                    room_complete = False
                    boost_energy = 1.0
                    displayed_contour = []
                elif event.key == pygame.K_h:
                    size_key = pygame.K_p
                    high_detail = True
                    start = ROOM_START if room_mode else None
                    creature = create_creature(size_key, high_detail, start)
                    boost_energy = 1.0
                    displayed_contour = []
                elif event.key == pygame.K_n:
                    high_detail = False
                    start = ROOM_START if room_mode else None
                    creature = create_creature(size_key, high_detail, start)
                    boost_energy = 1.0
                    displayed_contour = []
                elif event.key in {pygame.K_F1, pygame.K_F2, pygame.K_F3}:
                    render_mode = event.key - pygame.K_F1 + 1
                elif pygame.K_1 <= event.key <= pygame.K_4:
                    room_mode = False
                    deformation_mode = False
                    tunnel_width = TUNNEL_WIDTHS[event.key - pygame.K_1]
                    obstacles = tunnel_obstacles(tunnel_width)
                    creature = create_creature(size_key, high_detail)
                    nutrients = []
                    room_complete = False
                    boost_energy = 1.0
                    displayed_contour = []
                elif event.key in CREATURE_SIZES:
                    size_key = event.key
                    high_detail = False
                    start = ROOM_START if room_mode else None
                    creature = create_creature(size_key, high_detail, start)
                    boost_energy = 1.0
                    displayed_contour = []
            elif event.type == pygame.MOUSEBUTTONDOWN and event.button == 3:
                creature.set_target(Vec2(*event.pos))
        if pygame.mouse.get_pressed()[2]:
            creature.set_target(Vec2(*pygame.mouse.get_pos()))
        while accumulator >= FIXED_DT:
            keys = pygame.key.get_pressed()
            boosting = (
                room_mode
                and keys[pygame.K_SPACE]
                and boost_energy > 0.0
                and not room_complete
            )
            creature.config.locomotion_speed_multiplier = (
                2.15 if boosting else 1.45 if room_mode else 1.0
            )
            if boosting:
                boost_energy = max(0.0, boost_energy - 0.48 * FIXED_DT)
            elif room_mode:
                boost_energy = min(1.0, boost_energy + 0.07 * FIXED_DT)
            creature.step(FIXED_DT, obstacles)
            if deformation_mode:
                manual_extension = pygame.mouse.get_pressed()[0]
                if manual_extension:
                    creature.set_target(None)
                    creature.extend_pseudopod_towards(
                        Vec2(*pygame.mouse.get_pos()), FIXED_DT
                    )
                else:
                    creature.release_manual_pseudopod()
            if room_mode and not deformation_mode and any(
                ROOM_ACID.collidepoint(point.x, point.y)
                for point in creature.positions
            ):
                boost_energy = max(0.0, boost_energy - 0.32 * FIXED_DT)
            accumulator -= FIXED_DT

        if room_mode:
            previous_nutrient_count = len(nutrients)
            nutrients = [
                nutrient
                for nutrient in nutrients
                if all((point - nutrient).length() > 13.0 for point in creature.positions)
            ]
            collected = previous_nutrient_count - len(nutrients)
            if collected:
                boost_energy = min(1.0, boost_energy + 0.34 * collected)
            if not nutrients and ROOM_EXIT.collidepoint(creature.center.x, creature.center.y):
                room_complete = True

        screen.fill((14, 18, 26))
        if deformation_mode and pygame.mouse.get_pressed()[0]:
            reach = round(
                creature.reference_radius
                * (1.0 + creature.config.manual_pseudopod_reach_ratio)
            )
            pygame.draw.circle(
                screen,
                (70, 94, 111),
                (round(creature.center.x), round(creature.center.y)),
                reach,
                1,
            )
        if room_mode:
            if deformation_mode:
                for top, bottom, label in LAB_GAPS:
                    gap_text = font.render(
                        f"{label}: {bottom - top}px", True, (176, 196, 211)
                    )
                    screen.blit(gap_text, (625, (top + bottom) // 2 - 9))
            else:
                acid_surface = pygame.Surface(ROOM_ACID.size, pygame.SRCALPHA)
                acid_surface.fill((193, 67, 104, 105))
                screen.blit(acid_surface, ROOM_ACID)
                pygame.draw.rect(screen, (245, 101, 139), ROOM_ACID, 2)
            exit_color = (68, 190, 123) if not nutrients else (73, 86, 103)
            pygame.draw.rect(screen, exit_color, ROOM_EXIT, border_radius=8)
            pygame.draw.rect(screen, (173, 234, 196), ROOM_EXIT, 2, border_radius=8)
            for nutrient in nutrients:
                pygame.draw.circle(screen, (255, 197, 78), (nutrient.x, nutrient.y), 9)
                pygame.draw.circle(screen, (255, 235, 164), (nutrient.x, nutrient.y), 9, 2)
        obstacle_rectangles: list[pygame.Rect] = []
        for obstacle in obstacles:
            rect = pygame.Rect(
                obstacle.left,
                obstacle.top,
                obstacle.right - obstacle.left,
                obstacle.bottom - obstacle.top,
            )
            obstacle_rectangles.append(rect)
            pygame.draw.rect(screen, (49, 57, 70), rect)
        if render_mode in {2, 3}:
            render_direction = None
            if creature.target is not None:
                target_delta = creature.target - creature.center
                if target_delta.length() > 1e-9:
                    render_direction = target_delta / target_delta.length()
            raw_contour = creature_contour(
                creature.positions,
                direction=render_direction,
                anchor_points=[
                    point
                    for point, adhesion in zip(
                        creature.positions,
                        creature.adhesion_weights,
                        strict=True,
                    )
                    if adhesion > (0.28 if creature.particle_count < 70 else 0.48)
                ],
            )
            displayed_contour = stabilize_contour(
                displayed_contour,
                raw_contour,
            )
            if len(displayed_contour) >= 3:
                for polygon in clip_contour_to_obstacles(displayed_contour, obstacles):
                    pygame.draw.polygon(screen, (56, 157, 146), polygon)
                    pygame.draw.aalines(screen, (255, 72, 72), True, polygon)
        if render_mode in {1, 3}:
            for point in creature.positions:
                pygame.draw.circle(screen, (100, 211, 196), (point.x, point.y), 4)
        nucleus_major, nucleus_minor = creature.nucleus_axes
        nucleus_axis = creature.nucleus_axis
        nucleus_perpendicular = Vec2(-nucleus_axis.y, nucleus_axis.x)
        nucleus_polygon = []
        for index in range(28):
            angle = 2.0 * pi * index / 28
            point = (
                creature.center
                + nucleus_axis * (cos(angle) * nucleus_major)
                + nucleus_perpendicular * (sin(angle) * nucleus_minor)
            )
            nucleus_polygon.append((point.x, point.y))
        pygame.draw.polygon(screen, (114, 73, 157), nucleus_polygon)
        pygame.draw.aalines(screen, (211, 170, 242), True, nucleus_polygon)
        for rect in obstacle_rectangles:
            pygame.draw.rect(screen, (113, 132, 151), rect, 2)
        if creature.target is not None:
            pygame.draw.circle(
                screen, (255, 204, 88), (creature.target.x, creature.target.y), 13, 2
            )
        pygame.draw.circle(screen, (244, 248, 250), (creature.center.x, creature.center.y), 3)
        diagnostics = creature.diagnostics
        size_name, nominal_radius = CREATURE_SIZES[size_key]
        resolution_name = "alta" if high_detail else "normale"
        if room_mode:
            energy_back = pygame.Rect(620, 18, 170, 16)
            pygame.draw.rect(screen, (42, 48, 59), energy_back, border_radius=4)
            energy_fill = pygame.Rect(
                energy_back.x,
                energy_back.y,
                round(energy_back.width * boost_energy),
                energy_back.height,
            )
            pygame.draw.rect(screen, (255, 185, 72), energy_fill, border_radius=4)
            pygame.draw.rect(screen, (235, 224, 198), energy_back, 2, border_radius=4)
        lines = (
            (
                f"taglia: {size_name} ({nominal_radius:.0f}px)  "
                f"risoluzione: {resolution_name}  "
                f"particelle: {creature.particle_count}  contatti: {diagnostics.contacts}  "
                f"connesse: {diagnostics.connected_particles}/{creature.particle_count}"
            ),
            (
                f"densita media: {diagnostics.average_density_ratio:.3f}  "
                f"errore massimo: {diagnostics.maximum_density_error:.3f}  "
                f"perimetro: {diagnostics.perimeter_ratio:.2f}/"
                f"{creature.config.maximum_perimeter_ratio:.2f}  "
                f"nucleo: {diagnostics.nucleus_aspect:.2f}:1"
            ),
            (f"fase: {diagnostics.locomotion_phase}  pseudopodi: {diagnostics.pseudopod_count}"),
            (
                f"estensione locale: {creature.manual_pseudopod_extension:.1f}/"
                f"{creature.reference_radius * creature.config.manual_pseudopod_reach_ratio:.1f}px  "
                f"massa: {creature.manual_pseudopod_count}/{creature.particle_count}"
                if deformation_mode
                else ""
            ),
            (
                f"stanza: nutrienti rimasti {len(nutrients)}  "
                f"uscita: {'raggiunta' if room_complete else 'attiva' if room_mode and not nutrients else 'bloccata'}"
                if room_mode
                else f"strettoia: {tunnel_width:.0f}px  "
                f"spazio fisico utile: {tunnel_width - 2 * creature.config.collision_margin:.0f}px"
            ),
            "destro: guida · sinistro: pseudopodio locale · C: sfida deformazione",
            "L: stanza · 1/2/3/4: strettoia",
            "Estensione · adesione · trazione del retro · rilascio progressivo",
            "F1: punti · F2: membrana · F3: entrambi",
        )
        if room_complete:
            message = font.render("STANZA COMPLETATA", True, (184, 255, 207))
            padding_x = 22
            padding_y = 12
            banner = pygame.Rect(
                WIDTH // 2 - message.get_width() // 2 - padding_x,
                116,
                message.get_width() + padding_x * 2,
                message.get_height() + padding_y * 2,
            )
            banner_surface = pygame.Surface(banner.size, pygame.SRCALPHA)
            banner_surface.fill((18, 66, 48, 225))
            screen.blit(banner_surface, banner)
            pygame.draw.rect(screen, (147, 255, 186), banner, 2, border_radius=10)
            screen.blit(
                message,
                (
                    banner.centerx - message.get_width() // 2,
                    banner.centery - message.get_height() // 2,
                ),
            )
        for row, line in enumerate(lines):
            color = (230, 235, 242) if row < 2 else (158, 171, 188)
            screen.blit(font.render(line, True, color), (18, 16 + row * 25))
        pygame.display.flip()
    pygame.quit()


if __name__ == "__main__":
    main()
