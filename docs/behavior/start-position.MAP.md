# Where `docs/behavior/start-position.md` went

Every fact of the start-position specification now lives in a system specification; the start view stays
only as worked examples. Citations of start-position sections repoint as below (WC
`docs/behavior/world-coordinates.md`, new; SAV `docs/formats/sav.md`; PM `docs/behavior/player-movement.md`;
VP `docs/behavior/view-projection.md`). Once nothing cites it, start-position.md can be deleted.

| start-position section | New home |
|---|---|
| Introduction (scope; item 6 of level-loading section 8; resolves level-loading 2.5, 5.10, 5.11, 7, 9 camera items) | split by topic: WC (coordinates, angles), SAV 10.1 – 10.2 (spawn records), PM 7 (eye height), VP 1.1, 2 – 2.3 (view, projection, far value) |
| Conventions: paths, sizes, integer and string types, "chunked" | SAV conventions |
| Conventions: angles, row-vector matrices | WC conventions (angles, column-vector rotations); VP conventions (row-vector matrices) |
| Note that `game.graph` is not needed | SAV 10.2 (vertex 4 resolved as in level-loading 2.2, 5.11) |
| 1 World coordinate system (units, +y up, left-handed, spawn coordinates = level coordinates, right-handed conversion) | WC 1 |
| 2 `spawns\all.spawn`: locating the actor's record (chunk 1 layout, object chunks, packet storage, the actor is index 63) | SAV 10.1 |
| 3.1 Spawn packet, common part (field order, version-dependent fields, invalid records, versions below 112, skipping) | SAV 7.2 (fields and version rules; the unnamed fields are named there: game type, respawn time, id, parent id, phantom id, script version, spawn id) and SAV 10.1 (values over all records, skipping) |
| 3.2 The actor's packet | SAV 10.2 |
| 3.3 Object-specific data (the actor's first fields) | SAV 7.4.1 (Object and Visual blocks) and SAV 10.2 (the player's values) |
| 4.1 Stored values (position, angles, bytes) | SAV 10.2 |
| 4.1 The stored position is the feet (floor 0.00013 m above it) | PM 7.1 |
| 4.2 Eye height (formula, box 0 at spawn, values, start eye, offset not rotated, rising lag, lean) | PM 7 and 7.1 |
| 5 Stored angles → yaw, pitch, roll; `lim_pitch`, `lim_yaw` | WC 2 (limits also PM 8.1) |
| 5 Camera basis (R, direction, up, consequences) | WC 3 |
| 5 The actor's start yaw, pitch, direction, up; the trader | WC 4 |
| 6 View transform; `cam_inert` | VP 1.1 |
| 6 The actor's right vector and view matrix row | VP 2.3 |
| 7 Projection matrix; depth range; x right, y up | VP 2 |
| 7 F: field of view 67.5, fixed | VP 3.1 |
| 7 A: aspect factor and its examples | VP 1 (step 1) and VP 2 |
| 7 N: near plane 0.2 / 0.01 | VP 3.1 |
| 7 Z: far plane | VP 2.1 |
| 7.1 Far plane (formula, weather keys, `rs_vis_distance`, environment modifiers) | VP 2.1 |
| 7.1 The start's far plane 254.478 m | VP 2.3 |
| 7.2 Smoothing of field of view, aspect and far plane; start values 90, 1, 100 | VP 1 (step 2) and VP 2.2 |
| 8 Verification facts: `spawns\all.spawn` layout, invariants, sums | SAV 10.1 and SAV 15 |
| 8 Derived values for the start (eye, yaw, pitch, direction, far plane) | PM 7.1, WC 4, VP 2.3 |
| 9 Unestablished: meanings of the unnamed packet fields; legacy fields below 112 | SAV 7.2 and SAV 16 |
| 9 Unestablished: triangle winding | WC 5 |
| 9 Unestablished: first-frame time; a dynamic object within 0.2 m of the start eye; scripted camera effects at the start | VP 6 |
| 10 Provenance | the provenance section of each receiving specification |
