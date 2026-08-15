# Bevy Blob Prototype

A playable 2D vertical-platformer prototype built with Bevy 0.19. The player
controls a deformable blob that can roll, compress, jump, split into smaller
independent creatures, merge back together, and fire radial bursts of acid.

The body is simulated by a dedicated soft-body solver based on Verlet
integration. It combines membrane distance constraints, curvature resistance,
area preservation, collision response, deformation limits, and automatic
topology recovery. Avian is included for future rigid bodies and world queries,
while the blob itself currently uses the custom solver.

## Features

- Soft-body movement with visible rolling instead of rigid translation.
- Proportional jump charging through body compression.
- Size-dependent jumps: smaller fragments can jump higher.
- Air-control damping and impact-shape recovery.
- Dynamic filled meshes that follow every membrane particle.
- Uneven random splitting with preserved area, mass, and momentum.
- Recursive splitting up to four active blobs when enough particles remain.
- Independent fragment selection and camera tracking.
- Family colors that identify fragments that can merge together.
- Deliberate sibling rejoining with attraction, obstacle checks, and timeout.
- Radial acid bursts for blobs above the minimum physical size.
- Automated protection against collapsed or self-intersecting membranes.

The initial creature uses a global physical scale of 65%. Collision margins,
indicators, rendering, and split fragments all derive their dimensions from the
same physical scale.

## Requirements

- A recent stable Rust toolchain with Cargo.
- A graphics adapter and drivers supported by Bevy/wgpu.

## Running the Game

From the repository root:

```bash
cd bevy_blob
cargo run
```

Run the automated checks with:

```bash
cargo test
```

## Controls

| Input | Action |
| --- | --- |
| `A` / `D` or Left / Right Arrow | Roll and move horizontally |
| Hold Down Arrow | Compress the selected blob and charge a jump |
| Release Down Arrow | Jump with power based on the accumulated charge |
| `X` | Split the selected blob, up to four active fragments |
| `Tab` | Select the next active blob |
| `E` | Start rejoining the selected blob with its sibling |
| `R` | Reset the game and restore the initial creature |
| `Space` | Fire a radial acid burst when the blob is large enough |
| `Esc` | Exit the game |

## Movement and Jumping

Horizontal movement applies torque to the membrane while the creature is on a
surface, producing visible rolling instead of translating the body rigidly.
Air control is intentionally weaker and does not inject rotation into the soft
body.

Holding the Down Arrow compresses the creature and progressively charges the
jump. Short, medium, and full charges produce clearly different launch power.
Jump strength is scaled non-linearly by physical size: large blobs have a lower,
more controllable jump, while smaller fragments gain a substantially greater
vertical advantage. The multiplier is capped so the smallest valid fragments
cannot reach unstable speeds.

Pressing `R` performs a complete gameplay reset. It restores the original blob,
selection and genealogy state, cancels rejoining, and removes active acid drops
and weapon cooldowns.

## Splitting and Rejoining

Splitting creates two uneven fragments with a randomized size ratio. The
membrane resolution is increased while physical area, mass, dimensions, and
momentum are preserved. A selected fragment may split again only when it still
contains at least 16 source particles, and no more than four blobs may be active
at the same time.

Each fragment retains its lineage. Siblings share a family color and must merge
before the reconstructed parent can merge at the next level. Pressing `E`
enables attraction only for the selected sibling pair. They roll toward one
another and merge on contact if no platform blocks the path. The attempt is
cancelled after four seconds when contact cannot be achieved.

## Acid Defense

Pressing `Space` emits droplets from points around the selected blob's membrane.
The directions contain random variation but are distributed over the full
circle, keeping the burst useful as an area-defense action. Larger creatures
produce more droplets.

The weapon is available only when the blob radius is at least 55% of the
initial radius. This makes splitting a strategic trade-off: small fragments are
more mobile and jump higher, but sufficiently small fragments lose access to
the acid burst. The weapon has a 0.85-second cooldown, produces mild recoil, and
its droplets disappear after hitting level geometry or reaching their lifetime.

## Project Layout

```text
bevy_blob/
├── Cargo.toml          Rust package metadata and dependencies
├── Cargo.lock          Reproducible dependency versions
├── README.md           Project documentation
├── src/                Game source code and automated tests
│   ├── main.rs         Application setup, world state, simulation, split/merge flow
│   ├── blob.rs         Soft-body model, constraints, movement, and collisions
│   ├── acid.rs         Acid weapon, droplets, cooldown, simulation, and rendering
│   ├── input.rs        Keyboard input, selection, reset, and player actions
│   ├── camera.rs       Smooth tracking of the selected creature
│   ├── rendering.rs    Dynamic meshes, family colors, outlines, and indicators
│   ├── blob_tests.rs   Soft-body and movement regression tests
│   └── game_tests.rs   Split, merge, camera, collision, and rendering tests
└── target/             Generated Cargo build output; not tracked by Git
```

## Current Architecture

The physics simulation runs at a fixed 120 Hz. Each active blob owns its soft
body and genealogy metadata. Rendering systems create one dynamic mesh per blob
and update its vertices from the simulated membrane. Mesh entities are added and
removed automatically when creatures split, merge, or reset.

The selected creature is shown with a brighter, more opaque material, while its
family color remains visible. The camera follows that creature in both axes and
smoothly changes target after pressing `Tab`.

## Validation

The current automated suite contains 42 tests covering soft-body constraints,
jump charging and size scaling, rolling, collision recovery, topology repair,
recursive splitting, hierarchical merging, camera targeting, dynamic mesh
generation, acid bursts, and complete weapon reset behavior.

## Roadmap

1. Add controlled anchoring and stretching as a gameplay ability.
2. Add enemies, damage, and acid-hit reactions.
3. Introduce shader-based organic shading and liquid highlights.
4. Improve the visual transitions for splitting and merging.
5. Add moving platforms and more advanced Avian world interactions.
