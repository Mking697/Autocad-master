//! Reading and writing real AutoCAD `.ctb`/`.stb` plot style table files.
//!
//! This is a thin adapter around the `acadrust` crate's own CTB/STB codec (MPL-2.0, used
//! unmodified — see `crates/dwg/src/lib.rs` for the same pattern with DWG). `acadrust` already
//! implements the binary container (a 60-byte header: 48-byte ASCII prefix + adler32 checksum +
//! decompressed length + compressed length, then a zlib-DEFLATE body) and the nested
//! key=value/brace ASCII grammar inside it, grammar it ported from ezdxf's `acadctb.py` (MIT,
//! Manfred Moitzi: <https://github.com/mozman/ezdxf/blob/master/src/ezdxf/addons/acadctb.py>).
//! It also already tolerates hostile input (bad checksums, truncated streams, oversized
//! declared lengths, zlib bombs) by returning `Err` instead of panicking — see its own
//! `lenient_loading_tests` and `inflate_limit_tests` modules.
//!
//! Nothing outside this module touches `acadrust::plot_style` types directly: this file is the
//! only place that converts between them and [`PlotStyleTable`].
//!
//! # Field mapping
//!
//! - `color`, `screening`, `fill_style`/`end_style`/`join_style` map directly (ezdxf's
//!   `END_STYLE_OBJECT = 4`, `JOIN_STYLE_OBJECT = 5`, `FILL_STYLE_OBJECT = 73` and the
//!   `FILL_STYLE_*` constants 64..=72 are reproduced in [`end_to_u8`]/[`join_to_u8`]/
//!   [`fill_to_u8`] and their inverses, straight from ezdxf's source).
//! - `color_policy`'s bit 2 is "print as grayscale" (ezdxf's `GRAYSCALE_ON`); we round-trip it
//!   through [`PlotStyle::grayscale`] and otherwise always write bit 1 ("dithering on", ezdxf's
//!   default `_color_policy`).
//! - `lineweight` is an index into the table's own lineweight list (in mm), not a value itself;
//!   0 and the legacy 255 mean "use the object's lineweight" (ezdxf's `OBJECT_LINEWEIGHT = 0`).
//!   We resolve/build that table on decode/encode and store the resolved millimetre value (as
//!   hundredths of a mm) on [`PlotStyle::lineweight`], matching every other lineweight field in
//!   this codebase.
//! - `linetype` is a CTB-local index (0..=30) into AutoCAD's 31 built-in CTB dash patterns,
//!   *not* a name from the drawing's own linetype table — a completely different namespace from
//!   [`crate::Linetype`]. We have no public-domain source for those 31 patterns' names, so rather
//!   than invent names we store the index verbatim as a decimal string (`"0"`..`"30"`); `None`/31
//!   means "use the object's linetype" (ezdxf's `OBJECT_LINETYPE = 31`). This is lossless for
//!   round-tripping and for telling styles apart, just not yet human-labelled.
//! - A table-level `scale_factor`/`apply_factor` exists in the file format but has no home in
//!   [`PlotStyleTable`] yet; it is read and discarded, and always written as `1.0`/`false`
//!   (identity). `physical_pen_number`/`virtual_pen_number`/`localized_name`/`adaptive_linetype`
//!   are likewise not modelled yet and always round-trip as their defaults.

use crate::{EndStyle, FillStyle, JoinStyle, PlotStyle, PlotStyleKind, PlotStyleTable};
use cadcraft_color::Rgb;

/// Decode a real `.ctb`/`.stb` file's raw bytes into our own [`PlotStyleTable`] model.
///
/// `name` becomes the table's name (normally the file name, e.g. `monochrome.ctb`); it also
/// decides CTB vs STB the same way `acadrust` does (a `.stb` extension, case-insensitively).
/// Never panics: a malformed, truncated or hostile file returns `Err`.
pub fn decode_ctb_stb(bytes: &[u8], name: &str) -> Result<PlotStyleTable, String> {
    let table = acadrust::plot_style::PlotStyleTable::from_bytes(name, bytes)?;
    let kind = if table.is_stb { PlotStyleKind::Named } else { PlotStyleKind::ColorDependent };
    let styles = if table.is_stb {
        // HashMap order is unspecified; sort so the same file always decodes to the same
        // `Vec` order (round-trip and equality tests need that), `sanitized()` then pins
        // "Normal" to the front.
        let mut named: Vec<(&String, &acadrust::plot_style::PlotStyleEntry)> = table.named_entries.iter().collect();
        named.sort_by_key(|(name, _)| name.to_ascii_lowercase());
        named.into_iter().map(|(_, e)| from_entry(e, &table.lineweights)).collect()
    } else {
        (1..=255usize)
            .map(|aci| match table.aci_entries.get(aci) {
                Some(e) => from_entry(e, &table.lineweights),
                None => PlotStyle::object(&format!("Color_{aci}")),
            })
            .collect()
    };
    Ok(PlotStyleTable { name: name.into(), description: table.description.clone(), kind, styles }.sanitized())
}

/// Encode a [`PlotStyleTable`] as real `.ctb` file bytes (zlib-compressed, with the 60-byte
/// header and checksums `acadrust` computes). The table is sanitized first, so this never
/// panics on an out-of-range field.
///
/// Only colour-dependent (`.ctb`) tables are supported. **Named (`.stb`) tables cannot be
/// written here**, even though they can be [`decode_ctb_stb`]'d: `acadrust` 0.6.6's (and
/// still 0.6.8's, checked when this limitation was found) own `PlotStyleTable::to_text()`
/// unconditionally serializes from `aci_entries` and never looks at `named_entries` or
/// `is_stb` at all, regardless of which one holds the real data — so asking it to write an
/// STB table silently produces a CTB-shaped file full of default `Color_N` entries instead
/// (caught by this crate's own round-trip test, not a guess from reading the source). This
/// is a real gap in the vendored crate, not something CADCraft's own code can fix without
/// patching `acadrust` itself, which we don't do (used unmodified only, see `crates/dwg`).
/// Refusing with a clear error here is safer than returning bytes that silently discard the
/// caller's actual style names/values.
pub fn encode_ctb_stb(table: &PlotStyleTable) -> Result<Vec<u8>, String> {
    let table = table.clone().sanitized();
    if !table.is_color_dependent() {
        return Err(format!(
            "{}: writing a named (.stb) plot style table isn't supported yet — the vendored \
             acadrust crate's own writer only serializes colour-dependent (.ctb) tables \
             (confirmed in acadrust 0.6.6 and 0.6.8); reading an existing .stb file still works",
            table.name
        ));
    }
    let mut out = acadrust::plot_style::PlotStyleTable::identity(table.name.clone());
    out.is_stb = false;
    out.description = table.description.clone();
    for (i, style) in table.styles.iter().enumerate() {
        let aci = i + 1; // ACI 1..=255; `aci_entries` is indexed by ACI (0 unused).
        let entry = to_entry(style, &mut out.lineweights);
        if let Some(slot) = out.aci_entries.get_mut(aci) {
            *slot = entry;
        }
    }
    out.to_bytes()
}

/// `PlotStyleEntry` -> our own [`PlotStyle`], resolving its lineweight index against `lineweights`.
fn from_entry(e: &acadrust::plot_style::PlotStyleEntry, lineweights: &[f32]) -> PlotStyle {
    let lineweight_mm = if matches!(e.lineweight, 0 | 255) { None } else { lineweights.get(e.lineweight as usize).copied() };
    PlotStyle {
        name: e.name.clone(),
        description: e.description.clone(),
        color: e.color.map(|[r, g, b]| Rgb(r, g, b)),
        grayscale: e.color_policy & 2 != 0,
        screening: e.screening.min(100),
        linetype: linetype_name_from_index(e.linetype),
        lineweight: lineweight_mm.map(mm_to_hundredths),
        end: end_from_u8(e.end_style),
        join: join_from_u8(e.join_style),
        fill: fill_from_u8(e.fill_style),
    }
}

/// Our own [`PlotStyle`] -> `PlotStyleEntry`, finding or appending its lineweight in `lineweights`
/// (capped at the 256 entries a `u8` index can reach; once full, further new lineweights fall
/// back to "object lineweight" rather than reusing an unrelated slot).
fn to_entry(s: &PlotStyle, lineweights: &mut Vec<f32>) -> acadrust::plot_style::PlotStyleEntry {
    acadrust::plot_style::PlotStyleEntry {
        name: s.name.clone(),
        localized_name: s.name.clone(),
        description: s.description.clone(),
        color: s.color.map(|c| [c.0, c.1, c.2]),
        lineweight: lineweight_index(s.lineweight, lineweights),
        screening: s.screening.min(100),
        color_policy: if s.grayscale { 0b11 } else { 0b01 },
        physical_pen_number: 0,
        virtual_pen_number: 0,
        linepattern_size: 0.5,
        linetype: linetype_index_from_name(s.linetype.as_deref()),
        adaptive_linetype: true,
        fill_style: fill_to_u8(s.fill),
        end_style: end_to_u8(s.end),
        join_style: join_to_u8(s.join),
    }
}

fn mm_to_hundredths(mm: f32) -> u16 {
    // Float-to-int casts in Rust saturate (no UB, no panic) since 1.45, so a non-finite or
    // out-of-range `mm` just clamps instead of wrapping or trapping.
    (mm.max(0.0) * 100.0).round() as u16
}

/// Find `mm` (hundredths of a mm) in `lineweights`, appending it if it is new. `None` is
/// "object lineweight" (index 0).
fn lineweight_index(mm_hundredths: Option<u16>, lineweights: &mut Vec<f32>) -> u8 {
    let Some(h) = mm_hundredths else { return 0 };
    let mm = f32::from(h) / 100.0;
    if let Some(pos) = lineweights.iter().position(|w| (w - mm).abs() < 0.001) {
        return pos.min(255) as u8;
    }
    if lineweights.len() < 256 {
        lineweights.push(mm);
        return (lineweights.len() - 1).min(255) as u8;
    }
    0 // the 256-entry table is full; fall back to "object lineweight" rather than reuse a slot.
}

fn linetype_name_from_index(idx: u8) -> Option<String> {
    if idx <= 30 { Some(idx.to_string()) } else { None }
}

fn linetype_index_from_name(name: Option<&str>) -> u8 {
    match name.and_then(|n| n.trim().parse::<u32>().ok()) {
        Some(n) if n <= 30 => n as u8,
        _ => 31, // ezdxf's OBJECT_LINETYPE: "use the object's own linetype".
    }
}

fn end_from_u8(v: u8) -> EndStyle {
    match v {
        0 => EndStyle::Butt,
        1 => EndStyle::Square,
        2 => EndStyle::Round,
        3 => EndStyle::Diamond,
        _ => EndStyle::Object,
    }
}
fn end_to_u8(e: EndStyle) -> u8 {
    match e {
        EndStyle::Butt => 0,
        EndStyle::Square => 1,
        EndStyle::Round => 2,
        EndStyle::Diamond => 3,
        EndStyle::Object => 4,
    }
}

fn join_from_u8(v: u8) -> JoinStyle {
    match v {
        0 => JoinStyle::Miter,
        1 => JoinStyle::Bevel,
        2 => JoinStyle::Round,
        3 => JoinStyle::Diamond,
        _ => JoinStyle::Object,
    }
}
fn join_to_u8(j: JoinStyle) -> u8 {
    match j {
        JoinStyle::Miter => 0,
        JoinStyle::Bevel => 1,
        JoinStyle::Round => 2,
        JoinStyle::Diamond => 3,
        JoinStyle::Object => 5,
    }
}

fn fill_from_u8(v: u8) -> FillStyle {
    match v {
        64 => FillStyle::Solid,
        65 => FillStyle::Checkerboard,
        66 => FillStyle::Crosshatch,
        67 => FillStyle::Diamonds,
        68 => FillStyle::HorizontalBars,
        69 => FillStyle::SlantLeft,
        70 => FillStyle::SlantRight,
        71 => FillStyle::SquareDots,
        72 => FillStyle::VerticalBars,
        _ => FillStyle::Object,
    }
}
fn fill_to_u8(f: FillStyle) -> u8 {
    match f {
        FillStyle::Solid => 64,
        FillStyle::Checkerboard => 65,
        FillStyle::Crosshatch => 66,
        FillStyle::Diamonds => 67,
        FillStyle::HorizontalBars => 68,
        FillStyle::SlantLeft => 69,
        FillStyle::SlantRight => 70,
        FillStyle::SquareDots => 71,
        FillStyle::VerticalBars => 72,
        FillStyle::Object => 73,
    }
}

/// True when `bytes` look like a real binary `.ctb`/`.stb` file (the fixed 48-byte ASCII
/// prefix `acadctb`-family writers use), as opposed to CADCraft's own JSON table format.
pub fn looks_like_binary_ctb(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PIAFILEVERSION_2.0,")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn color_dependent_table() -> PlotStyleTable {
        let mut t = PlotStyleTable::color_dependent("roundtrip.ctb", "a synthetic CTB", |aci| {
            Some(match aci {
                1 => PlotStyle { color: Some(Rgb(255, 0, 0)), screening: 80, lineweight: Some(50), end: EndStyle::Round, ..PlotStyle::default() },
                2 => PlotStyle { grayscale: true, fill: FillStyle::Crosshatch, join: JoinStyle::Bevel, ..PlotStyle::default() },
                3 => PlotStyle { lineweight: Some(50), ..PlotStyle::default() }, // shares color_dependent_table's #1 weight
                9 => PlotStyle { linetype: Some("5".into()), ..PlotStyle::default() },
                _ => return None,
            })
        });
        t.name = "roundtrip.ctb".into();
        t
    }

    fn named_table() -> PlotStyleTable {
        // Already alphabetical (besides "Normal") so decode's forced sort matches this order.
        PlotStyleTable::named(
            "roundtrip.stb",
            "a synthetic STB",
            vec![
                PlotStyle { name: "Black".into(), color: Some(Rgb(0, 0, 0)), ..PlotStyle::default() },
                PlotStyle { name: "Screened 50%".into(), screening: 50, ..PlotStyle::default() },
                PlotStyle { name: "Thick".into(), lineweight: Some(70), end: EndStyle::Square, fill: FillStyle::Solid, ..PlotStyle::default() },
            ],
        )
    }

    #[test]
    fn ctb_round_trips_through_our_own_encoder() {
        let original = color_dependent_table();
        let bytes = encode_ctb_stb(&original).expect("encode");
        assert!(looks_like_binary_ctb(&bytes), "real header bytes");
        let decoded = decode_ctb_stb(&bytes, "roundtrip.ctb").expect("decode");
        assert_eq!(decoded, original, "a CTB must decode(encode(x)) == x");
        // Two styles sharing the same lineweight share the same table slot.
        assert_eq!(decoded.by_color(1).unwrap().lineweight, Some(50));
        assert_eq!(decoded.by_color(3).unwrap().lineweight, Some(50));
        assert_eq!(decoded.by_color(9).unwrap().linetype.as_deref(), Some("5"));
    }

    #[test]
    fn stb_encoding_is_refused_not_silently_wrong() {
        // `acadrust`'s own writer can't serialize a named table (see `encode_ctb_stb`'s doc
        // comment) — refusing cleanly here is the honest behaviour, not a bytes-shaped lie.
        let err = encode_ctb_stb(&named_table()).expect_err("STB encoding must be refused");
        assert!(err.contains("roundtrip.stb"), "names the table: {err}");
        assert!(err.contains("named"), "explains why: {err}");
    }

    #[test]
    fn linetype_index_is_not_confused_with_a_linetype_name() {
        assert_eq!(linetype_name_from_index(0), Some("0".into()));
        assert_eq!(linetype_name_from_index(30), Some("30".into()));
        assert_eq!(linetype_name_from_index(31), None, "31 means object linetype");
        assert_eq!(linetype_name_from_index(200), None, "out-of-range values are treated as object");
        assert_eq!(linetype_index_from_name(Some("12")), 12);
        assert_eq!(linetype_index_from_name(Some("not a number")), 31);
        assert_eq!(linetype_index_from_name(None), 31);
    }

    /// A malformed, truncated, or outright hostile `.ctb`/`.stb` must return `Err`, never panic.
    #[test]
    fn hostile_ctb_bytes_never_panic() {
        let good = encode_ctb_stb(&color_dependent_table()).expect("encode");
        let cases: Vec<Vec<u8>> = vec![
            Vec::new(),
            vec![0u8; 10],
            good[..good.len() / 2].to_vec(),
            good[..59].to_vec(), // one byte short of the 60-byte header
            {
                // A header that claims a vastly larger compressed length than the file holds.
                let mut b = good.clone();
                b[56..60].copy_from_slice(&u32::MAX.to_le_bytes());
                b
            },
            {
                // A header that claims a vastly larger decompressed length than is real.
                let mut b = good.clone();
                b[52..56].copy_from_slice(&u32::MAX.to_le_bytes());
                b
            },
            b"not a ctb file at all, just some bytes".to_vec(),
            [b"PIAFILEVERSION_2.0,CTBVER1,compress\r\npmzlibcodec".as_slice(), &[0u8; 100]].concat(),
        ];
        for bytes in cases {
            // Must not panic; whether it loads or errors is secondary to that.
            let _ = decode_ctb_stb(&bytes, "hostile.ctb");
        }
    }

    #[test]
    fn empty_and_malformed_tables_sanitize_rather_than_panic() {
        // A table with zero styles still encodes/decodes to something sane (sanitized() pads
        // a CTB to 255 styles).
        let empty_ctb = PlotStyleTable { name: "empty.ctb".into(), kind: PlotStyleKind::ColorDependent, ..PlotStyleTable::default() };
        let bytes = encode_ctb_stb(&empty_ctb).expect("encode");
        let decoded = decode_ctb_stb(&bytes, "empty.ctb").expect("decode");
        assert_eq!(decoded.styles.len(), crate::CTB_STYLES);

        // An empty named (STB) table is still refused cleanly, not silently mis-encoded as a
        // CTB (see `encode_ctb_stb`'s doc comment) — sanitizing first doesn't change its kind.
        let empty_stb = PlotStyleTable { name: "empty.stb".into(), kind: PlotStyleKind::Named, ..PlotStyleTable::default() };
        encode_ctb_stb(&empty_stb).expect_err("STB encoding must still be refused when empty");
    }

    #[test]
    fn detects_binary_vs_our_own_json_format() {
        assert!(looks_like_binary_ctb(b"PIAFILEVERSION_2.0,CTBVER1,compress\r\n"));
        assert!(!looks_like_binary_ctb(br#"{"cadcraftPlotStyleTable": 1}"#));
        assert!(!looks_like_binary_ctb(b""));
    }
}
