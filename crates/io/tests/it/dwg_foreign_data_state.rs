//! Diagnostic (not a fix): what of an entity's foreign xdata, extension dictionary/reactors,
//! colour book and a custom-class foreign object actually survives a DWG save+reopen today,
//! per `docs/gaps.md`'s Top-10 #1 ("lossless DWG save ... check current state"). Each assertion
//! records today's real behaviour so the next session knows exactly what, if anything, still
//! needs fixing versus what the row's wording already covers accurately.

use std::sync::Arc;

use cadcraft_doc::foreign::{Foreign, ForeignExtra, ForeignObject};
use cadcraft_doc::*;
use cadcraft_geom::Vec3;

fn through_dwg(d: &Drawing) -> Drawing {
    let dwg = cadcraft_io::write(d, "x.dwg").unwrap();
    cadcraft_io::read(&dwg, "x.dwg").unwrap()
}

fn line(d: &mut Drawing, common: Common) -> Handle {
    d.add(&Space::Model, common, EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(10.0, 5.0, 0.0) })).unwrap()
}

/// An entity's own foreign xdata (`Common::xdata`, e.g. a third-party app's `1001`/`1000` groups).
#[test]
fn entity_xdata_through_dwg() {
    let mut d = Drawing::new_metric();
    let xdata = vec![RawTag { code: 1001, value: "SOME_APP".into() }, RawTag { code: 1000, value: "hello".into() }];
    let h = line(&mut d, Common { xdata: xdata.clone(), ..Common::default() });

    let back = through_dwg(&d);
    let got = back.entity(h).map(|e| e.common.xdata.clone()).unwrap_or_default();
    println!("entity xdata through DWG: wrote {xdata:?}, got back {got:?}");
    // No assertion either way yet: this test's job is to report today's behaviour, not enforce
    // a direction. Flip to `assert_eq!` once this is deliberately fixed either way.
}

/// An entity's extension dictionary and persistent reactors (handles into `Foreign::objects`).
#[test]
fn entity_xdict_and_reactors_through_dwg() {
    let mut d = Drawing::new_metric();
    let other = line(&mut d, Common::default());
    let h = line(&mut d, Common::default());
    let f = Arc::make_mut(&mut d.foreign);
    f.objects.push(ForeignObject { handle: Handle(90001), kind: "XRECORD".into(), tags: vec![RawTag { code: 1, value: "x".into() }] });
    f.entities.insert(h, ForeignExtra { xdict: Some(Handle(90001)), reactors: vec![other], ..ForeignExtra::default() });

    let back = through_dwg(&d);
    let extra = back.foreign.entities.get(&h).cloned().unwrap_or_default();
    println!("entity xdict/reactors through DWG: wrote xdict=Some(90001) reactors=[{other:?}], got back {extra:?}");
}

/// An entity's colour-book colour (DXF groups 430/420 kept on `ForeignExtra::color_book`).
#[test]
fn entity_color_book_through_dwg() {
    let mut d = Drawing::new_metric();
    let h = line(&mut d, Common::default());
    let f = Arc::make_mut(&mut d.foreign);
    f.entities.insert(h, ForeignExtra { color_book: Some(("PANTONE".into(), "PANTONE 100 C".into())), ..ForeignExtra::default() });

    let back = through_dwg(&d);
    let extra = back.foreign.entities.get(&h).cloned().unwrap_or_default();
    println!("entity colour book through DWG: wrote Some((\"PANTONE\", \"PANTONE 100 C\")), got back {:?}", extra.color_book);
}

/// A custom-class foreign object (e.g. a third-party app's own object type, with its CLASS
/// record) kept raw in `Foreign::objects`/`Foreign::classes`.
#[test]
fn custom_class_object_through_dwg() {
    let mut d = Drawing::new_metric();
    let f: &mut Foreign = Arc::make_mut(&mut d.foreign);
    f.classes.push(vec![
        RawTag { code: 1, value: "THIRDPARTY_WIDGET".into() },
        RawTag { code: 2, value: "AcDbThirdPartyWidget".into() },
        RawTag { code: 3, value: "ThirdPartyApp".into() },
        RawTag { code: 90, value: "0".into() },
        RawTag { code: 91, value: "1".into() },
        RawTag { code: 280, value: "0".into() },
        RawTag { code: 281, value: "0".into() },
    ]);
    f.class_order.push("THIRDPARTY_WIDGET".into());
    f.objects.push(ForeignObject {
        handle: Handle(90002),
        kind: "THIRDPARTY_WIDGET".into(),
        tags: vec![RawTag { code: 100, value: "AcDbThirdPartyWidget".into() }, RawTag { code: 1, value: "payload".into() }],
    });
    let back = through_dwg(&d);
    let kept = back.foreign.objects.iter().find(|o| o.kind == "THIRDPARTY_WIDGET").cloned();
    println!("custom-class object through DWG: wrote one THIRDPARTY_WIDGET object, got back {kept:?}");
    println!("classes kept through DWG: {:?}", back.foreign.classes.iter().any(|c| c.iter().any(|t| t.value == "THIRDPARTY_WIDGET")));
}
