# XCLIP command data (in-session)

`xclip` is a single registered engine command reached by the command line, scripts,
menus, CLI, control channel and MCP. It works on one block or external-reference
INSERT in the current space, supplied by `handle` or the current selection.

**This first implementation manages clipping data only. The canvas and plotted /
exported geometry are NOT visually clipped. DXF and DWG do NOT persist the boundary.**
The command reports that limitation when changing the data. JSON serialization and
undo/redo retain it. Do not use a DXF/DWG save as a way to keep this data yet.

## JSON form

- Rectangle: `{"handle":"AB","p1":[0,0],"p2":[10,5]}`.
- Polygon: `{"handle":"AB","points":[[0,0],[10,0],[5,5]],"invert":true}`.
- Polyline: `{"handle":"AB","object":"CD"}`. An open or closed straight-segment
  LWPOLYLINE is accepted; its points are copied, without changing the source entity.
- Existing boundary: `{"handle":"AB","option":"on"}` / `"off"` / `"delete"`.
- Invert: `{"handle":"AB","option":"invert"}` toggles; `"invert":false` sets it.
- Generate an independent, editable polyline: `{"handle":"AB","option":"polyline"}`.
  It uses the current layer/color/linetype/lineweight, not the reference's properties.
- Clip-depth **metadata**: `{"handle":"AB","option":"clipdepth","front":3,"back":2}`;
  `"remove":true` removes both distances. Distances are nonnegative drawing units;
  full UCS/3D plane picking and depth rendering are not implemented.
- Inspect: `{"handle":"AB","option":"?"}` returns the reference and its clip data.

Boundary inputs are drawing-space coordinates. The reference owns a closed
`LwPolyline` in block-definition coordinates, so copying, moving, rotating or scaling
an INSERT carries its boundary. Other instances of the same block are independent.
New boundary replaces the previous boundary (including its depth metadata).
Delete removes both boundary and depth, whereas Off preserves both.

Boundaries have 3–1024 vertices, finite coordinates within ±10¹², no zero-length
edges, no self-intersections, and nonzero area. Arc-segment polylines are rejected
rather than silently flattening them. Invalid requests do not mutate the drawing.

## Typed form

Preselect one reference or pick it after `XCLIP`, then choose
`ON / OFF / Clipdepth / Delete / Polyline / New` (default New).
New offers `Select / Polygonal / Rectangular / Invert` (default Rectangular).
Polygonal accepts successive points, Undo, then Enter. Escape cancels without
changing the reference. Rectangle and polygon stages provide rubber-band previews.

## Clean-room references and follow-up boundary

Behavior was checked against Autodesk's public **XCLIP (Command)** reference,
GUID `104EDB9F-F025-4F67-B5C9-B3F174CFE2F3`.
The public **SPATIAL_FILTER (DXF)** reference, GUID
`34F179D8-2030-47E4-8D49-F87B6538A05A`, specifies a SPATIAL_FILTER object,
point count (70), boundary points (10/20), enabled flag (71), clipping-plane flags
and distances (72/40 and 73/41), and two 4×3 transformation matrices (repeated 40).
The public **Clip Blocks and Xrefs (.NET)** documentation, GUID
`72029044-0840-4187-9A58-F2A4518E3A23`, confirms the reference extension dictionary's
`ACAD_FILTER` dictionary and its `SPATIAL` entry. No proprietary implementation or
reference-application drawing was used.

A real round-trip needs that object/dictionary ownership graph, transforms and
inverted-filter representation, plus DWG support; no custom/fake DXF encoding was
added. Rendering follow-up must share the viewport's `PolyClip` pipeline while
handling nested INSERT transforms, shared expansion budgets, inverted fills,
attributes, infinite primitives and plot-text metadata. The existing viewport path
clips an isolated sub-display-list; directly reusing it on arbitrary INSERTs without
those invariants would silently lose content or break resource bounds.
