# Target-app parity: CADCraft vs AutoCAD 2027

> **Last reviewed:** 2026-11-23 · **Last updated:** 2026-11-23 · **Change:** major (fresh full weighted re-audit: three parallel agents re-read actual source + `git log --since=2026-10-24` for all 14 feature-area rows and all 9 cross-cutting dimensions, rather than trusting stale docs/gaps.md prose. Features dimension 41% → **≈ 43%**; ready for real work 38% → **≈ 41%**; mainstream practitioner 28% → **≈ 30%**; essentials user 46% → **≈ 48%**. Feature breadth (296/491=60%) unchanged, already fresh from 2026-11-21) · **Target:** Autodesk AutoCAD 2027

The authoritative assessment of how close CADCraft is to AutoCAD, and how much Opus 5.5 work
remains. [ROADMAP.md](../ROADMAP.md) summarises it, [gaps.md](gaps.md) lists every shortfall,
and the deep checklists are [geometry-parity.md](geometry-parity.md),
[ui-parity.md](ui-parity.md), [file-format-parity.md](file-format-parity.md),
[hardware-parity.md](hardware-parity.md) and [localization-parity.md](localization-parity.md).

## Headline

| Number | Value | Kind |
|---|---|---|
| **Feature breadth** (AutoCAD for Mac menu items with a live command) | **296 / 491 = 60%** by `cargo xtask parity` (measured 2026-11-21, unchanged this pass; mechanical source scan, UI-only items not included) | measured ([parity-checklist.md](parity-checklist.md), `cargo xtask parity`) |
| Feature breadth, 2D menus only (excluding 150 3D items) | **≈ 65%** (222 / 341 by `cargo xtask parity`'s rule) | measured (source scan, 2026-10-10) |
| Feature depth, weighted by use (features dimension) | **≈ 43%** | re-audited (table below) |
| **Ready for real work** | **≈ 41%** | re-audited (dimension table below; weighted 40.8%) |
| Mainstream practitioner (2D drafter, weekly tools) | **≈ 30%** | re-audited ([below](#mainstream-practitioner)) |
| Essentials user (core tools, default settings) | **≈ 48%** | re-audited ([below](#essentials-user)) |
| Remaining to alpha (gate rows) | **≈ 100–160 Opus 5.5 hours** | estimated |
| Remaining to beta | **≈ 560–880 Opus 5.5 hours** | estimated |
| Remaining to full parity | **≈ 1,220–1,970 Opus 5.5 hours** | estimated |

Stage: **pre-alpha**. Four of the five core workflows fail the [alpha gate](roadmap.md#alpha-gate) (precise drafting with tracking, blocks, plotting, lossless DWG save), and ready for real work is under the ~40% alpha bar (now just over it in the weighted composite, but the gate is pass/fail per workflow, not the composite — still pre-alpha). Alpha is ≈ 100–160 h away.

## Readiness by audience

| Audience | Ready % | Opus 5.5 agent wall-clock hours to ~95% | Work that dominates |
|---|---:|---|---|
| Full target (ready for real work) | ≈ 41% | ≈ 1,070–1,720 h (≈ 70% parallelises across 4–6 agents: ≈ 280–450 h elapsed) | 3D kernel (300–450 h, still the largest single gap, untouched since 2026-10-24), DWG/DXF binary fidelity, AutoLISP/ecosystem depth (now 22%, up from 10%), blocks/xrefs/dynamic blocks, localization |
| Mainstream practitioner (2D drafter) | ≈ 30% | ≈ 530–820 h (≈ 70% parallelises: ≈ 140–220 h elapsed) | lossless DWG exchange and a real-file corpus, precision input (tracking, dynamic input — unchanged this pass), 2D geometry precision, blocks/xrefs/dynamic-block authoring, annotation depth |
| Essentials user | ≈ 48% | ≈ 130–210 h (≈ 50% parallelises across 3 agents: ≈ 55–105 h elapsed) | stubbed options in basic draw/modify commands (array grips, CV editing), opening DWGs people send, plot dialog, dynamic-block authoring |

Hours are subsets (essentials ⊂ mainstream ⊂ full) and use the calibration in [Remaining effort](#remaining-effort-and-how-it-was-calibrated): ≈ 25 commands
per agent-hour at presence quality in the first build (≈ 11.5 agent-hours), 0.3–0.7 h per option
or bug fix (≈ 60 such fixes on 2026-10-10), 2–5 h per moderate feature, 15–100 h per subsystem.

The full number is an additive weighted sum over the dimensions (table below); only the
mainstream and essentials numbers use multiplicative discounts.

## Target and how it was measured

- **Target:** Autodesk AutoCAD 2027 for Mac, version 26.0 (installed under
  `/Applications/Autodesk/AutoCAD 2027`), and AutoCAD 2027 for Windows from Autodesk's
  documentation where the two differ (Ribbon, Sheet Set Manager, Express Tools, .NET).
- **Clean room:** the installed bundle was inspected by **listing names only**: localizations
  (`Base`, `de`, `es`, `fr`, `it`, `ja`, `ko`, `zh_CN` `.lproj` folders), `Contents/PlugIns`
  module names (AcSolids, AcDim, AcTable, AcSection, AcSubD, AcPublish, AcETransmit, AcXDiff /
  DWG Compare, AcCounting, AcMarkupAssist, AcTrace, AcSmartCenter, AcSpeller, Vlide …, about 130
  modules), and `Resources/Support` file names. Nothing inside a file was read, nothing was
  copied, AutoCAD was not launched in this pass.
- **Menu oracle:** `xtask/data/menu-catalog.txt` (feature names only, 491 leaf items, recorded from
  the running AutoCAD 2027 for Mac) against the command registry.
- **Behaviour reference:** `plan/autocad/01-observed-ui.md` and local reference screenshots (black
  box, never committed); Autodesk's public command reference.
- **CADCraft:** the source on `main` at bd2412f (2026-10-10/11), read by four agents in
  parallel: command-by-command option audits (prompts that print "not available yet"), the
  geometry kernel, DXF/DWG paths, interaction code. Plus the 88 GitHub issues and 47 open PRs.
  Disk was nearly full, so nothing was built or run: `cargo xtask parity` was reproduced by a
  Python scan of `CommandSpec` menus with the xtask's matching rule, which gives exactly the same
  uncovered list as the last generated report (233 / 491). Re-run `cargo xtask parity` to
  confirm.

## Mainstream practitioner

The typical AutoCAD professional is a 2D drafter or detailer: draw, modify, dimension, use
blocks, manage layers, plot sheets, and exchange DWGs with colleagues, every week. Excluded per
the standard: 3D, AutoLISP and plug-ins, cloud/AI (Autodesk Assistant, Smart Blocks, Traces,
Share), Sheet Set Manager administration, specialist hardware, languages other than English.

| Area (weekly use) | Weight | Depth | Weighted |
|---|---:|---:|---:|
| Draw (2D) | 15% | 60% | 9.0 |
| Modify | 18% | 52% | 9.4 |
| Precision input (snaps, tracking, coordinate entry, dynamic input) | 13% | 40% | 5.2 |
| Layers and properties | 8% | 72% | 5.8 |
| Dimensions | 10% | 47% | 4.7 |
| Text and MTEXT | 6% | 35% | 2.1 |
| Leaders and tables | 3% | 27% | 0.8 |
| Hatch | 5% | 38% | 1.9 |
| Blocks, attributes, xrefs | 8% | 30% | 2.4 |
| Layouts and plotting | 8% | 38% | 3.0 |
| Inquiry and utilities (incl. autosave) | 3% | 32% | 1.0 |
| Command line and navigation | 3% | 58% | 1.7 |
| **Average depth** | 100% | | **≈ 47.0%** |

Depths are the area percentages above and in [ui-parity.md](ui-parity.md), re-audited 2026-11-23
against actual source (`crates/engine/src/cmd/*`) and `git log --since=2026-10-24`: Modify,
Layers/properties, Dimensions, Leaders/tables, Hatch, Blocks/xrefs and Layouts/plotting all got
small real bumps (OVERKILL, MLEDIT tee mitering, colour books, Match Properties Settings dialog,
DIMALTTD/DIMALTTZ fix, Geometric Tolerance dialog, table word-wrap, XCLIP, Object Grouping
dialog, viewport-clip coupling); precision input and command line/navigation found no direct
evidence of movement and are carried forward unchanged. File I/O is not an area here; it is
counted once, as the file-exchange discount.

| Discount | Factor | Evidence |
|---|---:|---|
| Interaction fidelity (beyond the precision-input area) | ×0.90 | hot-grip options ignored (array grips still explicitly unimplemented, `array.rs:572,945`), no context menus, selection keywords W/C/F/WP/CP rejected (#224), transparent 'ZOOM cancels the running command (#220), ELLIPSE/BREAK keywords ignored (#219) — unchanged this pass |
| Stability on real machines | ×0.88 | CI now gates every pull request (not just pushes to main) via `cargo xtask ci` (fmt, clippy, tests, assets, layers, wasm) — closes the prior "no CI runs the Rust tests on PRs" gap; several of the 48 commits since 2026-10-24 are closed-issue proofs rather than new regressions (#166, #44, #134 confirmed already fixed); no full-command UI smoke test yet (S5 open) |
| Exchanging files with AutoCAD users | ×0.80 | saves drop unmodelled entities, images, layer states, multileader styles, true colours and more (#79, #201, #204, #208, #209, #213, #223); never checked in AutoCAD; a 10 MB DWG failed (#57); no binary-DWG-fidelity work landed since 2026-10-24 |

**Mainstream practitioner ≈ 47.0% × 0.90 × 0.88 × 0.80 ≈ 30%** (re-audited 2026-11-23). It is lower than
ready for real work (≈ 41%) for CADCraft, unlike most apps: the full number credits additively
what CADCraft does well beyond 2D drafting (platforms 80%, performance 60%, ecosystem 22%), while
here the three blockers a working drafter hits every day multiply.

**User evidence** (GitHub issues, PRs and comments, excluding maintainers, counted 2026-10-10):
97 issues filed (60 closed, 37 open) and 127 PRs from 19 outside authors. Praise: 2 (#65:
"I was looking for Autocad replacement for years"; #18), plus thanks on a translation PR (#218).
"Switched from AutoCAD" reports: 0. Open issues: 27 core-path bugs (file data loss, plotting,
selection, layers, web open/save, large DWG), 6 niche requests (ViewCube drag, preferences, SVG
icon, magnifier, constraint edge case, macOS window controls), 4 other (icon offer, trademark,
"Revitcraft", dynamic-block help offer). Interest is real; the open issues are about the core
path, which matches the discounts.

## Essentials user

A casual user who sticks to the essential features: start a drawing, the common draw and modify tools with default
settings, undo, save, print a PDF. Excluded: advanced options, sheet sets, xrefs, plot styles,
annotation styles, exchange edge cases and everything excluded above.

| Core feature | Weight | Depth | Weighted |
|---|---:|---|---:|
| Launch, Start tab, new/open drawing | 10% | 72% | 7.2 |
| LINE, PLINE, CIRCLE, ARC, RECTANG | 20% | 70% | 14.0 |
| MOVE, COPY, ROTATE, SCALE, MIRROR, ERASE | 15% | 70% | 10.5 |
| OFFSET, TRIM (quick), EXTEND, FILLET | 10% | 50% | 5.0 |
| Running object snaps, ortho, polar | 10% | 55% | 5.5 |
| Basic dimensions (linear, aligned, radius) | 8% | 62% | 5.0 |
| TEXT and MTEXT | 5% | 55% | 2.8 |
| Layers (create, colour, on/off) | 7% | 77% | 5.4 |
| Pan, zoom, picking and window selection | 5% | 65% | 3.3 |
| Undo, redo | 3% | 85% | 2.6 |
| Save and reopen own drawings | 4% | 75% | 3.0 |
| Print or export to PDF | 3% | 47% | 1.4 |
| **Average depth** | 100% | | **≈ 65.5%** |

Re-audited 2026-11-23: Launch/Start tab and Layers nudged up with stability (CI gate) and the
colour-book/Match-Properties-dialog landings; basic dimensions and Print/PDF nudged up with the
matching feature-area rows; everything else found no direct evidence of movement since 2026-10-24
and is carried forward unchanged.

| Discount | Factor | Evidence |
|---|---:|---|
| Launch and stability | ×0.92 | startup crash on Intel UHD graphics (#33, fixed in 0.4.0), lines not visible (#44, confirmed already fixed 2026-11-23), Linux mouse delay (#25), web open/save fixed (#55, #133, confirmed already fixed); CI now gates every PR with `cargo xtask ci` |
| Discoverability and UI clarity | ×0.93 | no right-click menus still (dynamic input got real pointer boxes in #60); a standalone Match Properties Settings dialog and a Geometric Tolerance dialog landed since 2026-10-24, but Print still has no plot dialog (#160) |
| Opening files people send them | ×0.85 | DWGs from others untested on real files; a 10 MB DWG failed (#57); `.dxf` from Finder fails (#158); pre-2007 CJK/Cyrillic DXF garbled (no code pages); no binary-DWG-fidelity work landed since 2026-10-24 |

**Essentials user ≈ 65.5% × 0.92 × 0.93 × 0.85 ≈ 48%** (re-audited 2026-11-23).

## By feature area (the features dimension)

Weights are the share of a typical AutoCAD user's working time (2D drafters and detailers are
the large majority; 3D modelling and cloud features are a minority). Percentages are depth, not
presence: an option that prints "not available yet" doesn't count.

| Area | Weight | Parity | Remaining (Opus 5.5 h) | Evidence and main gaps | Doc |
|---|---:|---:|---|---|---|
| Draw (2D) | 13% | 60% | 15–25 | **Unchanged since 2026-10-24** (re-audited): only touch in scope was BREAKLINE's Block option importing a symbol from a .dwg/.dxf (#E3), a minor addition. `draw.rs:1323` still returns "CV: not available yet" for control-vertex editing. ARC 11/11, CIRCLE 6/6, RECTANG all options, REVCLOUD, WIPEOUT, XLINE, PLINE/SPLINE options done; MLINE, REGION and HELIX are objects; isometric drafting done | [gaps](gaps.md#features) |
| Modify | 15% | 52% | 35–55 | Up (re-audited): OVERKILL segment-vs-object overlap detection (`cmd/overkill.rs`), MLEDIT tee mitering (`cmd/draw2/mline.rs`), clipped-viewport/clip-boundary coupling on MOVE/COPY/SCALE/STRETCH (`cmd/layout/viewports.rs`) all landed since 2026-10-24. Array grip-editing still explicitly unimplemented (`array.rs:572,945`, `array/edit.rs:749`) | [gaps](gaps.md#features) |
| Geometry precision | 10% | 41% | 65–105 | Up slightly (re-audited): a new Weiler-Atherton-style boundary-split module (`cmd/inquiry/regions/region_merge.rs`) lets UNION/SUBTRACT/INTERSECT handle properly-crossing region boundaries, not just touching ones; scoped to region booleans only — ellipse/spline intersection, offset and closest-point code in `crates/geom` untouched, still tessellated | [geometry-parity](geometry-parity.md) |
| Layers and properties | 7% | 72% | 8–13 | Up (re-audited): `-LAYER`/`-ATTEDIT` Color now resolves colour-book colours (`cmd/layer/prompts.rs`), a standalone Match Properties Settings dialog landed (`cmd/props/matchprop.rs`, `ui-egui/src/*`). Linetype count (19 of ~45) and transparency/Quick Properties unchanged | [gaps](gaps.md#features) |
| Dimensions | 9% | 47% | 22–32 | Up (re-audited): DIMALTTD/DIMALTTZ now actually format the alternate measurement's tolerance (previously had no effect); a shared tolerance-frame builder backs a new Geometric Tolerance dialog and LEADER/QLEADER GD&T annotation. Still no DIMSTYLE manager dialog (confirmed: no such file in `crates/ui-egui`) | [gaps](gaps.md#features) |
| Text and MTEXT | 6% | 35% | 30–45 | **Unchanged since 2026-10-24** (re-audited): only touch in scope was TXT2MTXT's uniform line-spacing option, a text-import utility, not MTEXT itself. No FIELD command exists anywhere in `crates/engine/src/cmd/*.rs`; editor still a dialog not in-place; no SHX | [gaps](gaps.md#features) |
| Leaders, tables, fields | 4% | 27% | 23–33 | Up slightly (re-audited): table cells now word-wrap with row growth (`crates/render/src/table.rs`, tested); LEADER/QLEADER gained GD&T annotation via the shared tolerance-frame builder. Still no MLEADEREDIT/ALIGN/COLLECT, no FIELD — confirmed unchanged by source inspection | [gaps](gaps.md#features) |
| Hatch and gradients | 5% | 38% | 13–22 | Up (re-audited, partly a stale-claim correction): still 23 of ~85 patterns; Hatch/Gradient dialog gained a real Preview button; the prior "gradients render flat" claim was **false** — `crates/render/src/hatch.rs` has rendered true 64-level stepped gradients since initial import, corrected here | [gaps](gaps.md#features) |
| Blocks, attributes, xrefs, groups | 9% | 30% | 70–108 | Up (re-audited): XCLIP landed (`cmd/blocks/xclip.rs`) but is data-model-only — canvas/plot rendering isn't clipped and DXF/DWG doesn't persist the clip, so it's lost on save/reopen (`docs/xclip.md:7-10`); a real Object Grouping dialog + GROUPDISPLAYMODE landed; BATTMAN multiline editing landed. Dynamic-block authoring still fully absent (`cmd/blocks/bedit.rs:123-126` explicitly refuses to edit dynamic parameters/actions) | [gaps](gaps.md#features) |
| Layouts, viewports, plotting | 9% | 38% | 30–45 | Up (re-audited, partly a stale-claim correction): clipped-viewport/clip-boundary coupling landed (F15); the prior "no MVIEW Restore" claim was **false** — `cmd/layout/viewports.rs` has a fully working named-viewport-configuration Restore with its own prompt machine, corrected here. Plot styles, PUBLISH, polygonal/object/clipped viewports unchanged | [gaps](gaps.md#features) |
| Inquiry and utilities | 4% | 32% | 28–42 | Up slightly (re-audited): COMPARE gained per-layout `allTabs` diffing and COMPAREEXPORT now puts each compared set into its own block (both incremental polish on an existing feature). BOQ gained cost/rate calculation + CSV export, but BOQ has no AutoCAD equivalent so doesn't move AutoCAD-parity depth. DIST/AREA/ID/LIST/MEASUREGEOM/COUNT/QuickCalc/AUDIT unchanged | [gaps](gaps.md#features) |
| Parametric constraints | 2% | 50% | 20–30 | **Confirmed unchanged since 2026-10-24** (re-audited): no real commits touched `crates/constraints` in the window; still all 12 geometric and dimensional constraints, own LM solver, DOF report, CONSTRAINTINFER | [geometry-parity](geometry-parity.md#constraint-solver--50) |
| 3D modelling, visualisation, rendering | 5% | 2% | 300–450 | **Confirmed unchanged since 2026-10-24** (re-audited): direct search found no `*3d*` files anywhere under `crates/`, no UCS/3dorbit/extrude/solid-modelling commands — still the largest single remaining gap | [gaps](gaps.md#features) |
| Collaboration and cloud (Sheet Set Manager, Share, Traces, Markup Import, Activity Insights, DWG Compare) | 2% | 15% | 35–62 | Up (re-audited, a stale-claim correction): the row's own "DWG Compare" sub-feature is **not absent** — COMPARE + COMPAREEXPORT (`cmd/file/compare.rs`, `cmd/file/compare/export.rs`, 552+703 lines) is a complete, working implementation, corrected here. The other 5 of 6 listed sub-features (Sheet Set Manager, Share, Traces, Markup Import, Activity Insights) confirmed fully absent — no `sheetset`/`SheetSet` references anywhere in `crates/` | [gaps](gaps.md#features) |
| **Features dimension** | 100% | **≈ 43%** | **694–1,067** | | |

## By dimension (ready for real work)

| Dimension | Weight | Parity | Remaining (h) | Kind | Doc |
|---|---:|---:|---|---|---|
| Features (table above) | 35% | 43% | 694–1,067 | re-audited | this file |
| UI/UX fidelity (point entry, snaps, tracking, dynamic input, grips, selection, command line, chrome) | 15% | 48% | 90–150 | re-audited: U12 landed real Ribbon chrome (opt-in Home tab, panels for Insert/Annotate/Parametric/View/Manage/Output/Express Tools, contextual tabs + named Workspaces, customizable quick access toolbar) plus a standalone Match Properties Settings dialog and in-place tolerance editing | [ui-parity](ui-parity.md) |
| File formats (DWG/DXF fidelity first) | 20% | 31% | 165–275 | re-audited: mostly unchanged since 2026-10-24; only a stale-row proof (Johab/DOS code pages already round-trip) and an SVG-import refinement landed; no DWG-binary-fidelity work found in the window | [file-format-parity](file-format-parity.md) |
| Stability and correctness | 10% | 42% | 25–40 | re-audited: `.github/workflows/ci.yml` now triggers on `pull_request` (not just push to main) and runs `cargo xtask ci` (fmt, clippy, tests, assets, layers, wasm) via craft-common-actions' gate — closes the prior "no CI runs the Rust tests on PRs" gap; several of the 48 commits since 2026-10-24 are closed-issue proofs (#166, #44, #134) rather than new regressions; S5 (no full-command UI smoke test) still open | [gaps](gaps.md#stability) |
| Performance | 5% | 60% | 20–40 | **confirmed unchanged since 2026-10-24**: no perf-specific commits found; still internal-numbers-only (200k entities fluent); 1M entities and large DWG loads untested; a 10 MB DWG took ~2 min to fail (#57) | [hardware-parity](hardware-parity.md#performance-on-hardware-internal-numbers-only) |
| Ecosystem and automation (AutoLISP/Visual LISP, .NET/ObjectARX, CUI, APPLOAD, Express Tools, App Store) | 5% | 22% | 100–170 | Up (re-audited): docs/gaps.md's own Ecosystem section already documents AutoLISP's core language, `command`, entity/selection-set access, `get...` input, APPLOAD with startup-file loading and a `.lsp` file picker, plus CUI's ALIASEDIT Command Aliases dialog — landed since the 2026-10-24 baseline; still no Visual LISP ActiveX/DCL/reactors, no plug-in API, no App Store | [gaps](gaps.md#ecosystem-and-automation) |
| Hardware | 3% | 45% | 15–25 | **confirmed unchanged since 2026-10-24**: no commits touching GPU/input-device/hardware support found in the window | [hardware-parity](hardware-parity.md) |
| Platforms | 3% | 80% | 10–20 | Up (re-audited): web build gained real localStorage-backed autosave/Drawing Recovery (#F4); File>Open and save-as-download (#55, #133) and the raw-PDF-bytes plot bug (#134) confirmed already fixed; web still has no DWG support and Windows/Linux get little runtime testing | [gaps](gaps.md#platforms) |
| Localization | 3% | 5% | 60–100 | **confirmed unchanged since 2026-10-24**: `crates/ui-egui/src/i18n/mod.rs` still carries a single translated catalog (Ukrainian, 936 rows); other BCP-47 mentions there are locale-tag *parsing* code, not additional translated catalogs; none of AutoCAD for Mac's 7 other languages | [localization-parity](localization-parity.md) |
| AI features (Autodesk Assistant, Smart Blocks, Markup Assist) | 1% | 20% | 40–80 | **confirmed unchanged since 2026-10-24**: no in-app-AI commits found in the window; none of AutoCAD's in-app AI exists; agent control over MCP is a different strength | [gaps](gaps.md#ai-features) |
| **Ready for real work** | 100% | **≈ 41%** (weighted 40.8%) | **≈ 1,220–1,970** | | |

The dimension weights describe what decides whether a professional can switch: features and
file compatibility dominate.

## Remaining effort and how it was calibrated

**Calibration.** The Claude Code transcripts of the session that built CADCraft show its whole
first version (about 51,000 lines, 290 commands, 47% menu breadth) took **about 11.5 Opus 5.5
agent-hours**: 6.9 hours of the lead agent (2026-10-07 02:16–09:08 UTC) plus about 4.6 hours of
eight sub-agents, some in parallel. (Git commit dates were rewritten earlier than that session
and can't be used.) That is about 25 commands per agent-hour **at presence quality**. Depth costs
far more: the next day external contributors found and fixed about 60 correctness bugs, each a
small PR with a regression test, and the audits in this pass found dozens of prompt options that
only print "not available yet". The rates used here are: an option or bug fix 0.3–0.7 h; a
moderate feature (object snap tracking, editable dynamic input, a dialog) 2–5 h; a subsystem
(xrefs, dynamic blocks, AutoLISP, plot styles) 15–100 h; the 3D kernel 300–450 h; then a 1.3×
tail factor for verification against AutoCAD.

**Why the total went up.** The 2026-10-07 roadmap put full parity at ≈ 29% with ≈ 570 h left,
from agent-reported milestone percentages. This pass audited behaviour instead of presence and
found new evidence: object snap tracking, extension, parallel and apparent snaps are toggles;
dynamic input is read-only; saves drop every entity the reader doesn't model; DWG always saves as
R2000; tolerances are absolute; and the old estimate had no rows for localization, ecosystem,
platforms, stability or AI. The weighted figure rose (29% → 41% features) because breadth and
depth both grew since 2026-10-07 and the weights are now written down; the hours rose because
the remaining work is depth and 3D.

**Parallelism.** About 70% of the work splits across 4–6 agents by area (each feature area,
file formats, UI precision, localization per language), so elapsed time is roughly a quarter of
the agent-hours. Needs a human: native-speaker review of every translation; a licensed real-file
DWG/DXF corpus (files from people who own them); black-box checks in AutoCAD of what we save;
3D mouse and tablet hardware; an owner decision on plug-in hosting and on DGN.

## History of this assessment

| Date | Feature breadth | Weighted parity | Remaining | Note |
|---|---|---|---|---|
| 2026-11-23 | unchanged (296/491) | ready ≈ 41% (40.8%), mainstream ≈ 30%, essentials ≈ 48% | 1,220–1,970 h | fresh full weighted re-audit (3 parallel agents, source + `git log --since=2026-10-24`): features dimension 41% → 43% (12 of 14 areas moved or were confirmed unchanged with evidence; 2 stale "doesn't exist" claims corrected — gradients already render properly, MVIEW Restore already works); UI/UX 43% → 48% (Ribbon landed for real); stability 35% → 42% (CI now gates PRs); ecosystem 10% → 22% (AutoLISP/APPLOAD/CUI depth already in gaps.md, not yet reflected here); platforms 75% → 80% (web autosave) |
| 2026-10-10 (merge) | unchanged (233/491 re-scanned) | ready ≈ 38% (37.7%), mainstream ≈ 28%, essentials ≈ 46% | unchanged within rounding | main landed dynamic input pointer boxes (#60), the Ukrainian catalog (#36), WIPEOUT/gradient DXF fixes (#147, #162): UI/UX 40% → 43%, localization 3% → 5% |
| 2026-10-10 (later) | unchanged | ready for real work ≈ 37% (unchanged: already built from the written dimension weights, no hidden judgement); mainstream practitioner ≈ 27%; essentials user ≈ 45% | unchanged | mainstream and essentials numbers added (craftrules standard) |
| 2026-10-10 | 245/491 (50%), engine-only 233/491 | features ≈ 41%, ready for real work ≈ 37% | 1,300–2,100 h | full re-measure in this format (this file) |
| 2026-10-07 | 233/491 (47%) | ≈ 29% overall, 2D ≈ 55% | ≈ 570 h | old ROADMAP: milestone percentages reported by the building agents |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-23 | major | Fresh full weighted re-audit per `plan/STATUS.md`'s queued task: 3 parallel subagents re-read actual source (`crates/engine/src/cmd/*`, `crates/geom`, `crates/constraints`) and `git log --since=2026-10-24` for all 14 feature-area rows and all 9 cross-cutting dimensions, rather than trusting docs/gaps.md prose (caught stale three times before this pass). Features dimension 41% → 43%, ready for real work 38% → 41%, mainstream practitioner 28% → 30%, essentials user 46% → 48%. Two stale "doesn't exist" claims corrected with evidence (gradients were never flat-rendered; MVIEW Restore already works). Feature breadth (296/491) unchanged, already fresh from 2026-11-21. docs/gaps.md itself was not rewritten in this pass — only this file's scores and evidence columns |
| 2026-11-21 | minor | Feature breadth re-measured by actually running `cargo xtask parity` against current `main`: **296/491 = 60%** engine-covered menu items (up from 233/491 = 47% on 2026-10-10). The weighted "ready for real work" (≈38%), mainstream (≈28%) and essentials (≈46%) composites below are **not re-measured** — those need a full manual per-dimension depth audit (several hours), queued for the next session (see `plan/STATUS.md`). Do not read "ready for real work" as still accurate without that re-audit; feature breadth moving 47%→60% strongly suggests it has improved, but by how much is unmeasured |
| 2026-10-22 | minor | Draw row: RECTANG gained a centre construction mode (`Center` keyword, `center: true`) beside its corner mode (#1127); parity percentages and hours not re-measured |
| 2026-10-12 | minor | Hatch row evidence: Hatch and Gradient / Hatch Edit dialogs; percentages and hours not re-measured |
| 2026-10-12 | minor | Blocks row evidence: Block Editor for static blocks (BEDIT, BSAVE, BSAVEAS, BCLOSE); percentages and hours not re-measured |
| 2026-10-11 | minor | Evidence text of the feature-area rows refreshed for merged work (#397, #406, #408, #417, #420, #433, #448, #451, #453, #459, #468, #507, #510, #525, #534, #546, #547, #558, #563, #564, #567, #571); parity percentages and hours not re-measured |
| 2026-10-10 | minor | Merged main: dynamic input pointer boxes (#60) lift UI/UX to 43%, Ukrainian catalog (#36) lifts localization to 5%; ready 37% → 38%, mainstream 27% → 28%, essentials 45% → 46% |
| 2026-10-10 | minor | Readiness-by-audience table: hours to ~95% for full (1,150–1,850 h), mainstream (570–880 h) and essentials (140–225 h); full number confirmed as an additive weighted sum, unchanged |
| 2026-10-10 | minor | Mainstream-practitioner (≈ 27%) and essentials-user (≈ 45%) numbers with written weights, discounts and user evidence |
| 2026-10-10 | minor | Stage alpha → pre-alpha under the core-workflow gate; remaining-to-alpha row |
| 2026-10-10 | major | Created from the old ROADMAP's parity section; full re-measure with weights, two numbers, per-area hours and calibration from the build session's transcripts |
