# Semi-Liquid Creature

An experimental 2D creature that moves and deforms procedurally. The project
explores amoeboid locomotion, pseudopods, narrow passages, dynamic membranes,
phagocytosis, and gameplay based on the physical shape of the creature.

The repository contains several prototypes. The current main direction is the
Position Based Fluids (PBF) model. It represents the body as a set of particles
without permanent springs or particle-to-particle anchors.

## Requirements

- Python 3.11 or newer
- [uv](https://docs.astral.sh/uv/) or a regular Python virtual environment
- Pygame 2.5 or newer

## Installation

With `uv`:

```bash
uv sync --extra dev
```

Alternatively, with Python:

```bash
python -m venv .venv
.venv/bin/pip install -e '.[dev]'
```

## Current PBF prototype

Run the current simulation with:

```bash
uv run creatura-pbf-demo
```

The creature uses three complementary physical layers:

1. **Position Based Fluids** preserve particle mass and resist local
   compression.
2. **A finite virtual membrane** monitors the outer perimeter and applies a
   distributed recovery force when it stretches beyond its allowed ratio. It
   does not use permanent springs.
3. **A deformable nucleus** preserves its area, can become elliptical within a
   limited aspect ratio, and has independent obstacle collisions. It defines a
   minimum passage size proportional to the creature instead of the particle
   resolution.

The purple ellipse drawn inside the creature is the nucleus. The red outline is
the reconstructed visual membrane; physical obstacle contacts are still solved
locally from particles and nearby membrane segments.

### Controls

- Hold the **right mouse button** to guide the whole creature.
- Hold the **left mouse button** to extend a local pseudopod toward the cursor.
- Hold `Space` for a temporary speed boost. Boosting consumes energy.
- Press `L` to restart the gameplay room.
- Press `C` to open the deformation laboratory.
- Press `1`–`4` to test progressively narrower tunnels.
- Press `P`, `M`, or `G` to select a small, medium, or large creature.
- Press `H` for the high-resolution small creature.
- Press `N` to return to normal particle resolution.
- Press `F1`, `F2`, or `F3` to show particles, membrane, or both.
- Press `R` to reset the current scene.
- Press `Esc` to exit.

### Local pseudopod limits

Manual pseudopods are deliberately bounded:

- their maximum reach scales with creature size;
- only a limited fraction of the total particle mass participates;
- acceleration fades as the tip approaches the reach limit;
- holding the button cannot extend the protrusion indefinitely;
- shape recovery retracts the protrusion after release.

In the deformation laboratory, a guide circle shows the maximum local reach.
The diagnostic overlay reports current extension, involved particle mass,
perimeter ratio, and nucleus aspect ratio.

The laboratory contains three openings:

- `70 px`: easy;
- `44 px`: requires substantial deformation;
- `6 px`: physically impossible.

The last opening is below both the particle collision scale and the deformable
nucleus limit. Increasing particle resolution therefore does not make it
passable.

## Gameplay room

The default PBF scene is a compact gameplay experiment:

- collect the three nutrients;
- avoid the pink acidic area, which drains boost energy;
- use nutrients to recover energy;
- reach the green exit after collecting everything.

This room is an early movement test. The deformation laboratory is used to
develop mechanics that cannot be reproduced by a rigid square or circle.

## Creature sizes and resolution

Small, medium, and large creatures share the same default particle spacing but
contain different particle counts. Their locomotion, pseudopod reach, nucleus,
and minimum passage size scale with their physical radius.

The high-resolution mode uses more particles to improve sampling while keeping
the same visible creature size. Gameplay limits are based on creature-scale
properties, so changing resolution should improve precision rather than change
which passages are possible.

## Earlier prototypes

The repository retains earlier approaches for comparison.

### Spring-based soft body

```bash
uv run creatura-demo
```

This prototype uses an explicit membrane, deformable internal modules, soft
constraints, adaptive temporary membrane nodes, procedural pseudopods, obstacle
contacts, and an experimental phagocytosis sequence. It remains useful as a
reference for ingestion and detailed membrane behavior.

### Dynamic particle cloud

```bash
uv run creatura-cloud-demo
```

This version removes the physical membrane and permanent anchors. A variable
point cloud generates a continuous metaball density field, and marching squares
extracts a vector contour. It was useful for exploring topology and rendering,
but produced less convincing locomotion and narrow-passage behavior than the
current PBF direction.

## Tests

Run the complete test suite with:

```bash
uv run pytest
```

The tests cover particle stability, mass conservation, locomotion phases,
collision behavior, connectivity, shape recovery, bounded manual pseudopods,
perimeter recovery, variable creature sizes, and resolution-independent nucleus
behavior.

Code style can be checked with:

```bash
uv run ruff check .
```

## Saved development checkpoints

The repository includes Git tags for important stable stages:

- `pbf-stabile-1`: first stable PBF body;
- `pbf-membrana-poligonale-1`: polygonal membrane and local collisions;
- `pbf-stanza-giocabile-1`: first playable room;
- `pbf-nucleo-deformabile-1`: finite membrane, bounded local pseudopods, and
  deformable nucleus.

These tags make it possible to compare approaches or restore a known working
state without discarding later experiments.
