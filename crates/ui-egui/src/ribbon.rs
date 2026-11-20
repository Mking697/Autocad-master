//! The AutoCAD-for-Windows style Ribbon: tabs across the top, each holding named panels of
//! labelled tool buttons (Home's Draw, Modify, Annotation, Layers, Block, Properties, Utilities),
//! matching the real Ribbon more closely than CADCraft's original Mac-style Tool Sets sidebar
//! (`palettes::toolsets`), which stays available (Window ▸ Tool Sets) for anyone who prefers it.
//!
//! Home, Insert, Annotate, Parametric, View, Manage, Output and Express Tools have real panels;
//! Collaborate stays a placeholder (no wireable commands yet — see `docs/gaps.md` F23).

use egui::{Rect, Sense, Stroke, pos2, vec2};

use crate::CadApp;
use crate::icons::{self, Icon};
use crate::palettes::DRAFTING;
use crate::theme::Tokens;

/// The Ribbon's tabs, in AutoCAD's own order. `"Home"`, `"Insert"`, `"Annotate"`, `"Parametric"`
/// and `"View"` have panels; the rest are listed but show a placeholder (see `panels_for`).
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

/// The Insert tab's panels: block references, external references, and file exchange.
fn insert_panels() -> Vec<(&'static str, Vec<Tool>)> {
    vec![
        (
            "Block",
            vec![
                (Icon::Insert, "insert", "Insert"),
                (Icon::BlockCreate, "block", "Create Block"),
                (Icon::BlockEdit, "bedit", "Block Editor"),
                (Icon::AttDef, "attdef", "Define Attributes"),
            ],
        ),
        (
            "Reference",
            vec![
                (Icon::Attach, "xattach", "Attach"),
                (Icon::Blocks, "xref", "Reference Manager"),
                (Icon::Insert, "xbind", "Bind"),
                (Icon::BlockEdit, "xopen", "Edit Reference"),
            ],
        ),
        ("Import", vec![(Icon::Import, "import", "Import"), (Icon::Export, "export", "Export")]),
    ]
}

/// The Annotate tab's panels: AutoCAD groups Text/Dimensions/Leaders/Tables under their own
/// panels here rather than Home's single combined Annotation panel.
fn annotate_panels() -> Vec<(&'static str, Vec<Tool>)> {
    ["Text", "Dimension", "Leader", "Table"]
        .iter()
        .filter_map(|group| DRAFTING.iter().find(|(n, _, _)| n == group).map(|(n, l, s)| (*n, l.iter().chain(s.iter()).copied().collect::<Vec<_>>())))
        .collect()
}

/// The Parametric tab's panels: AutoCAD splits Geometric and Dimensional constraints, plus a
/// Manage panel for the Parameters Manager and Delete Constraints.
fn parametric_panels() -> Vec<(&'static str, Vec<Tool>)> {
    let Some((_, large, small)) = DRAFTING.iter().find(|(n, _, _)| *n == "Parametric") else {
        return Vec::new();
    };
    let is_dim = |cmd: &str| cmd.starts_with("dc");
    let (dim, geo): (Vec<Tool>, Vec<Tool>) = small.iter().copied().partition(|(_, cmd, _)| is_dim(cmd));
    vec![
        ("Geometric", large.iter().copied().chain(geo).collect()),
        ("Dimensional", dim),
        ("Manage", vec![(Icon::Parameters, "parameters", "Parameters Manager"), (Icon::DelConstraint, "delconstraint", "Delete Constraints")]),
    ]
}

/// The View tab's panels: navigation, viewport configuration and palette toggles.
fn view_panels() -> Vec<(&'static str, Vec<Tool>)> {
    vec![
        (
            "Navigate",
            vec![
                (Icon::ZoomExtents, "zoom.extents", "Zoom Extents"),
                (Icon::ZoomWindow, "zoom.window", "Zoom Window"),
                (Icon::Pan, "pan", "Pan"),
                (Icon::Orbit, "3dorbit", "Orbit"),
            ],
        ),
        (
            "Viewports",
            vec![(Icon::Blocks, "vports", "New Viewports"), (Icon::Blocks, "vports.2", "2 Viewports"), (Icon::Blocks, "vports.4", "4 Viewports")],
        ),
        ("Palettes", vec![(Icon::Properties, "ui.toggle.palettes", "Properties"), (Icon::LayerProps, "ui.dialog.layers", "Layer Properties")]),
    ]
}

/// The Manage tab's panels: application loading, command-alias customization, and settings.
/// AutoCAD's real Manage tab also has an Action Recorder panel; CADCraft has no action-recording
/// engine yet, so it is left out rather than wired to a non-existent command.
fn manage_panels() -> Vec<(&'static str, Vec<Tool>)> {
    vec![
        ("Applications", vec![(Icon::Import, "appload", "Load Application")]),
        ("Customization", vec![(Icon::Gear, "aliasedit", "Edit Aliases")]),
        ("Settings", vec![(Icon::Gear, "options", "Options")]),
    ]
}

/// The Output tab's panels: plotting, exporting to other formats, and batch publishing.
fn output_panels() -> Vec<(&'static str, Vec<Tool>)> {
    vec![
        ("Plot", vec![(Icon::Plot, "plot", "Plot"), (Icon::Preview, "preview", "Preview")]),
        ("Export to DWF/PDF", vec![(Icon::Export, "export", "Export")]),
        ("Batch Publish", vec![(Icon::Publish, "publish", "Batch Publish")]),
    ]
}

/// The Express Tools tab's panels: the classic Express Tools menu's commands, grouped the way
/// AutoCAD's own Express Tools ribbon tab does (Blocks/Modify/Text/Draw/Tools).
fn express_panels() -> Vec<(&'static str, Vec<Tool>)> {
    vec![
        (
            "Blocks",
            vec![(Icon::Explode, "burst", "Burst"), (Icon::BlockEdit, "blockreplace", "Replace Block"), (Icon::Copy, "ncopy", "Copy Nested Objects")],
        ),
        ("Modify", vec![(Icon::Move, "mocoro", "Move/Copy/Rotate")]),
        ("Text", vec![(Icon::MText, "txt2mtxt", "Convert Text to Mtext")]),
        ("Draw", vec![(Icon::Break, "breakline", "Break-line Symbol")]),
        ("Tools", vec![(Icon::Gear, "aliasedit", "Command Aliases")]),
    ]
}

/// The panels for a Ribbon tab, or `None` for a tab that still shows the placeholder.
fn panels_for(tab: &str) -> Option<Vec<(&'static str, Vec<Tool>)>> {
    match tab {
        "Home" => Some(home_panels()),
        "Insert" => Some(insert_panels()),
        "Annotate" => Some(annotate_panels()),
        "Parametric" => Some(parametric_panels()),
        "View" => Some(view_panels()),
        "Manage" => Some(manage_panels()),
        "Output" => Some(output_panels()),
        "Express Tools" => Some(express_panels()),
        _ => None,
    }
}

/// Candidate Quick Access Toolbar commands: icon, command id, label (matching
/// `chrome::TOOLBAR`'s own labels so they share translations), and whether it is one of
/// the default CADCraft QAT actions (New, Open, Save, Undo, Redo, Plot).
pub const QAT_CANDIDATES: &[(Icon, &str, &str, bool)] = &[
    (Icon::New, "new", "New Drawing", true),
    (Icon::Open, "open", "Open", true),
    (Icon::Save, "qsave", "Save", true),
    (Icon::SaveAs, "saveas", "Save As", false),
    (Icon::Undo, "u", "Undo", true),
    (Icon::Redo, "redo", "Redo", true),
    (Icon::Plot, "plot", "Print", true),
    (Icon::Publish, "publish", "Batch Publish", false),
];

/// Bound preference/control input to unique supported tools, preserving the requested order.
/// Only inspect a fixed prefix so hostile lists cannot cause unbounded per-frame work.
pub(crate) fn normalize_qat(commands: &mut Vec<String>) {
    let mut normalized = Vec::with_capacity(QAT_CANDIDATES.len());
    for command in commands.iter().take(64) {
        if QAT_CANDIDATES.iter().any(|(_, id, _, _)| command == id) && !normalized.contains(command) {
            normalized.push(command.clone());
        }
    }
    *commands = normalized;
}

/// Quick Access Toolbar: a slim, customizable icon strip above the Ribbon's tabs (AutoCAD-for-
/// Windows keeps it in the title bar; CADCraft's title row already holds the window title and
/// macOS traffic lights, so it sits here instead, directly above the tabs it belongs with).
/// Draws the commands in `app.ui.qat_commands` (unknown ids are skipped, never panic) plus a
/// "Customize Quick Access Toolbar" dropdown at the end that adds or removes any candidate.
/// Returns a clicked command id, if any.
fn qat_row(app: &mut CadApp, ui: &mut egui::Ui, r: Rect) -> Option<&'static str> {
    normalize_qat(&mut app.ui.qat_commands);
    let t = Tokens::get();
    let size = 18.0;
    let customize_x = (r.left() + 8.0 + app.ui.qat_commands.len() as f32 * 24.0).min((r.right() - 18.0).max(r.left()));
    let tools_rect = Rect::from_min_max(r.min, pos2(customize_x.max(r.left()), r.bottom()));
    let mut clicked = None;
    ui.scope_builder(egui::UiBuilder::new().max_rect(tools_rect), |ui| {
        egui::ScrollArea::horizontal().id_salt("qat_tools").auto_shrink([false, false]).show(ui, |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.add_space(6.0);
                for command in &app.ui.qat_commands {
                    let Some((icon, cmd, label, _)) = QAT_CANDIDATES.iter().find(|(_, c, _, _)| c == command) else { continue };
                    let enabled = cadcraft_engine::find_command(cmd).is_some_and(|spec| (spec.enabled)(&app.session).is_ok());
                    if ui.add_enabled_ui(enabled, |ui| icons::button(ui, *icon, size, label, false)).inner.clicked() {
                        clicked = Some(*cmd);
                    }
                }
            });
        });
    });
    // Keep customization visible even when a narrow window scrolls its tool strip.
    let cr = Rect::from_min_size(pos2(customize_x, r.center().y - size / 2.0), vec2(14.0, size));
    let cresp = ui.interact(cr, ui.scope_id().with("qat_customize"), Sense::click());
    if cresp.hovered() {
        ui.painter().rect_filled(cr, 3.0, t.control_hover.gamma_multiply(0.6));
    }
    icons::paint(ui.painter(), cr.shrink(2.0), Icon::ChevronDown, false);
    icons::describe_control(ui, &cresp, "Customize Quick Access Toolbar", None, false);
    let cresp = cresp.on_hover_text(crate::tl!("Customize Quick Access Toolbar"));
    egui::Popup::from_toggle_button_response(&cresp).show(|ui| {
        for (_, cmd, label, _) in QAT_CANDIDATES {
            let mut shown = app.ui.qat_commands.iter().any(|c| c == cmd);
            if ui.checkbox(&mut shown, crate::i18n::t(label)).changed() {
                if shown {
                    app.ui.qat_commands.push((*cmd).to_string());
                } else {
                    app.ui.qat_commands.retain(|c| c != cmd);
                }
                ui.close();
            }
        }
    });
    clicked
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

/// Tab strip and, for the active tab, its panel row; 26 px tabs over a panel band like AutoCAD's
/// own Ribbon. Height is fixed so docking below it never jitters.
pub fn show(app: &mut CadApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    let qat_h = 24.0;
    let tab_h = 26.0;
    // The band below each panel's buttons that names it (AutoCAD's own "Draw ▾" style footer),
    // separated from the buttons by a rule so it doesn't read as crowded extra button text.
    let name_h = 20.0;
    let panel_h = 104.0;
    let button_h = panel_h - name_h - 10.0;
    egui::Panel::top("cc_ribbon").exact_size(qat_h + tab_h + panel_h).frame(egui::Frame::NONE.fill(t.chrome)).show(ui, |ui| {
        let r = ui.max_rect();
        let p = ui.painter().clone();
        // Quick Access Toolbar.
        let qat_clicked = qat_row(app, ui, Rect::from_min_size(r.min, vec2(r.width(), qat_h)));
        p.hline(r.x_range(), r.top() + qat_h, Stroke::new(1.0, t.border.gamma_multiply(0.6)));
        let r = Rect::from_min_max(pos2(r.left(), r.top() + qat_h), r.max);
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
                    if let Some(panels) = panels_for(&app.ui.ribbon_tab) {
                        for (i, (name, tools)) in panels.into_iter().enumerate() {
                            if i > 0 {
                                ui.add_space(6.0);
                                let (sr, _) = ui.allocate_exact_size(vec2(1.0, panel_h - 20.0), Sense::hover());
                                ui.painter().vline(sr.center().x, sr.y_range(), Stroke::new(1.0, t.border));
                                ui.add_space(6.0);
                            }
                            let panel_w = (tools.len() as f32 * 52.0 - 2.0).max(44.0);
                            ui.allocate_ui_with_layout(vec2(panel_w, panel_h - 4.0), egui::Layout::top_down(egui::Align::Center), |ui| {
                                ui.add_space(4.0);
                                ui.with_layout(egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(false), |ui| {
                                    ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
                                    for (icon, cmd, tip) in &tools {
                                        if ribbon_button(ui, *icon, tip, 50.0, button_h).clicked() {
                                            clicked = Some(*cmd);
                                        }
                                    }
                                });
                                // A rule, then the panel's own name, clearly below the buttons'
                                // own labels rather than crowding them (#U12, round 3).
                                let (rr, _) = ui.allocate_exact_size(vec2(panel_w * 0.85, 1.0), Sense::hover());
                                ui.painter().hline(rr.x_range(), rr.center().y, Stroke::new(1.0, t.border.gamma_multiply(0.7)));
                                ui.add_space(2.0);
                                ui.label(egui::RichText::new(crate::i18n::t(name)).color(t.text_dim).size(11.5));
                            });
                        }
                    } else if app.ui.ribbon_tab == "Collaborate" {
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(crate::tl!("Collaborate has no online service to connect to yet (Share, eTransmit, Markup Import)."))
                                .color(t.text_faint),
                        );
                    } else {
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new(crate::tl!("This Ribbon tab has no panels yet.")).color(t.text_faint));
                    }
                });
            });
        });
        if let Some(c) = clicked.or(qat_clicked) {
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
    fn every_tab_with_panels_only_references_known_commands() {
        for tab in TABS {
            if let Some(panels) = panels_for(tab) {
                assert!(!panels.is_empty(), "`{tab}` panels_for returned no panels");
                for (name, tools) in panels {
                    for (_, cmd, _) in tools {
                        assert!(
                            cmd.starts_with("ui.") || cadcraft_engine::find_command(cmd).is_some(),
                            "`{tab}` panel `{name}` references unknown command `{cmd}`"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn tabs_list_home_first_and_has_no_duplicates() {
        assert_eq!(TABS.first(), Some(&"Home"));
        let mut seen = std::collections::HashSet::new();
        assert!(TABS.iter().all(|t| seen.insert(*t)), "duplicate ribbon tab");
    }

    #[test]
    fn qat_candidates_only_reference_known_commands_and_have_no_duplicates() {
        let mut seen = std::collections::HashSet::new();
        for (_, cmd, _, _) in QAT_CANDIDATES {
            assert!(cadcraft_engine::find_command(cmd).is_some(), "QAT candidate references unknown command `{cmd}`");
            assert!(seen.insert(*cmd), "duplicate QAT candidate `{cmd}`");
        }
    }

    #[test]
    fn qat_preferences_drop_unknown_duplicates_and_excess_input() {
        let mut commands = vec!["qsave".into(), "bad".into(), "qsave".into(), "new".into()];
        normalize_qat(&mut commands);
        assert_eq!(commands, ["qsave", "new"]);
        let mut commands = vec!["bad".into(); 1000];
        commands.push("qsave".into());
        normalize_qat(&mut commands);
        assert!(commands.is_empty());
    }

    #[test]
    fn default_qat_commands_are_exactly_the_default_candidates_in_order() {
        let defaults: Vec<&str> = QAT_CANDIDATES.iter().filter(|(_, _, _, d)| *d).map(|(_, cmd, _, _)| *cmd).collect();
        let ui = crate::UiState::default();
        assert_eq!(ui.qat_commands, defaults);
    }
}
