"""Demo del modello a nuvola con contorno ricostruito."""

from __future__ import annotations

import pygame

from .cloud import PointCloudCreature
from .physics import Vec2
from .world import Obstacle

WIDTH, HEIGHT = 900, 620
FIXED_DT = 1.0 / 120.0


def screen_point(point: Vec2) -> tuple[float, float]:
    return point.x, point.y


def main() -> None:
    pygame.init()
    screen = pygame.display.set_mode((WIDTH, HEIGHT))
    pygame.display.set_caption("Creatura — nuvola dinamica")
    clock = pygame.time.Clock()
    font = pygame.font.Font(None, 25)
    creature = PointCloudCreature.create()
    obstacles = [
        Obstacle(390.0, 70.0, 650.0, 245.0),
        Obstacle(390.0, 375.0, 650.0, 550.0),
    ]
    accumulator = 0.0
    contacts = 0
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
                    creature = PointCloudCreature.create()
            elif event.type == pygame.MOUSEBUTTONDOWN and event.button == 3:
                creature.set_target(Vec2(*event.pos))

        while accumulator >= FIXED_DT:
            contacts = creature.update(FIXED_DT, obstacles)
            accumulator -= FIXED_DT

        screen.fill((16, 20, 28))
        for obstacle in obstacles:
            rect = pygame.Rect(
                obstacle.left,
                obstacle.top,
                obstacle.right - obstacle.left,
                obstacle.bottom - obstacle.top,
            )
            pygame.draw.rect(screen, (47, 55, 69), rect)
            pygame.draw.rect(screen, (103, 122, 142), rect, 3)

        if len(creature.outline) >= 3:
            pygame.draw.polygon(
                screen,
                (50, 112, 108),
                [screen_point(point) for point in creature.outline],
            )
            pygame.draw.lines(
                screen,
                (154, 255, 218),
                True,
                [screen_point(point) for point in creature.outline],
                3,
            )
        for point in creature.points:
            pygame.draw.circle(screen, (126, 186, 224), screen_point(point), 3)

        if creature.target is not None:
            pygame.draw.circle(
                screen, (90, 210, 255), screen_point(creature.target), 15, 2
            )
            pygame.draw.circle(screen, (90, 210, 255), screen_point(creature.target), 3)
        pygame.draw.circle(screen, (255, 210, 92), screen_point(creature.center), 5)
        status = (
            f"punti: {len(creature.points)}   contorno: {len(creature.outline)}   "
            f"pseudopodi: {creature.pseudopod_count}   contatti: {contacts}   "
            f"movimento: {'attivo' if creature.moving else 'fermo'}"
        )
        screen.blit(font.render(status, True, (230, 235, 242)), (18, 18))
        screen.blit(
            font.render(
                "click destro: bersaglio · R: reset · Esc: esci",
                True,
                (155, 166, 181),
            ),
            (18, 46),
        )
        pygame.display.flip()

    pygame.quit()


if __name__ == "__main__":
    main()
