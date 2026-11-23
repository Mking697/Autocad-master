//! Device printable areas must reach the model-space PDF renderer, not only PNG/layouts.

use cadcraft_doc::{Common, Drawing, EntityKind, Layout, Line, Space};
use cadcraft_geom::{Vec2, Vec3};
use cadcraft_io::{PdfOptions, pdf, plotter};
use cadcraft_render::Sheet;
use serde_json::json;

#[path = "support/pdf_read.rs"]
mod pdf_read;

#[test]
fn model_pdf_device_margins_match_layout_printable_area_and_content() {
    let mut d = Drawing::new_metric();
    d.layouts = vec![Layout::new("Device sheet", 1)];
    let page = &mut d.layouts[0].page;
    page.width_mm = 210.0;
    page.height_mm = 297.0;
    page.landscape = false;
    page.paper_mm = true;
    page.margins_mm = [11.0, 13.0, 17.0, 19.0];
    let device_sheet = Sheet::from_page(page, 1.0);
    assert_eq!(device_sheet.printable.min, Vec2::new(11.0, 13.0));
    assert_eq!(device_sheet.printable.max, Vec2::new(193.0, 278.0));
    let layout = Space::Paper("Device sheet".into());
    for space in [&Space::Model, &layout] {
        d.add(space, Common::default(), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(100.0, 100.0, 0.0) })).unwrap();
    }
    let opts = json!({"width": 210, "height": 297, "landscape": false, "fit": true, "margins": [11, 13, 17, 19], "layers": false});
    let page = pdf::page_for(&d, &Space::Model, &PdfOptions::from_json(&opts)).unwrap();
    assert_eq!(Sheet::from_page(&page, 1.0).printable, device_sheet.printable);
    let model_pdf = pdf_read::read(&plotter::plot(&d, &Space::Model, &opts).unwrap());
    let layout_pdf = pdf_read::read(&plotter::plot(&d, &layout, &json!({"fit": true, "layers": false})).unwrap());
    assert_eq!(model_pdf.pages[0].media, layout_pdf.pages[0].media);
    assert_eq!(model_pdf.pages[0].content, layout_pdf.pages[0].content, "same geometry fitted to the same printable area");
    let default_pdf =
        pdf_read::read(&plotter::plot(&d, &Space::Model, &json!({"width": 210, "height": 297, "landscape": false, "layers": false})).unwrap());
    assert_ne!(model_pdf.pages[0].content, default_pdf.pages[0].content, "asymmetric device margins must affect PDF geometry");
}

#[test]
fn malformed_margin_arrays_are_not_silently_shortened_into_valid_margins() {
    for raw in [
        json!([null, 11, 13, 17, 19]),
        json!([11, "bad", 13, 17, 19]),
        json!([11, 13, 17, 19, 0]),
        json!([11, 13, 17]),
        json!([-1, 0, 0, 0]),
        json!("margins"),
    ] {
        let opts = PdfOptions::from_json(&json!({"margins": raw}));
        assert_eq!(opts.margins, None, "{raw}");
    }
}
