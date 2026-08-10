"""Banco di prova visuale del corpo PBF."""

from __future__ import annotations

import pygame

from .pbf import PBFCreature
from .physics import Vec2
from .world import Obstacle

WIDTH, HEIGHT = 960, 620
FIXED_DT = 1.0 / 60.0
TUNNEL_CENTER_Y = 310.0
TUNNEL_WIDTHS = (88.0, 64.0, 48.0, 36.0)
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


def create_creature(size_key: int) -> PBFCreature:
    _, radius = CREATURE_SIZES[size_key]
    return PBFCreature.create(radius=radius)


def main() -> None:
    pygame.init()
    screen = pygame.display.set_mode((WIDTH, HEIGHT))
    pygame.display.set_caption("Creatura — banco di prova PBF")
    clock = pygame.time.Clock()
    font = pygame.font.Font(None, 24)
    tunnel_width = TUNNEL_WIDTHS[1]
    size_key = pygame.K_m
    obstacles = tunnel_obstacles(tunnel_width)
    creature = create_creature(size_key)
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
                    creature = create_creature(size_key)
                elif pygame.K_1 <= event.key <= pygame.K_4:
                    tunnel_width = TUNNEL_WIDTHS[event.key - pygame.K_1]
                    obstacles = tunnel_obstacles(tunnel_width)
                    creature = create_creature(size_key)
                elif event.key in CREATURE_SIZES:
                    size_key = event.key
                    creature = create_creature(size_key)
            elif event.type == pygame.MOUSEBUTTONDOWN and event.button == 3:
                creature.set_target(Vec2(*event.pos))
        while accumulator >= FIXED_DT:
            creature.step(FIXED_DT, obstacles)
            accumulator -= FIXED_DT

        screen.fill((14, 18, 26))
        for obstacle in obstacles:
            rect = pygame.Rect(
                obstacle.left,
                obstacle.top,
                obstacle.right - obstacle.left,
                obstacle.bottom - obstacle.top,
            )
            pygame.draw.rect(screen, (49, 57, 70), rect)
            pygame.draw.rect(screen, (113, 132, 151), rect, 2)
        for point in creature.positions:
            pygame.draw.circle(screen, (100, 211, 196), (point.x, point.y), 4)
        if creature.target is not None:
            pygame.draw.circle(
                screen, (255, 204, 88), (creature.target.x, creature.target.y), 13, 2
            )
        pygame.draw.circle(screen, (244, 248, 250), (creature.center.x, creature.center.y), 3)
        diagnostics = creature.diagnostics
        size_name, nominal_radius = CREATURE_SIZES[size_key]
        lines = (
            (
                f"taglia: {size_name} ({nominal_radius:.0f}px)  "
                f"particelle: {creature.particle_count}  contatti: {diagnostics.contacts}  "
                f"connesse: {diagnostics.connected_particles}/{creature.particle_count}"
            ),
            (
                f"densita media: {diagnostics.average_density_ratio:.3f}  "
                f"errore massimo: {diagnostics.maximum_density_error:.3f}"
            ),
            (
                f"fase: {diagnostics.locomotion_phase}  "
                f"pseudopodi: {diagnostics.pseudopod_count}"
            ),
            (
                f"strettoia: {tunnel_width:.0f}px  "
                f"spazio fisico utile: {tunnel_width - 2 * creature.config.collision_margin:.0f}px"
            ),
            "P/M/G: taglia · 1/2/3/4: strettoia · click destro: bersaglio · R: reset",
            "Estensione · adesione · trazione del retro · rilascio progressivo",
        )
        for row, line in enumerate(lines):
            color = (230, 235, 242) if row < 2 else (158, 171, 188)
            screen.blit(font.render(line, True, color), (18, 16 + row * 25))
        pygame.display.flip()
    pygame.quit()


if __name__ == "__main__":
    main()
