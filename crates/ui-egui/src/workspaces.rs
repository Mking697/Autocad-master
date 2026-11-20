//! Named, bounded UI layouts. Drawing state and transient interaction state never enter a layout.

use std::io::{self, Write};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{UiState, docking, ribbon};

const VERSION: u32 = 1;
const MAX_ENTRIES: usize = 16;
const MAX_NAME_CHARS: usize = 64;
const MAX_STRING_BYTES: usize = 256;
const MAX_STRINGS: usize = 256;
const MAX_LAYOUT_BYTES: usize = 32 * 1024;
const MAX_PREFS_BYTES: usize = 256 * 1024;

/// A registry owns snapshots, not an alternate copy of the app's live UI state.
#[derive(Clone, Default)]
pub struct Registry {
    entries: Vec<Entry>,
    current: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    name: String,
    layout: Snapshot,
}

// Deliberately no serde(default): incomplete layouts must not silently reset preferences.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Snapshot {
    docking: docking::Workspace,
    docking_version: u32,
    show_toolsets: bool,
    show_palettes: bool,
    show_toolbar: bool,
    show_ribbon: bool,
    show_file_tabs: bool,
    show_status_bar: bool,
    show_command_line: bool,
    show_viewcube: bool,
    show_ucs_icon: bool,
    show_layer_list: bool,
    ribbon_tab: String,
    toolset_tab: String,
    collapsed_groups: Vec<String>,
    hidden_tools: Vec<String>,
    properties_all: bool,
    in_window_menu: bool,
    history_lines: usize,
    qat_commands: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Preferences {
    version: u32,
    entries: Vec<Entry>,
    current: Option<String>,
    live_layout: Snapshot,
}

impl Snapshot {
    fn capture(ui: &UiState) -> Result<Self, String> {
        // Validate before cloning or serializing: JSON must not turn nonfinite floats into null.
        docking::validate(&ui.docking)?;
        validate_strings(&ui.collapsed_groups, MAX_STRINGS)?;
        validate_strings(&ui.hidden_tools, MAX_STRINGS)?;
        validate_strings(&ui.qat_commands, 64)?;
        validate_string(&ui.ribbon_tab)?;
        validate_string(&ui.toolset_tab)?;
        let mut layout = Self {
            docking: ui.docking.clone(),
            docking_version: docking::LAYOUT_VERSION,
            show_toolsets: ui.show_toolsets,
            show_palettes: ui.show_palettes,
            show_toolbar: ui.show_toolbar,
            show_ribbon: ui.show_ribbon,
            show_file_tabs: ui.show_file_tabs,
            show_status_bar: ui.show_status_bar,
            show_command_line: ui.show_command_line,
            show_viewcube: ui.show_viewcube,
            show_ucs_icon: ui.show_ucs_icon,
            show_layer_list: ui.show_layer_list,
            ribbon_tab: if ribbon::TABS.contains(&ui.ribbon_tab.as_str()) { ui.ribbon_tab.clone() } else { "Home".into() },
            toolset_tab: ui.toolset_tab.clone(),
            collapsed_groups: ui.collapsed_groups.clone(),
            hidden_tools: ui.hidden_tools.clone(),
            properties_all: ui.properties_all,
            in_window_menu: ui.in_window_menu,
            history_lines: ui.history_lines,
            qat_commands: ui.qat_commands.clone(),
        };
        layout.validate()?;
        Ok(layout)
    }

    fn validate(&mut self) -> Result<(), String> {
        if self.docking_version != docking::LAYOUT_VERSION {
            return Err("Unsupported workspace dockingVersion".into());
        }
        docking::validate(&self.docking)?;
        if !ribbon::TABS.contains(&self.ribbon_tab.as_str()) {
            return Err("Workspace ribbonTab must be a normal Ribbon tab".into());
        }
        if !matches!(self.toolset_tab.as_str(), "Drafting" | "Modeling") {
            return Err("Workspace toolsetTab must be Drafting or Modeling".into());
        }
        if self.history_lines > crate::cmdline::MAX_LINES {
            return Err("Workspace historyLines is out of range".into());
        }
        validate_strings(&self.collapsed_groups, MAX_STRINGS)?;
        validate_strings(&self.hidden_tools, MAX_STRINGS)?;
        validate_strings(&self.qat_commands, 64)?;
        // Enforce the input limit as well as the normalized output limit.
        check_size(self, MAX_LAYOUT_BYTES)?;
        ribbon::normalize_qat(&mut self.qat_commands);
        Ok(())
    }

    fn apply(mut self, ui: &mut UiState) -> Result<(), String> {
        self.validate()?;
        // No fallible work after the first assignment. Never replace the entire UiState.
        ui.docking = self.docking;
        ui.show_toolsets = self.show_toolsets;
        ui.show_palettes = self.show_palettes;
        ui.show_toolbar = self.show_toolbar;
        ui.show_ribbon = self.show_ribbon;
        ui.show_file_tabs = self.show_file_tabs;
        ui.show_status_bar = self.show_status_bar;
        ui.show_command_line = self.show_command_line;
        ui.show_viewcube = self.show_viewcube;
        ui.show_ucs_icon = self.show_ucs_icon;
        ui.show_layer_list = self.show_layer_list;
        ui.ribbon_tab = self.ribbon_tab;
        ui.toolset_tab = self.toolset_tab;
        ui.collapsed_groups = self.collapsed_groups;
        ui.hidden_tools = self.hidden_tools;
        ui.properties_all = self.properties_all;
        ui.in_window_menu = self.in_window_menu;
        ui.history_lines = self.history_lines;
        ui.qat_commands = self.qat_commands;
        Ok(())
    }
}

impl Registry {
    fn listing(&self) -> Value {
        json!({"names": self.entries.iter().map(|entry| &entry.name).collect::<Vec<_>>(), "current": self.current})
    }

    fn index(&self, name: &str) -> Option<usize> {
        self.entries.iter().position(|entry| entry.name.eq_ignore_ascii_case(name))
    }

    fn payload(&self, live_layout: Snapshot) -> Result<Value, String> {
        let payload = Preferences { version: VERSION, entries: self.entries.clone(), current: self.current.clone(), live_layout };
        check_size(&payload, MAX_PREFS_BYTES)?;
        serde_json::to_value(payload).map_err(|error| error.to_string())
    }

    /// `liveLayout` preserves edits made since the active named snapshot was saved.
    pub fn prefs(&self, ui: &UiState) -> Result<Option<Value>, String> {
        if self.entries.is_empty() && self.current.is_none() {
            return Ok(None);
        }
        self.payload(Snapshot::capture(ui)?).map(Some)
    }

    /// Validate the entire payload before modifying even one live layout field.
    pub fn restore(value: &Value, ui: &mut UiState) -> Result<Self, String> {
        validate_value_depth(value, 0, &mut 0)?;
        check_size(value, MAX_PREFS_BYTES)?;
        // Option fields normally allow missing keys in serde; this schema does not.
        if value.get("current").is_none() {
            return Err("Workspace preferences require current (a name or null)".into());
        }
        // Check raw snapshots too: nested docking serde may ignore unknown fields.
        let entries = value.get("entries").and_then(Value::as_array).ok_or("Workspace preferences require entries")?;
        if entries.len() > MAX_ENTRIES {
            return Err("At most 16 named workspaces are allowed".into());
        }
        for entry in entries {
            check_size(entry.get("layout").ok_or("Workspace entry requires layout")?, MAX_LAYOUT_BYTES)?;
        }
        check_size(value.get("liveLayout").ok_or("Workspace preferences require liveLayout")?, MAX_LAYOUT_BYTES)?;
        let mut prefs: Preferences = serde_json::from_value(value.clone()).map_err(|error| error.to_string())?;
        if prefs.version != VERSION {
            return Err("Unsupported workspace preferences version".into());
        }
        if prefs.entries.len() > MAX_ENTRIES {
            return Err("At most 16 named workspaces are allowed".into());
        }
        let mut names: Vec<String> = Vec::with_capacity(prefs.entries.len());
        for entry in &mut prefs.entries {
            let name = workspace_name(&entry.name)?;
            if names.iter().any(|existing| existing.eq_ignore_ascii_case(&name)) {
                return Err("Duplicate workspace name".into());
            }
            entry.name = name.clone();
            names.push(name);
            entry.layout.validate()?;
        }
        let current = match prefs.current {
            Some(name) => {
                let name = workspace_name(&name)?;
                Some(names.iter().find(|existing| existing.eq_ignore_ascii_case(&name)).cloned().ok_or("Active workspace does not exist")?)
            }
            None => None,
        };
        prefs.live_layout.validate()?;
        let registry = Self { entries: prefs.entries, current };
        // Trimming/normalization can change serialized lengths, so check the final payload too.
        registry.payload(prefs.live_layout.clone())?;
        prefs.live_layout.apply(ui)?;
        Ok(registry)
    }
}

/// Route only workspace commands; the caller owns registration and all interactive UI.
pub fn route(app: &mut crate::CadApp, id: &str, params: &Value) -> Option<Result<Value, String>> {
    match id {
        "ui.workspace.list" => Some(Ok(app.workspaces.listing())),
        "ui.workspace.save" => Some(save(app, params)),
        "ui.workspace.switch" => Some(switch(app, params)),
        "ui.workspace.delete" => Some(delete(app, params)),
        _ => None,
    }
}

fn parameter_name(params: &Value) -> Result<String, String> {
    workspace_name(params.get("name").and_then(Value::as_str).ok_or("Workspace name is required")?)
}

fn workspace_name(name: &str) -> Result<String, String> {
    if name.chars().any(char::is_control) {
        return Err("Workspace names cannot contain control characters".into());
    }
    let name = name.trim();
    if name.is_empty() || name.len() > MAX_STRING_BYTES || name.chars().count() > MAX_NAME_CHARS {
        return Err("Workspace names require 1–64 characters and at most 256 bytes".into());
    }
    Ok(name.to_owned())
}

fn save(app: &mut crate::CadApp, params: &Value) -> Result<Value, String> {
    let name = parameter_name(params)?;
    let replace = match params.get("replace") {
        Some(value) => value.as_bool().ok_or("Workspace replace must be a boolean")?,
        None => false,
    };
    let mut next = app.workspaces.clone();
    let index = next.index(&name);
    if index.is_some() && !replace {
        return Err("Workspace already exists; set replace to true to overwrite it".into());
    }
    if index.is_none() && next.entries.len() >= MAX_ENTRIES {
        return Err("At most 16 named workspaces are allowed".into());
    }
    let layout = Snapshot::capture(&app.ui)?;
    if let Some(entry) = index.and_then(|index| next.entries.get_mut(index)) {
        // Case-insensitive overwrite preserves the original display spelling and order.
        entry.layout = layout.clone();
        next.current = Some(entry.name.clone());
    } else {
        next.entries.push(Entry { name: name.clone(), layout: layout.clone() });
        next.current = Some(name);
    }
    next.payload(layout)?;
    app.workspaces = next;
    Ok(app.workspaces.listing())
}

fn switch(app: &mut crate::CadApp, params: &Value) -> Result<Value, String> {
    let name = parameter_name(params)?;
    let entry = app.workspaces.index(&name).and_then(|index| app.workspaces.entries.get(index)).ok_or("Workspace does not exist")?.clone();
    let mut next = app.workspaces.clone();
    next.current = Some(entry.name);
    next.payload(entry.layout.clone())?;
    entry.layout.apply(&mut app.ui)?;
    app.workspaces = next;
    Ok(app.workspaces.listing())
}

fn delete(app: &mut crate::CadApp, params: &Value) -> Result<Value, String> {
    let name = parameter_name(params)?;
    if app.workspaces.index(&name).is_none() {
        return Err("Workspace does not exist".into());
    }
    app.workspaces.entries.retain(|entry| !entry.name.eq_ignore_ascii_case(&name));
    if app.workspaces.current.as_ref().is_some_and(|current| current.eq_ignore_ascii_case(&name)) {
        app.workspaces.current = None;
    }
    // Deleting the active name leaves its live arrangement alone, now unnamed.
    Ok(app.workspaces.listing())
}

fn validate_string(value: &str) -> Result<(), String> {
    if value.len() > MAX_STRING_BYTES || value.chars().any(char::is_control) {
        return Err("Workspace layout strings must be control-free and at most 256 bytes".into());
    }
    Ok(())
}

fn validate_strings(values: &[String], limit: usize) -> Result<(), String> {
    if values.len() > limit {
        return Err(format!("Workspace layout string list exceeds {limit} entries"));
    }
    for value in values {
        validate_string(value)?;
    }
    Ok(())
}

/// A bounded writer checks encoded bytes without allocating an oversized JSON buffer.
struct SizeBudget(usize);

impl Write for SizeBudget {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self.0.checked_sub(bytes.len()).ok_or_else(|| io::Error::other("Workspace JSON exceeds its size limit"))?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn check_size(value: &impl Serialize, limit: usize) -> Result<(), String> {
    serde_json::to_writer(SizeBudget(limit), value).map_err(|error| error.to_string())
}

// Values can also come from Rust callers, bypassing serde_json's parser recursion limit.
fn validate_value_depth(value: &Value, depth: usize, nodes: &mut usize) -> Result<(), String> {
    if depth > 128 || *nodes >= MAX_PREFS_BYTES {
        return Err("Workspace preferences are too deeply nested or too large".into());
    }
    *nodes += 1;
    match value {
        Value::Array(values) => {
            for child in values {
                validate_value_depth(child, depth + 1, nodes)?;
            }
        }
        Value::Object(values) => {
            for child in values.values() {
                validate_value_depth(child, depth + 1, nodes)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_trim_and_reject_controls_and_oversize() {
        assert_eq!(workspace_name("  Drafting  "), Ok("Drafting".into()));
        assert!(workspace_name("\nDrafting").is_err());
        assert!(workspace_name(&"é".repeat(65)).is_err());
        assert!(workspace_name("   ").is_err());
    }

    #[test]
    fn restore_uses_live_layout_and_preserves_transient_fields() -> Result<(), String> {
        let ui = UiState::default();
        let registry = Registry { entries: vec![Entry { name: "One".into(), layout: Snapshot::capture(&ui)? }], current: Some("One".into()) };
        let mut live = ui.clone();
        live.show_ribbon = !ui.show_ribbon;
        let value = registry.prefs(&live)?.ok_or("Expected preferences")?;
        let mut restored = ui.clone();
        restored.dialog = Some("keep".into());
        restored.start_tab = true;
        let loaded = Registry::restore(&value, &mut restored)?;
        assert_eq!(loaded.current.as_deref(), Some("One"));
        assert_eq!(restored.show_ribbon, live.show_ribbon);
        assert_eq!(restored.dialog.as_deref(), Some("keep"));
        assert!(restored.start_tab);
        Ok(())
    }

    #[test]
    fn malformed_late_entry_does_not_apply_live_layout() -> Result<(), String> {
        let mut ui = UiState::default();
        let registry = Registry { entries: vec![Entry { name: "One".into(), layout: Snapshot::capture(&ui)? }], current: None };
        let mut value = registry.prefs(&ui)?.ok_or("Expected preferences")?;
        value["liveLayout"]["showRibbon"] = json!(!ui.show_ribbon);
        value["entries"][0]["layout"]["dockingVersion"] = json!(999);
        let before = serde_json::to_value(&ui).map_err(|error| error.to_string())?;
        assert!(Registry::restore(&value, &mut ui).is_err());
        assert_eq!(serde_json::to_value(&ui).map_err(|error| error.to_string())?, before);
        Ok(())
    }
}
