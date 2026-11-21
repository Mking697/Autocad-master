//! Text decoding for DXF files.
//!
//! R2007 (AC1021) and later DXF files are UTF-8. Older ones store text in the code page named by
//! the `$DWGCODEPAGE` header variable (`ANSI_1252`, `ANSI_932`, `DOS850`, …); characters outside
//! it are written as `\U+XXXX` (any character) or `\M+nXXXX` (a double-byte character of an East
//! Asian code page) escapes, which [`crate::parse`] decodes after the bytes.
//!
//! The Windows, ISO and East Asian code pages are decoded with `encoding_rs`; DOS 437, 850 and
//! 852 with the tables below (the upper halves of the IBM PC code pages; the lower halves are
//! ASCII). Decoding never fails: a byte sequence that is invalid in the code page (a truncated
//! double-byte character, an unassigned byte) becomes U+FFFD and the bytes after it decode
//! normally, so line ends and the group structure survive.

use std::borrow::Cow;

use encoding_rs::Encoding;

/// How the bytes of a code page become text.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Decoder {
    Utf8,
    Encoding(&'static Encoding),
    /// Single-byte DOS code page: ASCII, then these 128 characters for bytes 0x80–0xFF.
    Table(&'static [char; 128]),
    /// DOS864 differs below 0x80 too: byte 0x25 is the Arabic percent sign.
    Dos864,
    /// Korean Johab (`ANSI_1361`, Windows code page 1361): ASCII below 0x80, and a pair of
    /// bytes 0x84–0xD3/0x41–0xFE decoded algorithmically into a modern Hangul syllable (see
    /// [`decode_johab`]). Hanja, symbols and lone (non-syllabic) jamo, which Johab encodes in
    /// other byte ranges, are not covered and decode as U+FFFD, like any other unassigned byte.
    Johab,
}

/// The decoder for a `$DWGCODEPAGE` value (case-insensitive), or `None` when CADCraft doesn't
/// know the code page.
pub(crate) fn decoder(code_page: &str) -> Option<Decoder> {
    use encoding_rs as e;
    let name = code_page.trim().to_ascii_uppercase().replace(['-', ' '], "_");
    let enc = match name.as_str() {
        "UTF8" | "UTF_8" | "UNICODE" => return Some(Decoder::Utf8),
        "ASCII" | "ANSI_1252" | "ISO8859_1" | "ISO_8859_1" => e::WINDOWS_1252,
        "ANSI_874" => e::WINDOWS_874,
        "ANSI_932" | "DOS932" | "SHIFT_JIS" => e::SHIFT_JIS,
        "ANSI_936" | "GB2312" | "GBK" => e::GBK,
        "ANSI_949" | "KSC5601" | "KOREAN" => e::EUC_KR,
        "ANSI_950" | "BIG5" => e::BIG5,
        "ANSI_1250" => e::WINDOWS_1250,
        "ANSI_1251" => e::WINDOWS_1251,
        "ANSI_1253" => e::WINDOWS_1253,
        "ANSI_1254" | "ISO8859_9" | "ISO_8859_9" => e::WINDOWS_1254,
        "ANSI_1255" => e::WINDOWS_1255,
        "ANSI_1256" => e::WINDOWS_1256,
        "ANSI_1257" => e::WINDOWS_1257,
        "ANSI_1258" => e::WINDOWS_1258,
        "ISO8859_2" | "ISO_8859_2" => e::ISO_8859_2,
        "ISO8859_3" | "ISO_8859_3" => e::ISO_8859_3,
        "ISO8859_4" | "ISO_8859_4" => e::ISO_8859_4,
        "ISO8859_5" | "ISO_8859_5" => e::ISO_8859_5,
        "ISO8859_6" | "ISO_8859_6" => e::ISO_8859_6,
        "ISO8859_7" | "ISO_8859_7" => e::ISO_8859_7,
        "ISO8859_8" | "ISO_8859_8" => e::ISO_8859_8,
        "DOS866" => e::IBM866,
        "MACINTOSH" | "MAC_ROMAN" => e::MACINTOSH,
        "DOS437" => return Some(Decoder::Table(&DOS437)),
        "DOS850" => return Some(Decoder::Table(&DOS850)),
        "DOS852" => return Some(Decoder::Table(&DOS852)),
        "DOS855" => return Some(Decoder::Table(&DOS855)),
        "DOS857" => return Some(Decoder::Table(&DOS857)),
        "DOS860" => return Some(Decoder::Table(&DOS860)),
        "DOS861" => return Some(Decoder::Table(&DOS861)),
        "DOS862" => return Some(Decoder::Table(&DOS862)),
        "DOS863" => return Some(Decoder::Table(&DOS863)),
        "DOS864" => return Some(Decoder::Dos864),
        "DOS865" => return Some(Decoder::Table(&DOS865)),
        "DOS869" => return Some(Decoder::Table(&DOS869)),
        "ANSI_1361" | "JOHAB" => return Some(Decoder::Johab),
        _ => return None,
    };
    Some(Decoder::Encoding(enc))
}

/// Decode `bytes` from the code page `code_page` names (a `$DWGCODEPAGE` value such as
/// `ANSI_1251`), or `None` when CADCraft doesn't know that code page. Invalid sequences become
/// U+FFFD; escapes (`\U+XXXX`, `\M+nXXXX`) are left as they are.
pub fn decode_code_page(bytes: &[u8], code_page: &str) -> Option<String> {
    decoder(code_page).map(|d| decode(bytes, d).into_owned())
}

/// Decode bytes with a decoder. UTF-8 that isn't valid falls back to Windows-1252.
pub(crate) fn decode(bytes: &[u8], d: Decoder) -> Cow<'_, str> {
    match d {
        Decoder::Utf8 => match std::str::from_utf8(bytes) {
            Ok(s) => Cow::Borrowed(s),
            Err(_) => encoding_rs::WINDOWS_1252.decode_without_bom_handling(bytes).0,
        },
        Decoder::Encoding(e) => e.decode_without_bom_handling(bytes).0,
        Decoder::Table(t) => match std::str::from_utf8(bytes) {
            Ok(s) if s.is_ascii() => Cow::Borrowed(s),
            _ => Cow::Owned(
                bytes
                    .iter()
                    .map(|&b| if b.is_ascii() { char::from(b) } else { t.get(usize::from(b & 0x7F)).copied().unwrap_or('\u{FFFD}') })
                    .collect(),
            ),
        },
        Decoder::Dos864 => Cow::Owned(
            bytes
                .iter()
                .map(|&b| match b {
                    b'%' => '\u{066A}',
                    0..=0x7F => char::from(b),
                    _ => DOS864.get(usize::from(b & 0x7F)).copied().unwrap_or('\u{FFFD}'),
                })
                .collect(),
        ),
        Decoder::Johab => match std::str::from_utf8(bytes) {
            Ok(s) if s.is_ascii() => Cow::Borrowed(s),
            _ => Cow::Owned(decode_johab(bytes)),
        },
    }
}

/// Decode Johab (`ANSI_1361`) bytes: ASCII below 0x80, and lead/trail byte pairs in the modern
/// Hangul syllable range (lead 0x84–0xD3) decoded by the KS X 1001:1992 Annex 3 formula (each
/// syllable packs a 5-bit initial consonant, 5-bit vowel and 5-bit final consonant index into
/// the two bytes). Anything else — hanja, symbols and lone jamo, which Johab places in other
/// byte ranges — becomes U+FFFD, like any other unassigned byte; a truncated trailing lead byte
/// does too, and the next byte is still tried, so this never panics on hostile input.
fn decode_johab(bytes: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0usize;
    while let Some(&b1) = bytes.get(i) {
        if b1 < 0x80 {
            out.push(char::from(b1));
            i += 1;
            continue;
        }
        let pair = (0x84..=0xD3).contains(&b1).then(|| bytes.get(i + 1)).flatten();
        match pair.and_then(|&b2| johab_syllable(b1, b2)) {
            Some(c) => {
                out.push(c);
                i += 2;
            }
            None => {
                out.push('\u{FFFD}');
                i += 1;
            }
        }
    }
    out
}

/// Decode one Johab lead/trail byte pair into a modern Hangul syllable, or `None` when the
/// bits don't name a valid initial consonant, vowel and final consonant (lone jamo, reserved
/// codes, or a byte pair outside the Hangul syllable block).
fn johab_syllable(b1: u8, b2: u8) -> Option<char> {
    let value = (u16::from(b1) << 8) | u16::from(b2);
    let lead = (value >> 10) & 0x1F;
    let vowel = (value >> 5) & 0x1F;
    let tail = value & 0x1F;
    if !(2..=20).contains(&lead) {
        return None;
    }
    let lead0 = u32::from(lead - 2);
    let vowel0 = u32::try_from(*JOHAB_VOWEL.get(usize::from(vowel))?).ok()?;
    let tail0 = u32::try_from(*JOHAB_TAIL.get(usize::from(tail))?).ok()?;
    let code_point = 0xAC00u32.checked_add(21 * 28 * lead0)?.checked_add(28 * vowel0)?.checked_add(tail0)?;
    char::from_u32(code_point)
}

/// Index of each Johab 5-bit vowel code (0–31) among the 21 modern vowels, or -1 when the code
/// is a filler or unused (KS X 1001:1992 Annex 3 Table 2).
const JOHAB_VOWEL: [i8; 32] = [-1, -1, -1, 0, 1, 2, 3, 4, -1, -1, 5, 6, 7, 8, 9, 10, -1, -1, 11, 12, 13, 14, 15, 16, -1, -1, 17, 18, 19, 20, -1, -1];

/// Index of each Johab 5-bit final-consonant code (0–31) among the 28 final consonants
/// (including "no final", code 1), or -1 when the code is unused.
const JOHAB_TAIL: [i8; 32] = [-1, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, -1, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, -1, -1];

/// The `$DWGCODEPAGE` value of an ASCII DXF file: the line two lines after a `$DWGCODEPAGE`
/// line (the group code `3` between them), looked for in the HEADER section only (the scan stops
/// at the first `ENDSEC`). Works on the raw bytes, before they are decoded.
fn ascii_code_page(bytes: &[u8]) -> Option<&[u8]> {
    let mut lines = bytes.split(|&b| b == b'\n').map(<[u8]>::trim_ascii);
    while let Some(line) = lines.next() {
        if line == b"ENDSEC" {
            return None;
        }
        if line.eq_ignore_ascii_case(b"$DWGCODEPAGE") {
            return match (lines.next(), lines.next()) {
                (Some(b"3"), Some(value)) => Some(value),
                _ => None,
            };
        }
    }
    None
}

/// Decode a whole ASCII DXF file. Valid UTF-8 is read as UTF-8: R2007+ files, pure-ASCII files,
/// and older files from writers that ignored the code page. Anything else is decoded from the
/// file's `$DWGCODEPAGE`, or from Windows-1252 (the usual `ANSI_1252`) when the header has none
/// or names a code page CADCraft doesn't know, which is logged as a warning.
pub(crate) fn decode_ascii_file(bytes: &[u8]) -> Cow<'_, str> {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return Cow::Borrowed(s);
    }
    let d = match ascii_code_page(bytes) {
        Some(name) => {
            let name = String::from_utf8_lossy(name);
            decoder(&name).unwrap_or_else(|| {
                log::warn!("DXF: unknown code page `{name}`; reading the text as ANSI_1252");
                Decoder::Utf8
            })
        }
        None => Decoder::Utf8,
    };
    decode(bytes, d)
}

/// The text state of a binary DXF file, learnt from its header as the tags go by: `$ACADVER`
/// (R2007+ strings are UTF-8) and `$DWGCODEPAGE` (the code page of older files). Both come
/// first in the HEADER section and are ASCII, so every later string is decoded right.
#[derive(Default)]
pub(crate) struct BinaryText {
    /// The header variable whose value comes next (after a group 9).
    var: Option<HeaderVar>,
    utf8: bool,
    code_page: Option<Decoder>,
}

#[derive(Clone, Copy, PartialEq)]
enum HeaderVar {
    AcadVer,
    CodePage,
}

impl BinaryText {
    /// Decode one string value of group `code` and update the state from it. Strings of R2007+
    /// files, and of files without a code page, are UTF-8 when valid (else Windows-1252); older
    /// files' strings are in their code page.
    pub(crate) fn string(&mut self, code: i32, raw: &[u8]) -> String {
        let d = if self.utf8 { Decoder::Utf8 } else { self.code_page.unwrap_or(Decoder::Utf8) };
        let s = decode(raw, d).into_owned();
        match (code, self.var.take()) {
            (9, _) => {
                self.var = match s.trim() {
                    "$ACADVER" => Some(HeaderVar::AcadVer),
                    "$DWGCODEPAGE" => Some(HeaderVar::CodePage),
                    _ => None,
                }
            }
            (1, Some(HeaderVar::AcadVer)) => self.utf8 = is_utf8_version(&s),
            (3, Some(HeaderVar::CodePage)) => {
                self.code_page = decoder(&s);
                if self.code_page.is_none() {
                    log::warn!("DXF: unknown code page `{}`; reading the text as ANSI_1252", s.trim());
                }
            }
            _ => {}
        }
        s
    }
}

/// True for an `$ACADVER` of R2007 (AC1021) or later, whose text is UTF-8.
fn is_utf8_version(acadver: &str) -> bool {
    acadver.trim().strip_prefix("AC").and_then(|n| n.parse::<u32>().ok()).is_some_and(|n| n >= 1021)
}

/// The double-byte code page of a `\M+n` escape: 1 Japanese (Shift-JIS), 2 Traditional Chinese
/// (Big5), 3 Korean (Wansung), 5 Simplified Chinese (GB 2312). 4 (Korean Johab) is not
/// supported, so its escapes stay literal.
pub(crate) fn mif_encoding(n: char) -> Option<&'static Encoding> {
    match n {
        '1' => Some(encoding_rs::SHIFT_JIS),
        '2' => Some(encoding_rs::BIG5),
        '3' => Some(encoding_rs::EUC_KR),
        '5' => Some(encoding_rs::GBK),
        _ => None,
    }
}

// Generated from the IBM PC code page definitions (bytes 0x80–0xFF in order).
const DOS437: [char; 128] = [
    '\u{00C7}', '\u{00FC}', '\u{00E9}', '\u{00E2}', '\u{00E4}', '\u{00E0}', '\u{00E5}', '\u{00E7}', '\u{00EA}', '\u{00EB}', '\u{00E8}', '\u{00EF}',
    '\u{00EE}', '\u{00EC}', '\u{00C4}', '\u{00C5}', '\u{00C9}', '\u{00E6}', '\u{00C6}', '\u{00F4}', '\u{00F6}', '\u{00F2}', '\u{00FB}', '\u{00F9}',
    '\u{00FF}', '\u{00D6}', '\u{00DC}', '\u{00A2}', '\u{00A3}', '\u{00A5}', '\u{20A7}', '\u{0192}', '\u{00E1}', '\u{00ED}', '\u{00F3}', '\u{00FA}',
    '\u{00F1}', '\u{00D1}', '\u{00AA}', '\u{00BA}', '\u{00BF}', '\u{2310}', '\u{00AC}', '\u{00BD}', '\u{00BC}', '\u{00A1}', '\u{00AB}', '\u{00BB}',
    '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}', '\u{2561}', '\u{2562}', '\u{2556}', '\u{2555}', '\u{2563}', '\u{2551}', '\u{2557}',
    '\u{255D}', '\u{255C}', '\u{255B}', '\u{2510}', '\u{2514}', '\u{2534}', '\u{252C}', '\u{251C}', '\u{2500}', '\u{253C}', '\u{255E}', '\u{255F}',
    '\u{255A}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}', '\u{2550}', '\u{256C}', '\u{2567}', '\u{2568}', '\u{2564}', '\u{2565}', '\u{2559}',
    '\u{2558}', '\u{2552}', '\u{2553}', '\u{256B}', '\u{256A}', '\u{2518}', '\u{250C}', '\u{2588}', '\u{2584}', '\u{258C}', '\u{2590}', '\u{2580}',
    '\u{03B1}', '\u{00DF}', '\u{0393}', '\u{03C0}', '\u{03A3}', '\u{03C3}', '\u{00B5}', '\u{03C4}', '\u{03A6}', '\u{0398}', '\u{03A9}', '\u{03B4}',
    '\u{221E}', '\u{03C6}', '\u{03B5}', '\u{2229}', '\u{2261}', '\u{00B1}', '\u{2265}', '\u{2264}', '\u{2320}', '\u{2321}', '\u{00F7}', '\u{2248}',
    '\u{00B0}', '\u{2219}', '\u{00B7}', '\u{221A}', '\u{207F}', '\u{00B2}', '\u{25A0}', '\u{00A0}',
];
const DOS850: [char; 128] = [
    '\u{00C7}', '\u{00FC}', '\u{00E9}', '\u{00E2}', '\u{00E4}', '\u{00E0}', '\u{00E5}', '\u{00E7}', '\u{00EA}', '\u{00EB}', '\u{00E8}', '\u{00EF}',
    '\u{00EE}', '\u{00EC}', '\u{00C4}', '\u{00C5}', '\u{00C9}', '\u{00E6}', '\u{00C6}', '\u{00F4}', '\u{00F6}', '\u{00F2}', '\u{00FB}', '\u{00F9}',
    '\u{00FF}', '\u{00D6}', '\u{00DC}', '\u{00F8}', '\u{00A3}', '\u{00D8}', '\u{00D7}', '\u{0192}', '\u{00E1}', '\u{00ED}', '\u{00F3}', '\u{00FA}',
    '\u{00F1}', '\u{00D1}', '\u{00AA}', '\u{00BA}', '\u{00BF}', '\u{00AE}', '\u{00AC}', '\u{00BD}', '\u{00BC}', '\u{00A1}', '\u{00AB}', '\u{00BB}',
    '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}', '\u{00C1}', '\u{00C2}', '\u{00C0}', '\u{00A9}', '\u{2563}', '\u{2551}', '\u{2557}',
    '\u{255D}', '\u{00A2}', '\u{00A5}', '\u{2510}', '\u{2514}', '\u{2534}', '\u{252C}', '\u{251C}', '\u{2500}', '\u{253C}', '\u{00E3}', '\u{00C3}',
    '\u{255A}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}', '\u{2550}', '\u{256C}', '\u{00A4}', '\u{00F0}', '\u{00D0}', '\u{00CA}', '\u{00CB}',
    '\u{00C8}', '\u{0131}', '\u{00CD}', '\u{00CE}', '\u{00CF}', '\u{2518}', '\u{250C}', '\u{2588}', '\u{2584}', '\u{00A6}', '\u{00CC}', '\u{2580}',
    '\u{00D3}', '\u{00DF}', '\u{00D4}', '\u{00D2}', '\u{00F5}', '\u{00D5}', '\u{00B5}', '\u{00FE}', '\u{00DE}', '\u{00DA}', '\u{00DB}', '\u{00D9}',
    '\u{00FD}', '\u{00DD}', '\u{00AF}', '\u{00B4}', '\u{00AD}', '\u{00B1}', '\u{2017}', '\u{00BE}', '\u{00B6}', '\u{00A7}', '\u{00F7}', '\u{00B8}',
    '\u{00B0}', '\u{00A8}', '\u{00B7}', '\u{00B9}', '\u{00B3}', '\u{00B2}', '\u{25A0}', '\u{00A0}',
];
const DOS852: [char; 128] = [
    '\u{00C7}', '\u{00FC}', '\u{00E9}', '\u{00E2}', '\u{00E4}', '\u{016F}', '\u{0107}', '\u{00E7}', '\u{0142}', '\u{00EB}', '\u{0150}', '\u{0151}',
    '\u{00EE}', '\u{0179}', '\u{00C4}', '\u{0106}', '\u{00C9}', '\u{0139}', '\u{013A}', '\u{00F4}', '\u{00F6}', '\u{013D}', '\u{013E}', '\u{015A}',
    '\u{015B}', '\u{00D6}', '\u{00DC}', '\u{0164}', '\u{0165}', '\u{0141}', '\u{00D7}', '\u{010D}', '\u{00E1}', '\u{00ED}', '\u{00F3}', '\u{00FA}',
    '\u{0104}', '\u{0105}', '\u{017D}', '\u{017E}', '\u{0118}', '\u{0119}', '\u{00AC}', '\u{017A}', '\u{010C}', '\u{015F}', '\u{00AB}', '\u{00BB}',
    '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}', '\u{00C1}', '\u{00C2}', '\u{011A}', '\u{015E}', '\u{2563}', '\u{2551}', '\u{2557}',
    '\u{255D}', '\u{017B}', '\u{017C}', '\u{2510}', '\u{2514}', '\u{2534}', '\u{252C}', '\u{251C}', '\u{2500}', '\u{253C}', '\u{0102}', '\u{0103}',
    '\u{255A}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}', '\u{2550}', '\u{256C}', '\u{00A4}', '\u{0111}', '\u{0110}', '\u{010E}', '\u{00CB}',
    '\u{010F}', '\u{0147}', '\u{00CD}', '\u{00CE}', '\u{011B}', '\u{2518}', '\u{250C}', '\u{2588}', '\u{2584}', '\u{0162}', '\u{016E}', '\u{2580}',
    '\u{00D3}', '\u{00DF}', '\u{00D4}', '\u{0143}', '\u{0144}', '\u{0148}', '\u{0160}', '\u{0161}', '\u{0154}', '\u{00DA}', '\u{0155}', '\u{0170}',
    '\u{00FD}', '\u{00DD}', '\u{0163}', '\u{00B4}', '\u{00AD}', '\u{02DD}', '\u{02DB}', '\u{02C7}', '\u{02D8}', '\u{00A7}', '\u{00F7}', '\u{00B8}',
    '\u{00B0}', '\u{00A8}', '\u{02D9}', '\u{0171}', '\u{0158}', '\u{0159}', '\u{25A0}', '\u{00A0}',
];

// Generated from the IBM PC code page definitions published by the Unicode Consortium
// (https://unicode.org/Public/MAPPINGS/VENDORS/MICSFT/PC/, bytes 0x80–0xFF in order). Positions
// the vendor table itself leaves undefined decode as U+FFFD, like any other unassigned byte.
const DOS855: [char; 128] = [
    '\u{0452}', '\u{0402}', '\u{0453}', '\u{0403}', '\u{0451}', '\u{0401}', '\u{0454}', '\u{0404}', '\u{0455}', '\u{0405}', '\u{0456}', '\u{0406}',
    '\u{0457}', '\u{0407}', '\u{0458}', '\u{0408}', '\u{0459}', '\u{0409}', '\u{045A}', '\u{040A}', '\u{045B}', '\u{040B}', '\u{045C}', '\u{040C}',
    '\u{045E}', '\u{040E}', '\u{045F}', '\u{040F}', '\u{044E}', '\u{042E}', '\u{044A}', '\u{042A}', '\u{0430}', '\u{0410}', '\u{0431}', '\u{0411}',
    '\u{0446}', '\u{0426}', '\u{0434}', '\u{0414}', '\u{0435}', '\u{0415}', '\u{0444}', '\u{0424}', '\u{0433}', '\u{0413}', '\u{00AB}', '\u{00BB}',
    '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}', '\u{0445}', '\u{0425}', '\u{0438}', '\u{0418}', '\u{2563}', '\u{2551}', '\u{2557}',
    '\u{255D}', '\u{0439}', '\u{0419}', '\u{2510}', '\u{2514}', '\u{2534}', '\u{252C}', '\u{251C}', '\u{2500}', '\u{253C}', '\u{043A}', '\u{041A}',
    '\u{255A}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}', '\u{2550}', '\u{256C}', '\u{00A4}', '\u{043B}', '\u{041B}', '\u{043C}', '\u{041C}',
    '\u{043D}', '\u{041D}', '\u{043E}', '\u{041E}', '\u{043F}', '\u{2518}', '\u{250C}', '\u{2588}', '\u{2584}', '\u{041F}', '\u{044F}', '\u{2580}',
    '\u{042F}', '\u{0440}', '\u{0420}', '\u{0441}', '\u{0421}', '\u{0442}', '\u{0422}', '\u{0443}', '\u{0423}', '\u{0436}', '\u{0416}', '\u{0432}',
    '\u{0412}', '\u{044C}', '\u{042C}', '\u{2116}', '\u{00AD}', '\u{044B}', '\u{042B}', '\u{0437}', '\u{0417}', '\u{0448}', '\u{0428}', '\u{044D}',
    '\u{042D}', '\u{0449}', '\u{0429}', '\u{0447}', '\u{0427}', '\u{00A7}', '\u{25A0}', '\u{00A0}',
];

const DOS857: [char; 128] = [
    '\u{00C7}', '\u{00FC}', '\u{00E9}', '\u{00E2}', '\u{00E4}', '\u{00E0}', '\u{00E5}', '\u{00E7}', '\u{00EA}', '\u{00EB}', '\u{00E8}', '\u{00EF}',
    '\u{00EE}', '\u{0131}', '\u{00C4}', '\u{00C5}', '\u{00C9}', '\u{00E6}', '\u{00C6}', '\u{00F4}', '\u{00F6}', '\u{00F2}', '\u{00FB}', '\u{00F9}',
    '\u{0130}', '\u{00D6}', '\u{00DC}', '\u{00F8}', '\u{00A3}', '\u{00D8}', '\u{015E}', '\u{015F}', '\u{00E1}', '\u{00ED}', '\u{00F3}', '\u{00FA}',
    '\u{00F1}', '\u{00D1}', '\u{011E}', '\u{011F}', '\u{00BF}', '\u{00AE}', '\u{00AC}', '\u{00BD}', '\u{00BC}', '\u{00A1}', '\u{00AB}', '\u{00BB}',
    '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}', '\u{00C1}', '\u{00C2}', '\u{00C0}', '\u{00A9}', '\u{2563}', '\u{2551}', '\u{2557}',
    '\u{255D}', '\u{00A2}', '\u{00A5}', '\u{2510}', '\u{2514}', '\u{2534}', '\u{252C}', '\u{251C}', '\u{2500}', '\u{253C}', '\u{00E3}', '\u{00C3}',
    '\u{255A}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}', '\u{2550}', '\u{256C}', '\u{00A4}', '\u{00BA}', '\u{00AA}', '\u{00CA}', '\u{00CB}',
    '\u{00C8}', '\u{FFFD}', '\u{00CD}', '\u{00CE}', '\u{00CF}', '\u{2518}', '\u{250C}', '\u{2588}', '\u{2584}', '\u{00A6}', '\u{00CC}', '\u{2580}',
    '\u{00D3}', '\u{00DF}', '\u{00D4}', '\u{00D2}', '\u{00F5}', '\u{00D5}', '\u{00B5}', '\u{FFFD}', '\u{00D7}', '\u{00DA}', '\u{00DB}', '\u{00D9}',
    '\u{00EC}', '\u{00FF}', '\u{00AF}', '\u{00B4}', '\u{00AD}', '\u{00B1}', '\u{FFFD}', '\u{00BE}', '\u{00B6}', '\u{00A7}', '\u{00F7}', '\u{00B8}',
    '\u{00B0}', '\u{00A8}', '\u{00B7}', '\u{00B9}', '\u{00B3}', '\u{00B2}', '\u{25A0}', '\u{00A0}',
];

const DOS860: [char; 128] = [
    '\u{00C7}', '\u{00FC}', '\u{00E9}', '\u{00E2}', '\u{00E3}', '\u{00E0}', '\u{00C1}', '\u{00E7}', '\u{00EA}', '\u{00CA}', '\u{00E8}', '\u{00CD}',
    '\u{00D4}', '\u{00EC}', '\u{00C3}', '\u{00C2}', '\u{00C9}', '\u{00C0}', '\u{00C8}', '\u{00F4}', '\u{00F5}', '\u{00F2}', '\u{00DA}', '\u{00F9}',
    '\u{00CC}', '\u{00D5}', '\u{00DC}', '\u{00A2}', '\u{00A3}', '\u{00D9}', '\u{20A7}', '\u{00D3}', '\u{00E1}', '\u{00ED}', '\u{00F3}', '\u{00FA}',
    '\u{00F1}', '\u{00D1}', '\u{00AA}', '\u{00BA}', '\u{00BF}', '\u{00D2}', '\u{00AC}', '\u{00BD}', '\u{00BC}', '\u{00A1}', '\u{00AB}', '\u{00BB}',
    '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}', '\u{2561}', '\u{2562}', '\u{2556}', '\u{2555}', '\u{2563}', '\u{2551}', '\u{2557}',
    '\u{255D}', '\u{255C}', '\u{255B}', '\u{2510}', '\u{2514}', '\u{2534}', '\u{252C}', '\u{251C}', '\u{2500}', '\u{253C}', '\u{255E}', '\u{255F}',
    '\u{255A}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}', '\u{2550}', '\u{256C}', '\u{2567}', '\u{2568}', '\u{2564}', '\u{2565}', '\u{2559}',
    '\u{2558}', '\u{2552}', '\u{2553}', '\u{256B}', '\u{256A}', '\u{2518}', '\u{250C}', '\u{2588}', '\u{2584}', '\u{258C}', '\u{2590}', '\u{2580}',
    '\u{03B1}', '\u{00DF}', '\u{0393}', '\u{03C0}', '\u{03A3}', '\u{03C3}', '\u{00B5}', '\u{03C4}', '\u{03A6}', '\u{0398}', '\u{03A9}', '\u{03B4}',
    '\u{221E}', '\u{03C6}', '\u{03B5}', '\u{2229}', '\u{2261}', '\u{00B1}', '\u{2265}', '\u{2264}', '\u{2320}', '\u{2321}', '\u{00F7}', '\u{2248}',
    '\u{00B0}', '\u{2219}', '\u{00B7}', '\u{221A}', '\u{207F}', '\u{00B2}', '\u{25A0}', '\u{00A0}',
];

const DOS861: [char; 128] = [
    '\u{00C7}', '\u{00FC}', '\u{00E9}', '\u{00E2}', '\u{00E4}', '\u{00E0}', '\u{00E5}', '\u{00E7}', '\u{00EA}', '\u{00EB}', '\u{00E8}', '\u{00D0}',
    '\u{00F0}', '\u{00DE}', '\u{00C4}', '\u{00C5}', '\u{00C9}', '\u{00E6}', '\u{00C6}', '\u{00F4}', '\u{00F6}', '\u{00FE}', '\u{00FB}', '\u{00DD}',
    '\u{00FD}', '\u{00D6}', '\u{00DC}', '\u{00F8}', '\u{00A3}', '\u{00D8}', '\u{20A7}', '\u{0192}', '\u{00E1}', '\u{00ED}', '\u{00F3}', '\u{00FA}',
    '\u{00C1}', '\u{00CD}', '\u{00D3}', '\u{00DA}', '\u{00BF}', '\u{2310}', '\u{00AC}', '\u{00BD}', '\u{00BC}', '\u{00A1}', '\u{00AB}', '\u{00BB}',
    '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}', '\u{2561}', '\u{2562}', '\u{2556}', '\u{2555}', '\u{2563}', '\u{2551}', '\u{2557}',
    '\u{255D}', '\u{255C}', '\u{255B}', '\u{2510}', '\u{2514}', '\u{2534}', '\u{252C}', '\u{251C}', '\u{2500}', '\u{253C}', '\u{255E}', '\u{255F}',
    '\u{255A}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}', '\u{2550}', '\u{256C}', '\u{2567}', '\u{2568}', '\u{2564}', '\u{2565}', '\u{2559}',
    '\u{2558}', '\u{2552}', '\u{2553}', '\u{256B}', '\u{256A}', '\u{2518}', '\u{250C}', '\u{2588}', '\u{2584}', '\u{258C}', '\u{2590}', '\u{2580}',
    '\u{03B1}', '\u{00DF}', '\u{0393}', '\u{03C0}', '\u{03A3}', '\u{03C3}', '\u{00B5}', '\u{03C4}', '\u{03A6}', '\u{0398}', '\u{03A9}', '\u{03B4}',
    '\u{221E}', '\u{03C6}', '\u{03B5}', '\u{2229}', '\u{2261}', '\u{00B1}', '\u{2265}', '\u{2264}', '\u{2320}', '\u{2321}', '\u{00F7}', '\u{2248}',
    '\u{00B0}', '\u{2219}', '\u{00B7}', '\u{221A}', '\u{207F}', '\u{00B2}', '\u{25A0}', '\u{00A0}',
];

const DOS862: [char; 128] = [
    '\u{05D0}', '\u{05D1}', '\u{05D2}', '\u{05D3}', '\u{05D4}', '\u{05D5}', '\u{05D6}', '\u{05D7}', '\u{05D8}', '\u{05D9}', '\u{05DA}', '\u{05DB}',
    '\u{05DC}', '\u{05DD}', '\u{05DE}', '\u{05DF}', '\u{05E0}', '\u{05E1}', '\u{05E2}', '\u{05E3}', '\u{05E4}', '\u{05E5}', '\u{05E6}', '\u{05E7}',
    '\u{05E8}', '\u{05E9}', '\u{05EA}', '\u{00A2}', '\u{00A3}', '\u{00A5}', '\u{20A7}', '\u{0192}', '\u{00E1}', '\u{00ED}', '\u{00F3}', '\u{00FA}',
    '\u{00F1}', '\u{00D1}', '\u{00AA}', '\u{00BA}', '\u{00BF}', '\u{2310}', '\u{00AC}', '\u{00BD}', '\u{00BC}', '\u{00A1}', '\u{00AB}', '\u{00BB}',
    '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}', '\u{2561}', '\u{2562}', '\u{2556}', '\u{2555}', '\u{2563}', '\u{2551}', '\u{2557}',
    '\u{255D}', '\u{255C}', '\u{255B}', '\u{2510}', '\u{2514}', '\u{2534}', '\u{252C}', '\u{251C}', '\u{2500}', '\u{253C}', '\u{255E}', '\u{255F}',
    '\u{255A}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}', '\u{2550}', '\u{256C}', '\u{2567}', '\u{2568}', '\u{2564}', '\u{2565}', '\u{2559}',
    '\u{2558}', '\u{2552}', '\u{2553}', '\u{256B}', '\u{256A}', '\u{2518}', '\u{250C}', '\u{2588}', '\u{2584}', '\u{258C}', '\u{2590}', '\u{2580}',
    '\u{03B1}', '\u{00DF}', '\u{0393}', '\u{03C0}', '\u{03A3}', '\u{03C3}', '\u{00B5}', '\u{03C4}', '\u{03A6}', '\u{0398}', '\u{03A9}', '\u{03B4}',
    '\u{221E}', '\u{03C6}', '\u{03B5}', '\u{2229}', '\u{2261}', '\u{00B1}', '\u{2265}', '\u{2264}', '\u{2320}', '\u{2321}', '\u{00F7}', '\u{2248}',
    '\u{00B0}', '\u{2219}', '\u{00B7}', '\u{221A}', '\u{207F}', '\u{00B2}', '\u{25A0}', '\u{00A0}',
];

const DOS863: [char; 128] = [
    '\u{00C7}', '\u{00FC}', '\u{00E9}', '\u{00E2}', '\u{00C2}', '\u{00E0}', '\u{00B6}', '\u{00E7}', '\u{00EA}', '\u{00EB}', '\u{00E8}', '\u{00EF}',
    '\u{00EE}', '\u{2017}', '\u{00C0}', '\u{00A7}', '\u{00C9}', '\u{00C8}', '\u{00CA}', '\u{00F4}', '\u{00CB}', '\u{00CF}', '\u{00FB}', '\u{00F9}',
    '\u{00A4}', '\u{00D4}', '\u{00DC}', '\u{00A2}', '\u{00A3}', '\u{00D9}', '\u{00DB}', '\u{0192}', '\u{00A6}', '\u{00B4}', '\u{00F3}', '\u{00FA}',
    '\u{00A8}', '\u{00B8}', '\u{00B3}', '\u{00AF}', '\u{00CE}', '\u{2310}', '\u{00AC}', '\u{00BD}', '\u{00BC}', '\u{00BE}', '\u{00AB}', '\u{00BB}',
    '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}', '\u{2561}', '\u{2562}', '\u{2556}', '\u{2555}', '\u{2563}', '\u{2551}', '\u{2557}',
    '\u{255D}', '\u{255C}', '\u{255B}', '\u{2510}', '\u{2514}', '\u{2534}', '\u{252C}', '\u{251C}', '\u{2500}', '\u{253C}', '\u{255E}', '\u{255F}',
    '\u{255A}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}', '\u{2550}', '\u{256C}', '\u{2567}', '\u{2568}', '\u{2564}', '\u{2565}', '\u{2559}',
    '\u{2558}', '\u{2552}', '\u{2553}', '\u{256B}', '\u{256A}', '\u{2518}', '\u{250C}', '\u{2588}', '\u{2584}', '\u{258C}', '\u{2590}', '\u{2580}',
    '\u{03B1}', '\u{00DF}', '\u{0393}', '\u{03C0}', '\u{03A3}', '\u{03C3}', '\u{00B5}', '\u{03C4}', '\u{03A6}', '\u{0398}', '\u{03A9}', '\u{03B4}',
    '\u{221E}', '\u{03C6}', '\u{03B5}', '\u{2229}', '\u{2261}', '\u{00B1}', '\u{2265}', '\u{2264}', '\u{2320}', '\u{2321}', '\u{00F7}', '\u{2248}',
    '\u{00B0}', '\u{2219}', '\u{00B7}', '\u{221A}', '\u{207F}', '\u{00B2}', '\u{25A0}', '\u{00A0}',
];

const DOS864: [char; 128] = [
    '\u{00B0}', '\u{00B7}', '\u{2219}', '\u{221A}', '\u{2592}', '\u{2500}', '\u{2502}', '\u{253C}', '\u{2524}', '\u{252C}', '\u{251C}', '\u{2534}',
    '\u{2510}', '\u{250C}', '\u{2514}', '\u{2518}', '\u{03B2}', '\u{221E}', '\u{03C6}', '\u{00B1}', '\u{00BD}', '\u{00BC}', '\u{2248}', '\u{00AB}',
    '\u{00BB}', '\u{FEF7}', '\u{FEF8}', '\u{FFFD}', '\u{FFFD}', '\u{FEFB}', '\u{FEFC}', '\u{FFFD}', '\u{00A0}', '\u{00AD}', '\u{FE82}', '\u{00A3}',
    '\u{00A4}', '\u{FE84}', '\u{FFFD}', '\u{FFFD}', '\u{FE8E}', '\u{FE8F}', '\u{FE95}', '\u{FE99}', '\u{060C}', '\u{FE9D}', '\u{FEA1}', '\u{FEA5}',
    '\u{0660}', '\u{0661}', '\u{0662}', '\u{0663}', '\u{0664}', '\u{0665}', '\u{0666}', '\u{0667}', '\u{0668}', '\u{0669}', '\u{FED1}', '\u{061B}',
    '\u{FEB1}', '\u{FEB5}', '\u{FEB9}', '\u{061F}', '\u{00A2}', '\u{FE80}', '\u{FE81}', '\u{FE83}', '\u{FE85}', '\u{FECA}', '\u{FE8B}', '\u{FE8D}',
    '\u{FE91}', '\u{FE93}', '\u{FE97}', '\u{FE9B}', '\u{FE9F}', '\u{FEA3}', '\u{FEA7}', '\u{FEA9}', '\u{FEAB}', '\u{FEAD}', '\u{FEAF}', '\u{FEB3}',
    '\u{FEB7}', '\u{FEBB}', '\u{FEBF}', '\u{FEC1}', '\u{FEC5}', '\u{FECB}', '\u{FECF}', '\u{00A6}', '\u{00AC}', '\u{00F7}', '\u{00D7}', '\u{FEC9}',
    '\u{0640}', '\u{FED3}', '\u{FED7}', '\u{FEDB}', '\u{FEDF}', '\u{FEE3}', '\u{FEE7}', '\u{FEEB}', '\u{FEED}', '\u{FEEF}', '\u{FEF3}', '\u{FEBD}',
    '\u{FECC}', '\u{FECE}', '\u{FECD}', '\u{FEE1}', '\u{FE7D}', '\u{0651}', '\u{FEE5}', '\u{FEE9}', '\u{FEEC}', '\u{FEF0}', '\u{FEF2}', '\u{FED0}',
    '\u{FED5}', '\u{FEF5}', '\u{FEF6}', '\u{FEDD}', '\u{FED9}', '\u{FEF1}', '\u{25A0}', '\u{FFFD}',
];

const DOS865: [char; 128] = [
    '\u{00C7}', '\u{00FC}', '\u{00E9}', '\u{00E2}', '\u{00E4}', '\u{00E0}', '\u{00E5}', '\u{00E7}', '\u{00EA}', '\u{00EB}', '\u{00E8}', '\u{00EF}',
    '\u{00EE}', '\u{00EC}', '\u{00C4}', '\u{00C5}', '\u{00C9}', '\u{00E6}', '\u{00C6}', '\u{00F4}', '\u{00F6}', '\u{00F2}', '\u{00FB}', '\u{00F9}',
    '\u{00FF}', '\u{00D6}', '\u{00DC}', '\u{00F8}', '\u{00A3}', '\u{00D8}', '\u{20A7}', '\u{0192}', '\u{00E1}', '\u{00ED}', '\u{00F3}', '\u{00FA}',
    '\u{00F1}', '\u{00D1}', '\u{00AA}', '\u{00BA}', '\u{00BF}', '\u{2310}', '\u{00AC}', '\u{00BD}', '\u{00BC}', '\u{00A1}', '\u{00AB}', '\u{00A4}',
    '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}', '\u{2561}', '\u{2562}', '\u{2556}', '\u{2555}', '\u{2563}', '\u{2551}', '\u{2557}',
    '\u{255D}', '\u{255C}', '\u{255B}', '\u{2510}', '\u{2514}', '\u{2534}', '\u{252C}', '\u{251C}', '\u{2500}', '\u{253C}', '\u{255E}', '\u{255F}',
    '\u{255A}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}', '\u{2550}', '\u{256C}', '\u{2567}', '\u{2568}', '\u{2564}', '\u{2565}', '\u{2559}',
    '\u{2558}', '\u{2552}', '\u{2553}', '\u{256B}', '\u{256A}', '\u{2518}', '\u{250C}', '\u{2588}', '\u{2584}', '\u{258C}', '\u{2590}', '\u{2580}',
    '\u{03B1}', '\u{00DF}', '\u{0393}', '\u{03C0}', '\u{03A3}', '\u{03C3}', '\u{00B5}', '\u{03C4}', '\u{03A6}', '\u{0398}', '\u{03A9}', '\u{03B4}',
    '\u{221E}', '\u{03C6}', '\u{03B5}', '\u{2229}', '\u{2261}', '\u{00B1}', '\u{2265}', '\u{2264}', '\u{2320}', '\u{2321}', '\u{00F7}', '\u{2248}',
    '\u{00B0}', '\u{2219}', '\u{00B7}', '\u{221A}', '\u{207F}', '\u{00B2}', '\u{25A0}', '\u{00A0}',
];

const DOS869: [char; 128] = [
    '\u{FFFD}', '\u{FFFD}', '\u{FFFD}', '\u{FFFD}', '\u{FFFD}', '\u{FFFD}', '\u{0386}', '\u{FFFD}', '\u{00B7}', '\u{00AC}', '\u{00A6}', '\u{2018}',
    '\u{2019}', '\u{0388}', '\u{2015}', '\u{0389}', '\u{038A}', '\u{03AA}', '\u{038C}', '\u{FFFD}', '\u{FFFD}', '\u{038E}', '\u{03AB}', '\u{00A9}',
    '\u{038F}', '\u{00B2}', '\u{00B3}', '\u{03AC}', '\u{00A3}', '\u{03AD}', '\u{03AE}', '\u{03AF}', '\u{03CA}', '\u{0390}', '\u{03CC}', '\u{03CD}',
    '\u{0391}', '\u{0392}', '\u{0393}', '\u{0394}', '\u{0395}', '\u{0396}', '\u{0397}', '\u{00BD}', '\u{0398}', '\u{0399}', '\u{00AB}', '\u{00BB}',
    '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}', '\u{039A}', '\u{039B}', '\u{039C}', '\u{039D}', '\u{2563}', '\u{2551}', '\u{2557}',
    '\u{255D}', '\u{039E}', '\u{039F}', '\u{2510}', '\u{2514}', '\u{2534}', '\u{252C}', '\u{251C}', '\u{2500}', '\u{253C}', '\u{03A0}', '\u{03A1}',
    '\u{255A}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}', '\u{2550}', '\u{256C}', '\u{03A3}', '\u{03A4}', '\u{03A5}', '\u{03A6}', '\u{03A7}',
    '\u{03A8}', '\u{03A9}', '\u{03B1}', '\u{03B2}', '\u{03B3}', '\u{2518}', '\u{250C}', '\u{2588}', '\u{2584}', '\u{03B4}', '\u{03B5}', '\u{2580}',
    '\u{03B6}', '\u{03B7}', '\u{03B8}', '\u{03B9}', '\u{03BA}', '\u{03BB}', '\u{03BC}', '\u{03BD}', '\u{03BE}', '\u{03BF}', '\u{03C0}', '\u{03C1}',
    '\u{03C3}', '\u{03C2}', '\u{03C4}', '\u{0384}', '\u{00AD}', '\u{00B1}', '\u{03C5}', '\u{03C6}', '\u{03C7}', '\u{00A7}', '\u{03C8}', '\u{0385}',
    '\u{00B0}', '\u{00A8}', '\u{03C9}', '\u{03CB}', '\u{03B0}', '\u{03CE}', '\u{25A0}', '\u{00A0}',
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dos864_maps_the_low_half_arabic_percent_sign() {
        assert_eq!(decode_code_page(b"25%", "DOS864").as_deref(), Some("25\u{066A}"));
        assert_eq!(decode_code_page(b"25%", "DOS865").as_deref(), Some("25%"));
    }

    #[test]
    fn code_page_names() {
        let d = |bytes: &[u8], cp: &str| decode_code_page(bytes, cp);
        assert_eq!(d(b"\xCF\xF0\xE8\xE2\xE5\xF2", "ANSI_1251").as_deref(), Some("Привет"));
        assert_eq!(d(b"\xCF\xF0\xE8\xE2\xE5\xF2", " ansi_1251 ").as_deref(), Some("Привет"));
        assert_eq!(d(b"\x90\x7D\x96\xCA", "ANSI_932").as_deref(), Some("図面"));
        assert_eq!(d(b"\xCD\xBC\xB2\xE3", "GB2312").as_deref(), Some("图层"));
        assert_eq!(d(b"\xB9\xCF\xBC\x68", "BIG5").as_deref(), Some("圖層"));
        assert_eq!(d(b"\xB5\xB5\xB8\xE9", "ANSI_949").as_deref(), Some("도면"));
        assert_eq!(d(b"\xD3\xF7\xDD\xE4\xE9\xEF", "ANSI_1253").as_deref(), Some("Σχέδιο"));
        assert_eq!(d(b"\xC0\xD2\xBE", "ANSI_874").as_deref(), Some("ภาพ"));
        assert_eq!(d(b"\x80", "ANSI_1252").as_deref(), Some("€"));
        // DOS 437 and 850 agree on letters but differ on 0x9B (¢ / ø).
        assert_eq!(d(b"Gr\x94\xE1e \x9B", "DOS437").as_deref(), Some("Größe ¢"));
        assert_eq!(d(b"Gr\x94\xE1e \x9B", "dos850").as_deref(), Some("Größe ø"));
        assert_eq!(d(b"\xBD\xA2\x88w", "DOS852").as_deref(), Some("Żółw"));
        assert_eq!(d(b"\x91\xAB\xAE\xA9", "DOS866").as_deref(), Some("Слой"));
        // Each of the DOS code pages added for the Johab/DOS gap closure, decoded with a real
        // non-ASCII word of its own script.
        assert_eq!(d(b"\xDD\xE1\xB7\xEB\xA8\xE5", "DOS855").as_deref(), Some("Привет"));
        assert_eq!(d(b"\x47\x81\x6E\x65\x9F", "DOS857").as_deref(), Some("Güneş"));
        assert_eq!(d(b"\x87\x94\x65\x73", "DOS860").as_deref(), Some("ções"));
        assert_eq!(d(b"\x95\x98\x8C\x69\x6E\x67", "DOS861").as_deref(), Some("þýðing"));
        assert_eq!(d(b"\x99\x8C\x85\x8D", "DOS862").as_deref(), Some("שלום"));
        assert_eq!(d(b"\x88\x74\x72\x65", "DOS863").as_deref(), Some("être"));
        // Arabic-Indic digits, which Johab's cousin DOS864 maps without needing shaping forms.
        assert_eq!(d(b"\xB0\xB1\xB2\xB3\xB4", "DOS864").as_deref(), Some("٠١٢٣٤"));
        assert_eq!(d(b"\x62\x6C\x86\x62\x91\x72", "DOS865").as_deref(), Some("blåbær"));
        assert_eq!(d(b"\xDE\xE5\xEA\x9F\xDD\xD6", "DOS869").as_deref(), Some("ελπίδα"));
        assert_eq!(d("é".as_bytes(), "UTF8").as_deref(), Some("é"));
        // ANSI_1361 (Johab): modern Hangul syllables decode via the KS X 1001 Annex 3 formula.
        assert_eq!(d(b"\x88\x61\x88\x62\x88\x63", "ANSI_1361").as_deref(), Some("가각갂"));
        assert_eq!(d(b"\xD0\x65\x8B\x69\x90\x69", "johab").as_deref(), Some("한글날"));
        assert_eq!(d(b"x", "EBCDIC"), None);
        assert_eq!(d(b"x", ""), None);
    }

    #[test]
    fn invalid_sequences_become_replacement_characters() {
        // A Shift-JIS lead byte cut off by the line end keeps the line end.
        assert_eq!(decode_code_page(b"\x90\n1\n", "ANSI_932").as_deref(), Some("\u{FFFD}\n1\n"));
        assert_eq!(decode_code_page(b"\x90", "ANSI_932").as_deref(), Some("\u{FFFD}"));
        assert_eq!(decode_code_page(b"\xFF\xFF", "ANSI_936").as_deref(), Some("\u{FFFD}\u{FFFD}"));
    }

    /// Regression for the Johab decoder: truncated lead bytes, reserved jamo codes, lone jamo
    /// and byte pairs outside the Hangul syllable block must become U+FFFD and keep decoding
    /// the rest of the buffer, never panic, for every possible lead/trail byte pair.
    #[test]
    fn johab_hostile_bytes_never_panic() {
        // A Hangul lead byte with nothing after it (file truncated mid-character).
        assert_eq!(decode_code_page(b"AB\x8C", "ANSI_1361").as_deref(), Some("AB\u{FFFD}"));
        // A lead byte followed by a trail byte that names no valid vowel/final (reserved code).
        assert_eq!(decode_code_page(b"\x84\x01", "ANSI_1361").as_deref(), Some("\u{FFFD}\u{0001}"));
        // Lead bytes outside 0x84-0xD3 (hanja/symbol rows, or plain unassigned) are not decoded.
        assert_eq!(decode_code_page(b"\xD8\x31\xFF", "ANSI_1361").as_deref(), Some("\u{FFFD}1\u{FFFD}"));
        // Every lead byte (0x80-0xFF) paired with every trail byte (0x00-0xFF): must terminate
        // and never panic, decoding to exactly one char per pair tried.
        for lead in 0x80u16..=0xFF {
            for trail in 0x00u16..=0xFF {
                let bytes = [lead as u8, trail as u8];
                let out = decode_johab(&bytes);
                assert!(!out.is_empty());
            }
        }
        // A buffer that is only lead bytes must still decode without panicking or looping
        // forever (adjacent lead bytes may legitimately pair into real syllables, so the
        // output length is not fixed, only bounded and non-empty).
        let all_leads: Vec<u8> = (0x84u16..=0xD3).map(|b| b as u8).collect();
        let decoded = decode_johab(&all_leads);
        let n = decoded.chars().count();
        assert!(n > 0 && n <= all_leads.len());
    }

    #[test]
    fn header_scan() {
        let head = b"0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1015\n9\n$DWGCODEPAGE\n3\nANSI_1251\r\n0\nENDSEC\n";
        assert_eq!(ascii_code_page(head), Some(&b"ANSI_1251"[..]));
        // Only the HEADER section counts.
        assert_eq!(ascii_code_page(b"0\nSECTION\n2\nHEADER\n0\nENDSEC\n9\n$DWGCODEPAGE\n3\nANSI_1251\n"), None);
        // Wrong group code, or the file ends early.
        assert_eq!(ascii_code_page(b"9\n$DWGCODEPAGE\n1\nANSI_1251\n"), None);
        assert_eq!(ascii_code_page(b"9\n$DWGCODEPAGE\n3"), None);
        assert!(is_utf8_version("AC1021") && is_utf8_version(" AC1032 "));
        assert!(!is_utf8_version("AC1018") && !is_utf8_version("AC1009") && !is_utf8_version("garbage"));
    }

    #[test]
    fn mif_escapes() {
        let one = |s: &str| crate::parse(format!("1\n{s}\n0\nEOF\n").as_bytes()).unwrap()[0].str();
        assert_eq!(one("\\M+5BCFE\\M+1907D\\M+2B9CF\\M+3B5B5!"), "件図圖도!");
        assert_eq!(one("\\M+5BCFE\\U+0021"), "件!");
        // Johab, unknown code pages, bad hex, short escapes and unassigned codes stay literal.
        for s in ["\\M+4ABCD", "\\M+9BCFE", "\\M+5GGGG", "\\M+5BC", "\\M+", "\\M+50041", "\\M+5FFFF"] {
            assert_eq!(one(s), s);
        }
    }
}
