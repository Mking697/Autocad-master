//! Local drawing information and bounded command-name search for the ribbon.

use crate::CadApp;
use crate::theme::Tokens;

const MAX_COMMAND_CHARS: usize = 128;
const MAX_READOUT_CHARS: usize = 256;

#[derive(Clone, Default)]
struct Search {
    text: String,
    unknown: bool,
}

/// Bound imported document names before allocating text/layout data.
fn readout(text: &str) -> String {
    let mut chars = text.chars();
    let mut out: String = chars.by_ref().take(MAX_READOUT_CHARS).collect();
    if chars.next().is_some() {
        out.push('…');
    }
    out
}

/// Render after the ribbon. Only registered command names/aliases are accepted;
/// this is not a command-line argument parser or an online search service.
pub fn show(app: &mut CadApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    let id = egui::Id::unique("cc_ribbon_infobar_search");
    let mut search = ui.ctx().data_mut(|data| data.get_temp::<Search>(id)).unwrap_or_default();
    // Also bound restored temporary state, not just text entered through the widget.
    if let Some((end, _)) = search.text.char_indices().nth(MAX_COMMAND_CHARS) {
        search.text.truncate(end);
    }
    let mut submit = false;
    egui::Panel::top("cc_ribbon_infobar")
        .exact_size(40.0)
        .frame(egui::Frame::NONE.fill(t.chrome_dark).inner_margin(egui::Margin::symmetric(8, 4)))
        .show(ui, |ui| {
            egui::ScrollArea::horizontal().id_salt("cc_infobar_scroll").show(ui, |ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false), |ui| {
                    if let Ok(st) = app.session.state() {
                        ui.add_sized([180.0, 20.0], egui::Label::new(egui::RichText::new(readout(&st.title)).color(t.text)).truncate());
                        // Keep the dirty mark visible even when the title is elided.
                        if st.is_dirty() {
                            ui.label(egui::RichText::new("*").color(t.warn)).on_hover_text(crate::tl!("Unsaved changes"));
                        }
                        let space = match (&st.space, st.edit_space()) {
                            (cadcraft_doc::Space::Model, _) => crate::tl!("Model").to_string(),
                            (cadcraft_doc::Space::Paper(name), cadcraft_doc::Space::Model) => {
                                crate::tf!("Model (viewport: {name})", name = readout(name))
                            }
                            (cadcraft_doc::Space::Paper(name), _) => crate::tf!("Paper: {name}", name = readout(name)),
                        };
                        ui.add_sized([160.0, 20.0], egui::Label::new(egui::RichText::new(space).color(t.text_dim)).truncate());
                    } else {
                        ui.label(egui::RichText::new(crate::tl!("No active document")).color(t.text_dim));
                    }
                    ui.separator();
                    let label = ui.label(crate::tl!("Command search"));
                    let response = ui
                        .add(egui::TextEdit::singleline(&mut search.text).id(id.with("input")).desired_width(160.0).char_limit(MAX_COMMAND_CHARS))
                        .labelled_by(label.id);
                    crate::control::record_widget(ui, &response, "Command search", "text", None);
                    if response.changed() {
                        search.unknown = false;
                    }
                    let nonempty = !search.text.trim().is_empty();
                    let enter = response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                    if enter {
                        // The command-line overlay runs later in the same pass. Enter belongs
                        // to this field, not also to the command we are about to start.
                        ui.input_mut(|input| input.events.retain(|event| !matches!(event, egui::Event::Key { key: egui::Key::Enter, .. })));
                    }
                    let run_response = ui.add_enabled(nonempty, egui::Button::new(crate::tl!("Run")));
                    crate::control::record_widget(ui, &run_response, "Run", "button", None);
                    submit = nonempty && (run_response.clicked() || enter);
                    if search.unknown {
                        ui.label(egui::RichText::new(crate::tl!("Unknown command")).color(t.warn));
                    }
                });
            });
        });
    if submit {
        ui.ctx().request_repaint();
        let name = search.text.trim().to_ascii_lowercase();
        let resolved = cadcraft_engine::cmd::resolve_alias(&name);
        let command =
            if crate::menus::is_ui_command(&name) { Some(name.as_str()) } else { cadcraft_engine::cmd::find_command(&resolved).map(|spec| spec.id) };
        if let Some(command) = command {
            app.cmd.buffer.clear();
            app.start(command);
            search.text.clear();
            search.unknown = false;
        } else {
            // Do not cancel a running command, echo untrusted text, or guess a match.
            search.unknown = true;
        }
    }
    ui.ctx().data_mut(|data| data.insert_temp(id, search));
}
