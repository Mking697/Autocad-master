//! Ephemeral contextual-tab lifecycle; drawing and editor state remain engine-owned.
use crate::CadApp;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Context {
    Block,
    Table,
    Text,
}

impl Context {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Block => "Block Editor",
            Self::Table => "Table Tools",
            Self::Text => "Text Editor",
        }
    }

    fn bit(self) -> u8 {
        match self {
            Self::Block => 1,
            Self::Table => 2,
            Self::Text => 4,
        }
    }
}

#[derive(Clone)]
struct State {
    document: Option<u64>,
    normal: String,
    available: u8,
    rendered: String,
}

impl Default for State {
    fn default() -> Self {
        Self { document: None, normal: "Home".into(), available: 0, rendered: String::new() }
    }
}

fn normal(tab: &str) -> bool {
    super::TABS.contains(&tab)
}

/// Return only a single selected table; ambiguous, deleted or other objects never qualify.
pub(super) fn selected_table(app: &CadApp) -> Option<cadcraft_engine::doc::Handle> {
    let st = app.session.state().ok()?;
    if st.selection.len() != 1 {
        return None;
    }
    let handle = *st.selection.first()?;
    matches!(st.doc.entity(handle)?.kind, cadcraft_engine::doc::EntityKind::Table(_)).then_some(handle)
}

/// Selected-table actions route through the existing commands, with an explicit handle.
/// While TABLEDIT owns a prompt, do not start another command or discard its typed cell input.
pub(super) fn table_controls(app: &mut CadApp, ui: &mut egui::Ui, button_height: f32) {
    use crate::icons::Icon;
    let handle = selected_table(app);
    let idle = app.session.running.is_none();
    let editable = idle && handle.is_some_and(|h| !cadcraft_engine::cmd::curves::is_locked(&app.session, h));
    let mut clicked = None;
    ui.allocate_ui_with_layout(egui::vec2(284.0, button_height + 24.0), egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(false), |ui| {
            for (icon, command, label) in [
                (Icon::Table, "tabledit", "Edit Table Cell"),
                (Icon::Plus, "table.insertrow", "Insert Row"),
                (Icon::Plus, "table.insertcol", "Insert Column"),
                (Icon::Table, "table.recalc", "Recalculate Table Formulas"),
            ] {
                let enabled = editable && cadcraft_engine::find_command(command).is_some_and(|spec| (spec.enabled)(&app.session).is_ok());
                if ui.add_enabled_ui(enabled, |ui| super::ribbon_button(ui, icon, label, 68.0, button_height)).inner.clicked() {
                    clicked = Some(command);
                }
            }
        });
        if idle {
            ui.label(crate::tl!("Table"));
        } else {
            ui.label(crate::tl!("Use the command line to edit the cell."));
        }
    });
    if let (Some(command), Some(handle)) = (clicked, handle) {
        if command == "tabledit" {
            app.cmd.buffer.clear();
            app.start(command);
        } else {
            let _ = app.run(command, serde_json::json!({"handle": handle.hex()}));
        }
    }
}

fn available(app: &CadApp) -> Vec<Context> {
    let Ok(st) = app.session.state() else { return Vec::new() };
    if app.ui.start_tab {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(3);
    if st.block_editor().is_some() {
        out.push(Context::Block);
    }
    let tabledit = app.session.running.as_ref().is_some_and(|r| r.id == "tabledit");
    if tabledit || (app.session.running.is_none() && selected_table(app).is_some()) {
        out.push(Context::Table);
    }
    if app.session.editing_mtext().is_some() && app.session.current_prompt().is_some_and(|p| p.accept.text && !p.accept.point) {
        out.push(Context::Text);
    }
    out
}

fn reconcile(state: &mut State, tab: &mut String, document: Option<u64>, contexts: &[Context]) -> bool {
    if normal(tab) {
        state.normal = tab.clone();
    }
    if state.document != document {
        state.document = document;
        state.available = 0;
        if !normal(tab) {
            *tab = state.normal.clone();
        }
    }
    let mask = contexts.iter().fold(0, |mask, c| mask | c.bit());
    let entered = mask & !state.available;
    if let Some(context) = contexts.iter().rev().find(|c| entered & c.bit() != 0) {
        *tab = context.name().into();
    } else if !normal(tab) && !contexts.iter().any(|c| c.name() == tab) {
        *tab = contexts.last().map_or_else(|| state.normal.clone(), |c| c.name().into());
    }
    state.available = mask;
    let changed = state.rendered != *tab;
    state.rendered = tab.clone();
    changed
}

/// Auto-select newly entered contexts; leave manual normal-tab choices alone, then restore the
/// enclosing context or last normal tab on exit. Never persist editor context as a preference.
pub(super) fn sync(app: &mut CadApp, ctx: &egui::Context) -> (Vec<Context>, bool) {
    let id = egui::Id::unique("ribbon_context_lifecycle");
    let mut state = ctx.data_mut(|data| data.get_temp::<State>(id)).unwrap_or_default();
    let contexts = available(app);
    let document = app.session.state().ok().map(|st| st.uid);
    let changed = reconcile(&mut state, &mut app.ui.ribbon_tab, document, &contexts);
    ctx.data_mut(|data| data.insert_temp(id, state));
    (contexts, changed)
}
