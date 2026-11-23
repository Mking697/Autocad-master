//! Restore native MLEADERSTYLE fields omitted by acadrust 0.6.6's DXF reader.
//! Its DWG writer/reader already support these fields. Scan only the public DXF
//! groups we need, with constant auxiliary memory, leaving the dependency unmodified.

use acadrust::objects::{BlockContentConnectionType, ObjectType};
use acadrust::{CadDocument, Handle};

#[derive(Default)]
struct Fields {
    handle: Option<Handle>,
    x: Option<f64>,
    y: Option<f64>,
    z: Option<f64>,
    connection: Option<i16>,
}

impl Fields {
    fn apply(&self, doc: &mut CadDocument) {
        let Some(handle) = self.handle else { return };
        let Some(ObjectType::MultiLeaderStyle(st)) = doc.objects.get_mut(&handle) else { return };
        if let Some(v) = self.x {
            st.block_content_scale_x = v;
        }
        if let Some(v) = self.y {
            st.block_content_scale_y = v;
        }
        if let Some(v) = self.z {
            st.block_content_scale_z = v;
        }
        if let Some(v) = self.connection {
            st.block_content_connection = BlockContentConnectionType::from(v);
        }
    }
}

pub(crate) fn restore(doc: &mut CadDocument, bytes: &[u8]) {
    // The normal save bridge emits UTF-8 ASCII DXF. Binary/foreign encodings keep
    // their existing conversion path rather than guessing at their byte layout.
    let Ok(text) = std::str::from_utf8(bytes) else { return };
    let mut lines = text.lines();
    let mut active = false;
    let mut fields = Fields::default();
    while let (Some(code), Some(value)) = (lines.next(), lines.next()) {
        let Ok(code) = code.trim().parse::<i32>() else { continue };
        if code == 0 {
            if active {
                fields.apply(doc);
            }
            fields = Fields::default();
            active = value.trim() == "MLEADERSTYLE";
            continue;
        }
        if !active {
            continue;
        }
        let value = value.trim();
        let scale = || value.parse::<f64>().ok().filter(|v| v.is_finite() && *v != 0.0 && v.abs() <= 1.0e12);
        match code {
            5 => fields.handle = u64::from_str_radix(value, 16).ok().map(Handle::new),
            47 => fields.x = scale(),
            49 => fields.y = scale(),
            140 => fields.z = scale(),
            177 => fields.connection = value.parse::<i16>().ok().filter(|v| matches!(v, 0 | 1)),
            _ => {}
        }
    }
    if active {
        fields.apply(doc);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restores_native_fields_by_handle_without_touching_other_records() {
        let mut doc = CadDocument::new();
        let mut st = acadrust::objects::MultiLeaderStyle::new("Symbols");
        st.handle = Handle::new(0xABC);
        doc.objects.insert(st.handle, ObjectType::MultiLeaderStyle(st));
        restore(&mut doc, b"0\nMLEADERSTYLE\n5\nABC\n47\n2.5\n49\n3\n140\n4\n177\n1\n0\nLINE\n47\n9\n");
        let Some(ObjectType::MultiLeaderStyle(st)) = doc.objects.get(&Handle::new(0xABC)) else { panic!("style") };
        assert_eq!((st.block_content_scale_x, st.block_content_scale_y, st.block_content_scale_z), (2.5, 3.0, 4.0));
        assert_eq!(st.block_content_connection, BlockContentConnectionType::BasePoint);
        for bytes in [b"0\nMLEADERSTYLE\n5\nABC\n47\nNaN\n177\n999\n".as_slice(), b"0\nMLEADERSTYLE\n5\nnothex\n47\n1e309\n", &[255, 0, 1]] {
            restore(&mut doc, bytes);
        }
        let Some(ObjectType::MultiLeaderStyle(st)) = doc.objects.get(&Handle::new(0xABC)) else { panic!("style") };
        assert_eq!(st.block_content_scale_x, 2.5);
    }
}
