//! The AutoCAD-for-Windows style Ribbon: tabs across the top, each holding named panels of
//! labelled tool buttons (Home's Draw, Modify, Annotation, Layers, Block, Properties, Utilities),
//! matching the real Ribbon more closely than CADCraft's original Mac-style Tool Sets sidebar
//! (`palettes::toolsets`), which stays available (Window ▸ Tool Sets) for anyone who prefers it.
//!
//! Only the Home tab has real panels so far; the other tabs are listed (clicking them switches
//! the active tab) but show a placeholder until their commands are grouped the same way.

use egui::{Rect, Sense, Stroke, pos2, vec2};

use crate::CadApp;
use crate::icons::{self, Icon};
use crate::palettes::DRAFTING;
use crate::theme::Tokens;

/// The Ribbon's tabs, in AutoCAD's own order. Only `"Home"` has panels today.
pub const TABS: &[&str] = &["Home", "Insert", "Annotate", "Parametric", "View", "Manage", "Output", "Collaborate", "Express Tools"];

type Tool = (Icon, &'static str, &'static str);

fn draw_tools() -> Vec<Tool> {
    DRAFTING.iter().find(|(n, _, _)| *n == "Draw").map(|(_, l, s)| l.iter().chain(s.iter()).copied().take(10).collect()).unwrap_or_default()
}

fn modify_tools() -> Vec<Tool> {
    DRAFTING.iter().find(|(n, _, _)| *n == "Modify").map(|(_, l, s)| l.iter().chain(s.iter()).copied().take(10).collect()).unwrap_or_default()
}

fn annotation_tools() -> Vec<Tool> {
    let mut v: Vec<Tool> = Vec::new();
    for group in ["Text", "Dimension", "Leader"] {
        if let Some((_, l, s)) = DRAFTING.iter().find(|(n, _, _)| *n == group) {
            v.extend(l.iter().chain(s.iter()).copied());
        }
    }
    v.truncate(10);
    v
}

/// The Home tab's panels: (name, tools). Layers, Block, Properties and Utilities are one
/// representative command each (the full set lives behind the dialog/palette the button opens).
fn home_panels() -> Vec<(&'static str, Vec<Tool>)> {
    vec![
        ("Draw", draw_tools()),
        ("Modify", modify_tools()),
        ("Annotation", annotation_tools()),
        ("Layers", vec![(Icon::LayerProps, "ui.dialog.layers", "Layer Properties"), (Icon::LayerOff, "layoff", "Off")]),
        ("Block", DRAFTING.iter().find(|(n, _, _)| *n == "Block").map(|(_, l, s)| l.iter().chain(s.iter()).copied().collect()).unwrap_or_default()),
        ("Properties", vec![(Icon::Properties, "ui.toggle.palettes", "Properties"), (Icon::MatchProp, "matchprop", "Match Properties")]),
        ("Utilities", vec![(Icon::Measure, "dist", "Measure"), (Icon::List, "list", "List"), (Icon::Area, "area", "Area")]),
    ]
}

/// A large Ribbon button: icon above a word-wrapped label, AutoCAD's own panel-button shape.
fn ribbon_button(ui: &mut egui::Ui, icon: Icon, label: &str, w: f32, h: f32) -> egui::Response {
    let t = Tokens::get();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        if resp.hovered() {
            p.rect_filled(rect, 3.0, t.control_hover.gamma_multiply(0.6));
        }
        let icon_size = (h - 20.0).clamp(16.0, 32.0);
        let icon_rect = Rect::from_center_size(pos2(rect.center().x, rect.top() + icon_size / 2.0 + 2.0), vec2(icon_size, icon_size));
        icons::paint(p, icon_rect, icon, false);
        let mut job = egui::text::LayoutJob::simple(label.to_string(), crate::theme::small(), t.text, w - 4.0);
        job.halign = egui::Align::Center;
        let galley = p.layout_job(job);
        p.galley(pos2(rect.center().x - galley.size().x / 2.0, rect.bottom() - galley.size().y - 2.0), galley, t.text);
    }
    icons::describe_control(ui, &resp, label, None, false);
    resp.on_hover_text(crate::i18n::t(label))
}

/// Tab strip and, for the active tab, its panel row; 26 px tabs over a ~90 px panel band like
/// AutoCAD's own Ribbon. Height is fixed so docking below it never jitters.
pub fn show(app: &mut CadApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    let tab_h = 26.0;
    let panel_h = 92.0;
    egui::Panel::top("cc_ribbon").exact_size(tab_h + panel_h).frame(egui::Frame::NONE.fill(t.chrome)).show(ui, |ui| {
        let r = ui.max_rect();
        let p = ui.painter().clone();
        // Tabs.
        let mut x = r.left() + 8.0;
        for name in TABS {
            let w = ui.painter().layout_no_wrap(crate::i18n::t(name).to_string(), crate::theme::body(), t.text).size().x + 20.0;
            let tr = Rect::from_min_size(pos2(x, r.top()), vec2(w, tab_h));
            let active = app.ui.ribbon_tab == *name;
            let resp = ui.interact(tr, ui.scope_id().with(("ribtab", name)), Sense::click());
            if active {
                p.rect_filled(tr, 0.0, t.tab_active);
            } else if resp.hovered() {
                p.rect_filled(tr, 0.0, t.control_hover.gamma_multiply(0.5));
            }
            p.text(tr.center(), egui::Align2::CENTER_CENTER, crate::i18n::t(name), crate::theme::body(), if active { t.text } else { t.text_dim });
            icons::describe_control(ui, &resp, name, Some(active), true);
            if resp.clicked() {
                app.ui.ribbon_tab = (*name).to_string();
            }
            x += w;
        }
        p.hline(r.x_range(), r.top() + tab_h, Stroke::new(1.0, t.border));
        // Panel row for the active tab.
        let body = Rect::from_min_max(pos2(r.left(), r.top() + tab_h), pos2(r.right(), r.top() + tab_h + panel_h));
        let mut clicked = None;
        ui.scope_builder(egui::UiBuilder::new().max_rect(body), |ui| {
            egui::ScrollArea::horizontal().id_salt("ribbon_panels").auto_shrink([false, true]).show(ui, |ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(false), |ui| {
                    ui.add_space(6.0);
                    if app.ui.ribbon_tab == "Home" {
                        for (i, (name, tools)) in home_panels().into_iter().enumerate() {
                            if i > 0 {
                                ui.add_space(6.0);
                                let (sr, _) = ui.allocate_exact_size(vec2(1.0, panel_h - 20.0), Sense::hover());
                                ui.painter().vline(sr.center().x, sr.y_range(), Stroke::new(1.0, t.border));
                                ui.add_space(6.0);
                            }
                            let panel_w = (tools.len() as f32 * 52.0 - 2.0).max(40.0);
                            ui.allocate_ui_with_layout(vec2(panel_w, panel_h - 12.0), egui::Layout::top_down(egui::Align::Center), |ui| {
                                ui.add_space(4.0);
                                ui.with_layout(egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(false), |ui| {
                                    ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
                                    for (icon, cmd, tip) in &tools {
                                        if ribbon_button(ui, *icon, tip, 50.0, panel_h - 20.0).clicked() {
                                            clicked = Some(*cmd);
                                        }
                                    }
                                });
                                ui.add_space(2.0);
                                ui.label(egui::RichText::new(crate::i18n::t(name)).color(t.text_dim).small());
                            });
                        }
                    } else {
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new(crate::tl!("This Ribbon tab has no panels yet.")).color(t.text_faint));
                    }
                });
            });
        });
        if let Some(c) = clicked {
            app.cmd.buffer.clear();
            app.start(c);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_panels_only_reference_commands_the_engine_or_the_ui_knows() {
        for (_, tools) in home_panels() {
            for (_, cmd, _) in tools {
                assert!(cmd.starts_with("ui.") || cadcraft_engine::find_command(cmd).is_some(), "ribbon references unknown command `{cmd}`");
            }
        }
    }

    #[test]
    fn tabs_list_home_first_and_has_no_duplicates() {
        assert_eq!(TABS.first(), Some(&"Home"));
        let mut seen = std::collections::HashSet::new();
        assert!(TABS.iter().all(|t| seen.insert(*t)), "duplicate ribbon tab");
    }
}
