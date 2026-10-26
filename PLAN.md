# Apna "AutoCAD" — Master Plan

Base repo (installed): **CADCraft** — https://github.com/storytold/cadcraft
Location on this machine: `D:\autocad master\cadcraft`
GitHub stars: 6,879 | Forks: 2,996 | License: MIT OR Apache-2.0 | Language: Rust

Kyun yeh repo chuna:
- Yeh ek "clean-room reimplementation" hai — matlab ispe kaam karne wali team ne khud AutoCAD
  ko observe karke, usi command line, usi workflow, usi shortcuts (L=line, C=circle, OFFSET,
  TRIM, FILLET, DIMSTYLE...) ko dobara Rust mein likha hai. Koi aur open-source repo itna
  close match nahi hai "exact AutoCAD jaisa" requirement ke liye.
- Already 2D (drafting, dimensions, hatch, blocks, layouts/plotting) + parametric constraints
  ka kaam shuru hai. 3D abhi 0% hai (roadmap mein saaf likha hai) — isliye 3D humein khud
  add karna padega (ya FreeCAD se geometry kernel idea le kar).
  (Reason jaanna zaroori: FreeCAD jyada stars/mature hai (34.8k) 3D ke liye, lekin woh
  "AutoCAD jaisa" nahi — apna alag parametric 3D workflow hai. CADCraft 2D UX mein hi
  "bilkul AutoCAD jaisa" promise nibhata hai, isliye isko base rakha.)
- File format: DXF (R12-2018 ASCII+binary) read/write native. DWG (R13-2018) read/write via
  open-source `acadrust` library — matlab real AutoCAD .dwg files khul/save ho sakte hain.
- Agent-friendly: MCP server + JSON control channel + CLI hai — matlab Claude Code jaise AI
  agents isko seedha drive/test kar sakte hain, development bahut fast hoga.
- Dev environment is machine par already ready kar diya gaya hai: Rust toolchain
  (rustup + cargo, stable-x86_64-pc-windows-msvc) install ho gaya, repo clone ho gaya,
  pehla `cargo build --release` background mein chal raha hai.

Current honest parity status (project ke apne ROADMAP.md se, 2026-10 tak):
- Feature breadth (AutoCAD menu items jinke live command hai): ~50% (245/491)
- "Ready for real work" / alpha bar: ~38%
- Beta tak pahunchne mein ~600-930 engineering hours baaki (unki estimate)
- 3D modeling: 0% (sirf roadmap mein planned)

-----------------------------------------------------------------------
## GOAL — "Bilkul AutoCAD jaisa, different nahi"

Non-negotiable acceptance bar for every feature we add/fix:
1. Command name, keyboard shortcut aur command-line prompt sequence EXACT AutoCAD jaisa
   hona chahiye (e.g. typing `L` behaves exactly like AutoCAD's LINE).
2. UI layout (ribbon/toolbar positions, Properties palette, Layers palette, status bar
   icons, command line position at bottom) AutoCAD ke jaisa hi dikhna chahiye.
3. File compatibility: humara banaya DWG/DXF real AutoCAD mein bina data-loss ke khulna
   chahiye, aur AutoCAD ki file humare app mein sahi khulni chahiye.
4. Koi bhi naya UI pattern jo AutoCAD mein nahi hai, use sirf "Extra/Pro" tab mein daalo —
   core drafting experience se alag na mix karo.

-----------------------------------------------------------------------
## PHASE 0 — Environment & Baseline (Day 0-1)
- [x] Rust toolchain install (rustup, stable-msvc) — DONE
- [x] Repo clone — DONE (`D:\autocad master\cadcraft`)
- [ ] `cargo build --release -p cadcraft` success confirm karna (background mein chal raha)
- [ ] App run karke sample drawing dekhna: `cargo run --release -p cadcraft -- --sample`
- [ ] AutoCAD ki asli copy (trial/licensed) side-by-side rakh ke UI/behaviour compare karna
      (repo ke apne AGENTS.md rules ke mutabik: AutoCAD ko sirf observe karo, kabhi uske
      files/code/assets copy mat karo — clean-room rule, legal safety ke liye zaroori hai)
- [ ] `docs/target-app-parity.md`, `docs/gaps.md`, `docs/ui-parity.md` padhna — exact pata
      chalega kya missing hai AutoCAD jaisa banne ke liye

## PHASE 1 — 2D Drafting ko 100% AutoCAD-parity tak le jaana (Months 1-4)
Priority order (unki apni gaps.md follow karo, highest-impact rows pehle):
1. Baaki DRAW/MODIFY commands jo missing hain (gaps.md se list)
2. Command-line behaviour ko pixel/keystroke-level match karna (AutoComplete, dynamic
   input, grips behaviour)
3. Ribbon/toolbar/menu layout ko AutoCAD 2024/2025 jaisa arrange karna
4. Hatch patterns, linetypes, dimension styles ko AutoCAD ke standard set jaisa banana
   (apna khud ka original asset banana hai — Autodesk ke patterns copy NAHI karne — legal)
5. DWG round-trip testing: real AutoCAD files open/save, compare, no data loss

## PHASE 2 — 3D Modeling Engine Add Karna (Months 4-9)
CADCraft mein abhi 3D 0% hai, yeh sabse bada naya kaam hai:
1. Geometry kernel decide karna — options:
   a. Khud ka B-Rep/solid kernel Rust mein likhna (max control, max effort)
   b. Kisi licence-compatible Rust-native geometry crate se integrate karna
   (NOTE: repo ke apne rules GPL/LGPL libraries jaise OpenCascade/LibreDWG ko link karne
   se mana karte hain clean-room/licence wajah se — isliye OpenCascade directly nahi le
   sakte agar MIT/Apache license maintain karni hai. Yeh decision pehle lena hoga.)
2. Basic 3D primitives: EXTRUDE, REVOLVE, UNION/SUBTRACT/INTERSECT (solid booleans)
3. 3D viewport: orbit, perspective, visual styles (wireframe/shaded/realistic) — AutoCAD
   jaise ViewCube, UCS icon already partially 2D mein hai, isko 3D-aware banana
3. Import/export STEP/IGES compatibility (baad mein, lower priority — AutoCAD khud bhi
   isme weak hai, so 2D+basic 3D solids priority)

## PHASE 3 — Automatic BOQ (Bill of Quantities) Module (Months 6-10, parallel se shuru ho sakta hai)
Yeh feature AutoCAD mein khud built-in nahi hai (AutoCAD mein log Dynamic Blocks/Data
Extraction use karte hain) — hum isko "extra value-add" ke roop mein banayenge, lekin
core drawing experience ko disturb nahi karega (Phase goal #4 rule).
1. Drawing entity scanner likhna (Rust, engine crate ke upar se):
   - Walls (polylines with thickness/hatch) → length × height × thickness = volume
   - Rooms/closed areas → area (sq.ft/sq.m) nikal ke flooring/plaster quantity
   - Doors/windows (blocks) → count karke schedule banana
   - Columns/beams (if any 3D solids) → volume for concrete/steel estimate
2. Rate database (material/labour rates) — user-editable table (JSON/SQLite)
3. BOQ = entity quantities × rate database → Excel/PDF export
   (CADCraft already PDF export + python/JSON automation layer hai — isi engine.execute
   API ke upar BOQ command likhna sabse clean tareeka hai: naya command jaise "BOQ" jo
   AutoCAD command jaisa hi UI mein integrate ho jaye, command line se type karke chale)
4. AutoCAD jaisa hi ek naya ribbon tab "BOQ" add karna (extra, par look same rakhna)

## PHASE 4 — Polish, Branding, Packaging (Months 10-12)
1. App icon/branding apna (CADCraft already apna original branding deta hai legal reasons se)
2. Installer banana: Windows (.msi/.exe), cross-platform already supported (Linux/Mac/Web)
3. Performance testing on large real-world drawings (apartment plans, mechanical parts)
4. User testing — non-technical users se workflow verify karwana ("kya yeh AutoCAD jaisa feel
   hota hai?")

-----------------------------------------------------------------------
## RISKS / HONEST CONSTRAINTS
- Yeh 10-12 month+ ka serious engineering project hai, solo nahi — kam se kam 2-4 Rust
  developers ki zaroorat padegi continuous kaam ke liye.
- "Exact AutoCAD jaisa, bilkul different nahi" — 100% pixel-perfect kabhi nahi hoga bina
  Autodesk ke source code ke (jo legally possible nahi hai). CADCraft approach — observe
  + clean-room rebuild — sabse legal-safe aur practical raasta hai 95%+ behavioural match
  ke liye.
- 3D part sabse zyada risk/time leta hai kyunki CADCraft mein abhi 0% hai — yahi sabse
  bada unknown hai.
- BOQ automation ki accuracy drawing quality par depend karti hai (agar layers/blocks
  consistently naam rakhe gaye hain drawing mein, tabhi scanner reliably kaam karega) —
  isliye drawing conventions/standards define karna Phase 3 se pehle zaroori hai.

-----------------------------------------------------------------------
## IMMEDIATE NEXT STEPS (abhi)
1. Build complete hone do, phir `cargo run --release -p cadcraft -- --sample` chalao,
   screenshot le kar dekho kaisa dikhta hai.
2. `docs/gaps.md` aur `docs/target-app-parity.md` padh ke top-5 missing features decide karo.
3. Confirm karo: team/budget/timeline kya hai realistically, taaki Phase 1-4 ko sahi
   milestones mein baanta ja sake.
