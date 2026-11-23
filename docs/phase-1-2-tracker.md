# Phase 1–2 acceptance tracker

Started 2026-11-23. Work request: complete pending work in both phases, not merely one sub-item per row.
Source backlog: `gaps.md`. A delivered slice does not close its entire parent row.

## Phase 1

- F1 material rename: pending; requires a genuine material-name table first. Not equivalent to a working 3D material renderer.
- F4 RECOVER reader: parent implementation written, focused verification running; not yet accepted. Adds a separate hook in native/CLI/web hosts, salvages only complete ASCII DXF ENTITIES records before malformed/truncated tail, reports discarded bytes, keeps recovered drawing dirty, rejects speculative binary/DWG repair. Keep OPEN strict and preserve source files.
- F4 DWG autosave: dependency-blocked by lossless DWG writing (FF1). Keep existing DXF recovery safeguard; do not risk data loss to match an extension.
- FF3: command-line -SAVEAS landed (`3542d622`); native Save dialog per-file version choice and R14 DWG/R12 DXF output remain pending.
- U13: Mac Finder file-open investigation only; macOS reproduction and native event integration remain unverified.
- F19/F20 partially checked entries retain their detailed remaining settings/customization; checked part does not mean the entire feature is done.

## Phase 2

- F14: implementation assigned (plotting agent). Require real printer integration, not hardcoded papers labelled system printers; device PDF margins and plot-transparency persistence are separate criteria. Binary CTB/STB/PC3 remain pending until independently supported.
- F2: command/data model previously landed; clip rendering/persistence, xref in-place editing, nested selection, UI feedback and remaining attribute/reference behavior still need acceptance.
- Precision input Row 1: earlier command/point-input work landed (`43d5291b`); this does not prove every precision/UI gap or full alpha workflow passed.
- F5: rectangular count grips landed (`0cd16276`); polar grips assigned (arrays agent). Path associativity, item deletion and key-point base points remain pending beyond that slice.
- F7: style fields and manager assigned (dimensions agent). Smart DIM and detailed remaining style/render/break behavior must be accounted for separately.
- F10: multileader model/style behavior assigned (leaders agent). Verify public behavior before altering collection structure; no inferred compatibility claims.

## Integration rules

Children own scoped source changes and focused tests, not shared Git commits, planning docs or final CI. Parent owns reconciliation, source review and settled-tree `cargo xtask ci`, including wasm. No phase is complete while any criterion above is pending, blocked or unverified. Installed MSI/local browser builds are separate artifacts and do not update merely because source changed.
