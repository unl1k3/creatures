# Bevy Blob Prototype

A playable 2D vertical-platformer prototype built with Bevy 0.19. The player
controls a deformable blob that can roll, compress, jump, split into smaller
independent creatures, merge back together, and fire radial bursts of acid.

The body is simulated by a dedicated soft-body solver based on Verlet
integration. It combines membrane distance constraints, curvature resistance,
area preservation, collision response, deformation limits, and automatic
topology recovery. Avian provides the environment's static rigid bodies and
collision layers, while the living blob remains on the custom solver during the
incremental physics migration.

The migration currently runs Avian membrane contact probes in shadow mode.
Every membrane particle is projected against the static Avian environment and
compared with the legacy platform geometry. These probes collect diagnostics
but apply no forces, preventing duplicate collision responses while the two
models are validated.

Each detected contact is retained in a per-blob manifold containing the
particle index, Avian collider entity, projected surface point, outward normal,
and estimated depth. This data is ready to drive the custom soft-body response
once the shadow-mode comparison is sufficiently stable.

The initial floor and the first three suspended platforms are migrated collision
surfaces. They are excluded from the legacy membrane solver and resolved through
Avian at the fixed 120 Hz physics rate. Point projection handles resting
contacts, while a swept ray along each particle's movement prevents fast small
fragments from tunnelling through the platform and preserves the face hit from
above, below, or the side. The response removes inward normal velocity without
restitution, restores particle clearance, reports ground support, and feeds
impact speed into the trauma system. The final two platforms still use the
legacy response, limiting the scope of the migration.

Blob-to-blob and blob-to-carcass broad contact now uses temporary convex Avian
proxies generated directly from the current membrane contours. Avian returns a
2D contact manifold with up to two surface points. Compression is distributed
around those points while the Verlet solver retains control of the actual soft
body. Living patches store deformation energy; carcass patches apply the same
shape change inelastically. Corrections are centre-balanced so contact
deformation cannot create artificial translation.

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
- On Windows, the Rust MSVC toolchain and the Visual Studio C++ Build Tools.

## Running the Game

From the repository root:

```bash
cd bevy_blob
cargo run
```

The game opens three native desktop windows. At startup, the 900 x 900 game
window is placed on the left, while the controls and live metrics are stacked
on the right. The layout reserves additional room for native title bars and
window borders so none of the three windows overlap. The operating system may
adjust these requested positions to keep the windows inside the usable area of
a smaller display or a multi-monitor desktop.

The auxiliary windows use Bevy's embedded font with increased size, weight,
contrast, and a subtle shadow. No external font file or installed system font is
required, so text rendering remains consistent on macOS and Windows.

On a Windows machine, run the same commands from PowerShell or a Developer
Command Prompt. Bevy creates normal native Windows windows; gameplay and input
remain unchanged.

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
| Hold `C` | Extend the proboscis and probe for a nutrient |
| `Space` | Fire a radial acid burst when the blob is large enough |
| Hold `Q` | Deploy the pseudo-spine shield |
| `H` | Show or hide the controls legend |
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

## Pseudo-Spine Shield

Holding `Q` gradually deploys a corona of pseudo-spines from the selected
blob's membrane. The shield consumes biological energy while active and
recharges after fully retracting. Large blobs form a denser corona, while small
fragments receive fewer spines and fragments below 30% of the initial radius
cannot deploy it.

Spine positions, widths, and lengths vary through a stable per-creature random
layout, so the silhouette is irregular without flickering between frames.
When a spine reaches level geometry, its growth is continuously shortened to
the first contact point. It therefore rests against platforms and walls instead
of intersecting them or disappearing abruptly.

The shield cancels jump charging, reduces horizontal control, and prevents acid
fire while deployed. Its geometry follows the deforming membrane without adding
unstable particles to the soft-body solver. A cyan ring inside the selected
blob displays the remaining shield energy.

Press `H` at any time to show or hide the separate controls window.

## Vitality, Trauma, and Corpses

Every fragment has independent energy and health. Ordinary existence drains
energy very slowly, while movement, jump charging, the pseudo-spine shield, and
acid attacks consume more. Falling energy progressively lowers acceleration,
maximum speed, jump power, shield deployment, and the number and speed of acid
droplets. A depleted creature begins to lose health and gradually deflates.

The soft-body collision solver records the strongest normal impact against
level geometry. Impacts above the safe threshold reduce health and accumulate
trauma; trauma dissipates gradually, but a single extreme collision or repeated
hard impacts can kill a fragment. Normal landings remain below the initial
damage threshold.

Death by depletion and death by trauma remain distinct states. Control and idle
animation stop immediately, but the corpse keeps the blob's shape, size, color,
and soft-body material. There is no collapse or mineralization process. A corpse
cannot breathe, accept input, attack, deploy defenses, split, or rejoin, but it
remains a physical part of the level. Gravity, collisions, impacts, and other
blobs can push it, roll it off an edge, or make it fall, and it can support living
creatures. Selection moves to another living fragment when one exists. Press `R`
to restore the initial creature.

## On-Screen Metrics

A separate metrics window reports the smoothed frame rate, frame time in
milliseconds, fixed physics rate, active blob count, particle count and relative
size of the selected creature, life state, energy, health, accumulated trauma,
strongest recent impact, shield energy, and number of active acid droplets. It
is rendered by an isolated camera, so the game scene cannot cover the text. The
controls use a second dedicated window that can be shown or hidden with `H`.
During the physics migration it also reports Avian and legacy membrane-contact
counts, their agreement percentage, distinct supporting surfaces, grounded
contact points, maximum depth, and horizontal contact span.

## Project Layout

```text
bevy_blob/
├── Cargo.toml          Rust package metadata and dependencies
├── Cargo.lock          Reproducible dependency versions
├── README.md           Project documentation
├── PHYSICS_TEST_SCENARIOS.md  Difficult-contact layouts and acceptance criteria
├── assets/             Runtime level definitions and visual artwork
│   └── levels/sewer_01/level.json  Standard sewer geometry and layer references
├── src/                Game source code and automated tests
│   ├── main.rs         Application setup, world state, simulation, split/merge flow
│   ├── blob.rs         Soft-body model, constraints, movement, and collisions
│   ├── acid.rs         Acid weapon, droplets, cooldown, simulation, and rendering
│   ├── shield.rs       Pseudo-spines, defensive energy, and movement penalties
│   ├── vitality.rs     Energy, wasting, impact trauma, death, and carcasses
│   ├── nutrition.rs    Nutrient ingestion, digestion, energy recovery, and waste
│   ├── hud.rs          Dedicated controls and live-metrics windows
│   ├── input.rs        Keyboard input, selection, reset, and player actions
│   ├── level_format.rs Versioned JSON parsing and level validation
│   ├── camera.rs       Smooth tracking of the selected creature
│   ├── environment.rs  Level geometry, Avian layers, and static colliders
│   ├── rendering.rs    Dynamic meshes, family colors, outlines, and indicators
│   ├── blob_tests.rs   Soft-body and movement regression tests
│   └── game_tests.rs   Split, merge, camera, collision, and rendering tests
└── target/             Generated Cargo build output; not tracked by Git
```

Use [PHYSICS_TEST_SCENARIOS.md](PHYSICS_TEST_SCENARIOS.md) as the shared manual
test plan for irregular supports, blob stacks, and movable corpses.

Press `F1` through `F6` to load the standard level or one of five grouped test
laboratories. `F1` now crosses both lateral wings of the sewer before continuing
through its vertical platforms. `F2` combines narrow supports, stairs, corners, and a bridge; `F3`
combines a shallow ramp, semicircle, and segmented wave; `F4` combines a U-shaped
pocket with a low horizontal passage; `F5` is a separate fall-and-impact course
with narrow landings, alternating ledges, a drop shaft, and overhead contacts;
`F6` contains the V-shaped valley and split bridge. Each switch resets the active
blob.
All test geometry uses actual static Avian colliders, with the same tessellated
profiles used for collision detection and rendering.

Test laboratories use a wider camera scale and a small upward look-ahead so the
next structure is visible before committing to a jump. Numbered amber markers
grow progressively with their sequence and disappear after the selected blob
reaches them in order. Resetting the scenario restores the complete route.

## Current Architecture

The physics simulation runs at a fixed 120 Hz. Each active blob owns its soft
body and genealogy metadata. Rendering systems create one dynamic mesh per blob
and update its vertices from the simulated membrane. Mesh entities are added and
removed automatically when creatures split, merge, or reset.

The selected creature is shown with a brighter, more opaque material, while its
family color remains visible. The camera follows that creature in both axes and
smoothly changes target after pressing `Tab`.

## Nutrition and Energy Recovery

Small amber nutrients are physical level objects: they fall, settle on
rectangular and convex geometry, and can be pushed by a blob. Pressing `C`
produces one exploratory proboscis. While `C` remains held it grows, stays
extended, and continuously sweeps toward the nearest nutrient; releasing the
key retracts it smoothly. A nutrient that enters its reach while it is probing
can therefore be caught dynamically. When the selected living blob is large
enough and the rounded tip physically reaches a nutrient, a 1.25-second
engulfing phase begins. The tip first widens its grip around the nutrient; the
nutrient then remains attached to that rounded end while the whole proboscis
slowly retracts with eased motion. As soon as the nutrient crosses the membrane,
the proboscis disappears and releases it. The nutrient then falls freely inside
the creature, settling with a heavily damped internal bounce while digestion
continues. Temporary render points form this single curved, tapered appendage
without destabilizing the physical particle topology. They disappear
automatically when the interaction ends. The nutrient remains visible inside
the body for a six-second digestion cycle. Energy is restored gradually rather
than awarded on contact, up to 46 percentage points for a complete nutrient.

Each protrusion receives a stable organic variation. Its two curvature
controls, thickness, taper, and small width waves differ without being
randomized every frame, so repeated probing looks irregular without flicker.
Its render topology also grows with visible area: a minimal five-vertex shape
is used at birth, followed by medium and full curve sampling only after enough
space exists for stable triangles.
The first Bézier control is constrained to the outward normal of the two-edge
attachment chord. Random bending begins farther along the appendage, preventing
the nascent mesh from initially sliding almost tangent to the membrane.
The attachment is selected once when probing starts and stored in material
membrane coordinates: a particle-edge index plus a fixed interpolation value.
It follows deformation of that same edge but never slides after the moving tip.
Only the appendage curve and tip explore. Additional temporary shoulder and
curve samples smooth the junction without increasing the soft-body solver cost.
The attachment spans two consecutive physical edges: their shared particle is
the fixed material centre of the root, with one anchor on each edge. No extra
membrane refinement is required. The appendage mesh connects the two anchors
directly and then narrows progressively, avoiding a central pinch.

The digestive load initially reduces movement, jump power, pseudo-spine
deployment, and acid output to 48% of normal capacity. Performance returns
progressively as absorption advances. A digesting blob cannot split or rejoin.
When absorption completes, the remaining material starts from the exact place
where it settled inside the blob and immediately receives an upward launch
impulse. It travels through the interior with strong viscous drag; after the
whole pellet clears the membrane it switches to the normal external drag and
gravity. Its angle and launch strength vary on
each digestion while retaining an upward bias, and it also inherits a limited
part of the blob's current velocity. The resulting waste pellet has 42% of the
nutrient's original radius; it then falls under gravity, collides with
rectangular and convex level surfaces, can be nudged by blobs, and remains
visible in the environment. If the host dies during digestion,
the reduced residue is expelled immediately.

Resetting or switching a test laboratory restores the nutrients around the new
spawn point. Digestion progress and current capability are reported in the
metrics window.

## Validation

The current automated suite contains 97 tests covering soft-body constraints,
jump charging and size scaling, rolling, collision recovery, topology repair,
recursive splitting, hierarchical merging, camera targeting, dynamic mesh
generation, acid bursts, vitality states, death causes, and complete weapon
reset behavior.

## Level files

The standard sewer level is defined in
`assets/levels/sewer_01/level.json`. The versioned JSON contains world size and
center, blob spawn, route checkpoints, rectangular or polygonal collision geometry,
optional visual image layers, nutrients, lights, expulsion points, hazards, and
decorations. Physics never derives collision shapes from artwork, so images can
be replaced without changing gameplay.

Visual layers use asset-relative image paths plus an explicit world position,
size, and depth. Keep background images below the physics drawing depth and
foreground images above it. Large levels can use several adjacent images
instead of a single oversized texture.

Press `F7` to overlay level bounds, collider geometry, route checkpoints, and
the spawn marker on the artwork. While it is active, use `I/J/K/L` to pan,
`U/O` to zoom, and `P` to return to the selected blob. This diagnostic view
also shows light radii, expulsion directions, and hazard volumes without
altering physics. Closing the overlay preserves its camera position and zoom;
press `P` to return to the selected blob and resume automatic tracking.

## Roadmap

1. Add controlled anchoring and stretching as a gameplay ability.
2. Add enemies, damage, and acid-hit reactions.
3. Introduce shader-based organic shading and liquid highlights.
4. Improve the visual transitions for splitting and merging.
5. Add moving platforms and more advanced Avian world interactions.
