# Where `level-objects.md` went

`docs/behavior/level-objects.md` is split by system and can be deleted once the specifications below are
approved. Every statement of it is stated, unchanged unless noted, in the home given for its section. Use
this table to repoint every citation of `level-objects.md` (code comments, tests and the approved
specifications that cite it): replace "level-objects.md *n*" by the home of section *n*. Where a section's
content was split, the sub-items say which part went where.

Homes (new specifications are marked new):

- **OS** `docs/behavior/offline-simulation.md` (revised)
- **OV** `docs/behavior/object-visual-and-transform.md` (new)
- **SK** `docs/formats/skeleton-ogf.md` (new)
- **MS** `docs/behavior/model-shading.md` (new)
- **OPL** `docs/behavior/object-projected-light.md` (revised)
- **HL** `docs/behavior/hanging-lamps.md` (revised)
- **ODL** `docs/behavior/object-detail-level.md` (unchanged; it already answered two open items)

| `level-objects.md` | Home | Notes |
|---|---|---|
| Introduction, "Builds on", conventions | each home's own | The matrix convention (row vectors, `[p 1] · M`) is stated in OV, SK and MS. |
| 1 Overview | split | Items 1 – 2: OS 6.1 – 6.5; item 3: OV 1 – 4; item 4: SK and MS 2; item 5: MS and HL. |
| 2 Which objects are present | OS 6.5 | |
| 2.1 Objects of the level | OS 5.1, 6.1; OS 9.1 | The level of an object (game vertex): OS 5.1 and 6.1. The believed "only objects of the actor's level are candidates" is settled by OS 6.1 (the current level's list). The 870 records and their parent ids: OS 9.1. |
| 2.2 Switch distances | OS 6.5.1 | |
| 2.3 Switch capability | OS 6.5.2 | Includes the renderer match of hanging lamps and its fatal error. |
| 2.4 The rule | OS 6.5.3; OS 6.2 | The rule and the first evaluation at a New Game: 6.5.3. The switch-pass limits (`process_time`, `update_monster_factor`, `objects_per_update`) with checks LO0-1 … LO0-6: 6.2. |
| 2.5 Scripted replacements | OS 6.5.4; OS 9.1 | The script table and smart-terrain jobs: 6.5.4. "State at the start of a New Game": 9.1. |
| 2.6 Result for `l01_escape` | OS 9.1 | |
| 3 Where each present object's visual comes from | OV | |
| 3.1 Looking up a visual name | OV 1 | |
| 3.2 Source of the name | OV 2; OV 5.1 | The trader's visual on `l01_escape`: OV 5.1. |
| 3.3 Kinds without a model | OV 3 | |
| 3.4 Transform | OV 4 | The rotation rows R[0] – R[2] and *X*; the evidence is OV 5.2. |
| 4 Model files (`.ogf`) | SK | |
| 4.1 Visual types | SK 1 | |
| 4.2 Header boxes | SK 2 | |
| 4.3 Skeleton models | SK 3; MS 2 | "A type-3 model draws exactly like a type-10 model at rest": MS 2. |
| 4.4 Bones | SK 4 | Chunk 13, chunk 16 layout and the bind pose. |
| 4.5 Child meshes | SK 5 | |
| 4.6 Vertex formats | SK 6; MS 1 | "Tangent and binormal are not used by the first renderer's model programs": MS 1. |
| 4.7 How the first renderer stores skinned vertices | MS 1 | |
| 4.8 Skinning and the pose at rest | MS 2 | |
| 4.9 Detail level of skinned progressive meshes | ODL 4; MS 3 | Its unestablished detail value is settled by ODL 4. |
| 5 Shading | MS | |
| 5.1 What is drawn | MS 3; ODL 3 | Its unestablished small-object test is settled by ODL 3 (per mesh). |
| 5.2 Shader resolution | MS 4 | |
| 5.3 Element choice | MS 5; OPL 3.1 | The receiver conditions are OPL 3.1. Its review gap is kept in MS ("Gaps found in review"). |
| 5.4 Per-object light inputs | OPL 5.2, 5.3; MS 6 | Sky and sun visibility rays, directions, initial and converged values, smoothing: OPL 5.2 step 1 and 5.3. The constant L_dynamic_props: MS 6. |
| 5.5 Colour equations | MS 7 | Element 0's projected light texture: OPL 2 – 4, 6. |
| 5.6 Dynamic lights | MS 8 | Which lights and the passes: `dynamic-lights.md` 4 – 6. |
| 6 Hanging lamps' lights (structure) | HL | Light-flag bits: HL 2.2; lights, colour, glow, ambient: HL 3.1 – 3.3; placement: HL 4.2; on/off: HL 5; the `idle` motion: HL 3.4; the five online lamps of `l01_escape` and their flags: HL 9.3. The light-flag bits "of unestablished meaning" are settled by HL 2.2 (no effect). |
| 7 Verification facts (intro) | OV 5; OS 9.1 | |
| 7.1 Objects of `l01_escape` and online objects | OS 9.1; OV 5.1 | The per-class table, the online stalkers and monster, the absent `[spawner]` records: OS 9.1. "Online objects with a model … 352", the 101 model files and their vertex and index counts, the stalkers' visuals: OV 5.1. |
| 7.2 Named objects | OV 5.2; SK 7.2; HL 9.3 | Positions, angles, rotation rows, world boxes and the transform evidence: OV 5.2. Model-file contents (sizes, types, bones, children, sums, bind positions): SK 7.2. The lamp block of `trader_light_0003`: HL 9.3. |
| 7.3 Invariants over the model files | SK 7.1 | |
| 8 Unestablished | per item | Smart-terrain jobs and script state: OS 10. Visual flags byte: OV 6. Detail value and small-object test: settled (ODL 3, 4). Receiver order: OPL 9. Light-track values at the first frame: OPL 9. Hanging-lamp bits: settled (HL 2.2); glow drawing: `glows.md`. Poses in the running game: MS 2, MS 9. |
| 9 Provenance | each home's provenance | |
| Gaps found in review (receiver conditions of 5.3) | MS "Gaps found in review" | Also answered by OPL 3.1. |

Specifications that cite `level-objects.md` and were not revised here keep working citations once repointed
by this table; among them `dynamic-lights.md`, `glows.md`, `object-detail-level.md`, `bone-physics.md`,
`skeletal-motions.md`, `motion-playback.md`, `physics-shells.md`, `torch.md`, `light-shadows-projection.md`,
`narrowed-specs-scope.md`, `trader-and-crows-scope.md`, `particles-and-lights-scope.md` and the other scoping
specifications.
