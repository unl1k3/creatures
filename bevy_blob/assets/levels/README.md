# Level Asset Layout

Every playable or regression level has its own directory and a required
`level.json`. Any images used exclusively by a level live beneath that same
directory, normally in `art/`.

`sewer_01/` is the current playable sewer level:

- `level.json` defines collision geometry, gameplay objects, and visual layers.
- `art/` contains its standard background and platform artwork.
- `art/ink/` contains the ink-preview background and foreground overlay.

The remaining directories are regression labs. They currently use procedural
ink geometry and therefore contain only `level.json`; add an `art/` directory
to the relevant level when it receives unique artwork.

Shared artwork is deliberately avoided: a level JSON always references assets
inside its own directory, so moving, packaging, or replacing a level remains
self-contained.
