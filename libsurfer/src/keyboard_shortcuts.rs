use egui::{KeyboardShortcut, ModifierNames, Modifiers};
use eyre::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::SystemState;
use crate::message::Message;

// Table-driven dispatch action enum
#[derive(Clone, Copy, Debug)]
pub enum ShortcutAction {
    OpenFile,
    SwitchFile,
    Redo,
    Undo,
    ToggleSidePanel,
    ToggleToolbar,
    GoToEnd,
    GoToStart,
    SaveStateFile,
    GoToTop,
    GoToBottom,
    ItemFocus,
    GroupNew,
    SelectAll,
    SelectToggle,
    ReloadWaveform,
    ZoomIn,
    ZoomOut,
    UiZoomIn,
    UiZoomOut,
}

// Cached dispatch table entry: (action, modifier_priority)
#[derive(Clone, Debug)]
struct DispatchEntry {
    action: ShortcutAction,
    priority: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SurferShortcuts {
    #[serde(with = "keyboard_shortcuts_serde")]
    pub open_file: Vec<KeyboardShortcut>,
    #[serde(with = "keyboard_shortcuts_serde")]
    pub switch_file: Vec<KeyboardShortcut>,
    #[serde(with = "keyboard_shortcuts_serde")]
    pub undo: Vec<KeyboardShortcut>,
    #[serde(with = "keyboard_shortcuts_serde")]
    pub redo: Vec<KeyboardShortcut>,
    #[serde(with = "keyboard_shortcuts_serde")]
    pub toggle_side_panel: Vec<KeyboardShortcut>,
    #[serde(with = "keyboard_shortcuts_serde")]
    pub toggle_toolbar: Vec<KeyboardShortcut>,
    #[serde(with = "keyboard_shortcuts_serde")]
    pub goto_end: Vec<KeyboardShortcut>,
    #[serde(with = "keyboard_shortcuts_serde")]
    pub goto_start: Vec<KeyboardShortcut>,
    #[serde(with = "keyboard_shortcuts_serde")]
    pub save_state_file: Vec<KeyboardShortcut>,
    #[serde(with = "keyboard_shortcuts_serde")]
    pub goto_top: Vec<KeyboardShortcut>,
    #[serde(with = "keyboard_shortcuts_serde")]
    pub goto_bottom: Vec<KeyboardShortcut>,
    #[serde(with = "keyboard_shortcuts_serde")]
    pub group_new: Vec<KeyboardShortcut>,
    #[serde(with = "keyboard_shortcuts_serde")]
    pub item_focus: Vec<KeyboardShortcut>,
    #[serde(with = "keyboard_shortcuts_serde")]
    pub select_all: Vec<KeyboardShortcut>,
    #[serde(with = "keyboard_shortcuts_serde")]
    pub select_toggle: Vec<KeyboardShortcut>,
    #[serde(with = "keyboard_shortcuts_serde")]
    pub reload_waveform: Vec<KeyboardShortcut>,
    #[serde(with = "keyboard_shortcuts_serde")]
    pub zoom_in: Vec<KeyboardShortcut>,
    #[serde(with = "keyboard_shortcuts_serde")]
    pub zoom_out: Vec<KeyboardShortcut>,
    #[serde(with = "keyboard_shortcuts_serde")]
    pub ui_zoom_in: Vec<KeyboardShortcut>,
    #[serde(with = "keyboard_shortcuts_serde")]
    pub ui_zoom_out: Vec<KeyboardShortcut>,
    #[serde(skip)]
    cached_dispatch_table: Vec<DispatchEntry>,
}

impl SurferShortcuts {
    #[cfg(target_arch = "wasm32")]
    pub fn new(_force_default_config: bool) -> Result<Self> {
        Self::new_from_toml(&include_str!("../../default_shortcuts.toml"))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(force_default_config: bool) -> Result<Self> {
        use config::Config;
        use eyre::anyhow;

        let default_config = String::from(include_str!("../../default_shortcuts.toml"));
        let mut config = Config::builder().add_source(config::File::from_str(
            &default_config,
            config::FileFormat::Toml,
        ));
        let config = if !force_default_config {
            use config::{Environment, File};
            use directories::ProjectDirs;

            use crate::config::find_local_configs;

            if let Some(proj_dirs) = ProjectDirs::from("org", "surfer-project", "surfer") {
                let config_file = proj_dirs.config_dir().join("shortcuts.toml");
                config = config.add_source(File::from(config_file).required(false));
            }

            // `surfer.toml` will not be searched for upward, as it is deprecated.
            config = config.add_source(File::from(Path::new("surfer.toml")).required(false));

            // Add configs from most top-level to most local. This allows overwriting of
            // higher-level settings with a local `.surfer` directory.
            find_local_configs()
                .into_iter()
                .fold(config, |c, p| {
                    c.add_source(File::from(p.join("shortcuts.toml")).required(false))
                })
                .add_source(Environment::with_prefix("surfer")) // Add environment finally
        } else {
            config
        };

        config
            .build()?
            .try_deserialize::<Self>()
            .map(|mut shortcuts| {
                shortcuts.cached_dispatch_table = shortcuts.build_dispatch_table();
                shortcuts
            })
            .map_err(|e| anyhow!("Failed to parse config {e}"))
    }

    pub fn new_from_toml(config: &str) -> Result<Self> {
        let mut shortcuts: Self = toml::from_str(config)?;
        shortcuts.cached_dispatch_table = shortcuts.build_dispatch_table();
        Ok(shortcuts)
    }

    pub fn pressed(&self, ctx: &egui::Context, shortcuts: &[KeyboardShortcut]) -> bool {
        shortcuts
            .iter()
            .any(|shortcut| ctx.input_mut(|i| i.consume_shortcut(shortcut)))
    }

    pub fn save(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let toml_string = toml::to_string_pretty(&self)?;
        fs::write(path, toml_string)?;
        Ok(())
    }

    fn modifier_priority(shortcuts: &[KeyboardShortcut]) -> u8 {
        shortcuts
            .iter()
            .find_map(|shortcut| {
                let has_shift = shortcut.modifiers.contains(Modifiers::SHIFT);
                let has_alt = shortcut.modifiers.contains(Modifiers::ALT);

                match (has_shift, has_alt) {
                    (true, true) => Some(0), // Shift+Alt highest priority
                    (_, true) => Some(1),    // Alt second priority
                    (true, _) => Some(2),    // Shift third priority
                    _ => None,
                }
            })
            .unwrap_or(3) // Rest lowest priority
    }

    pub fn format_shortcut(&self, action: ShortcutAction) -> String {
        self.shortcuts_for_action(action)
            .iter()
            .map(|kb| kb.format(&ModifierNames::NAMES, false))
            .collect::<Vec<String>>()
            .join("/")
    }

    fn build_dispatch_table(&self) -> Vec<DispatchEntry> {
        // Pre-allocate with known capacity and build entries
        let mut dispatch_table = Vec::with_capacity(10);

        // Create entry for each action with its priority
        dispatch_table.extend_from_slice(&[
            DispatchEntry {
                action: ShortcutAction::OpenFile,
                priority: Self::modifier_priority(&self.open_file),
            },
            DispatchEntry {
                action: ShortcutAction::SwitchFile,
                priority: Self::modifier_priority(&self.switch_file),
            },
            DispatchEntry {
                action: ShortcutAction::Redo,
                priority: Self::modifier_priority(&self.redo),
            },
            DispatchEntry {
                action: ShortcutAction::Undo,
                priority: Self::modifier_priority(&self.undo),
            },
            DispatchEntry {
                action: ShortcutAction::ToggleSidePanel,
                priority: Self::modifier_priority(&self.toggle_side_panel),
            },
            DispatchEntry {
                action: ShortcutAction::ToggleToolbar,
                priority: Self::modifier_priority(&self.toggle_toolbar),
            },
            DispatchEntry {
                action: ShortcutAction::GoToEnd,
                priority: Self::modifier_priority(&self.goto_end),
            },
            DispatchEntry {
                action: ShortcutAction::GoToStart,
                priority: Self::modifier_priority(&self.goto_start),
            },
            DispatchEntry {
                action: ShortcutAction::SaveStateFile,
                priority: Self::modifier_priority(&self.save_state_file),
            },
            DispatchEntry {
                action: ShortcutAction::GoToTop,
                priority: Self::modifier_priority(&self.goto_top),
            },
            DispatchEntry {
                action: ShortcutAction::GoToBottom,
                priority: Self::modifier_priority(&self.goto_bottom),
            },
            DispatchEntry {
                action: ShortcutAction::GroupNew,
                priority: Self::modifier_priority(&self.group_new),
            },
            DispatchEntry {
                action: ShortcutAction::ItemFocus,
                priority: Self::modifier_priority(&self.item_focus),
            },
            DispatchEntry {
                action: ShortcutAction::SelectAll,
                priority: Self::modifier_priority(&self.select_all),
            },
            DispatchEntry {
                action: ShortcutAction::SelectToggle,
                priority: Self::modifier_priority(&self.select_toggle),
            },
            DispatchEntry {
                action: ShortcutAction::ReloadWaveform,
                priority: Self::modifier_priority(&self.reload_waveform),
            },
            DispatchEntry {
                action: ShortcutAction::ZoomIn,
                priority: Self::modifier_priority(&self.zoom_in),
            },
            DispatchEntry {
                action: ShortcutAction::ZoomOut,
                priority: Self::modifier_priority(&self.zoom_out),
            },
            DispatchEntry {
                action: ShortcutAction::UiZoomIn,
                priority: Self::modifier_priority(&self.ui_zoom_in),
            },
            DispatchEntry {
                action: ShortcutAction::UiZoomOut,
                priority: Self::modifier_priority(&self.ui_zoom_out),
            },
        ]);

        // Sort by modifier priority (lower number = higher priority)
        dispatch_table.sort_by_key(|entry| entry.priority);
        dispatch_table
    }

    fn shortcuts_for_action(&self, action: ShortcutAction) -> &[KeyboardShortcut] {
        match action {
            ShortcutAction::OpenFile => &self.open_file,
            ShortcutAction::SwitchFile => &self.switch_file,
            ShortcutAction::Undo => &self.undo,
            ShortcutAction::Redo => &self.redo,
            ShortcutAction::ToggleSidePanel => &self.toggle_side_panel,
            ShortcutAction::ToggleToolbar => &self.toggle_toolbar,
            ShortcutAction::GoToEnd => &self.goto_end,
            ShortcutAction::GoToStart => &self.goto_start,
            ShortcutAction::SaveStateFile => &self.save_state_file,
            ShortcutAction::GoToTop => &self.goto_top,
            ShortcutAction::GoToBottom => &self.goto_bottom,
            ShortcutAction::ItemFocus => &self.item_focus,
            ShortcutAction::GroupNew => &self.group_new,
            ShortcutAction::SelectAll => &self.select_all,
            ShortcutAction::SelectToggle => &self.select_toggle,
            ShortcutAction::ReloadWaveform => &self.reload_waveform,
            ShortcutAction::ZoomIn => &self.zoom_in,
            ShortcutAction::ZoomOut => &self.zoom_out,
            ShortcutAction::UiZoomIn => &self.ui_zoom_in,
            ShortcutAction::UiZoomOut => &self.ui_zoom_out,
        }
    }

    fn execute_action(&self, action: ShortcutAction, msgs: &mut Vec<Message>, state: &SystemState) {
        match action {
            ShortcutAction::OpenFile => {
                msgs.push(Message::OpenFileDialog(crate::file_dialog::OpenMode::Open));
            }
            ShortcutAction::SwitchFile => {
                msgs.push(Message::OpenFileDialog(
                    crate::file_dialog::OpenMode::Switch,
                ));
            }
            ShortcutAction::Redo => {
                msgs.push(Message::Redo(state.get_count()));
            }
            ShortcutAction::Undo => {
                msgs.push(Message::Undo(state.get_count()));
            }
            ShortcutAction::ToggleSidePanel => {
                msgs.push(Message::SetSidePanelVisible(!state.show_hierarchy()));
            }
            ShortcutAction::ToggleToolbar => {
                msgs.push(Message::SetToolbarVisible(!state.show_toolbar()));
            }
            ShortcutAction::GoToEnd => {
                msgs.push(Message::GoToEnd { viewport_idx: 0 });
            }
            ShortcutAction::GoToStart => {
                msgs.push(Message::GoToStart { viewport_idx: 0 });
            }
            ShortcutAction::SaveStateFile => {
                msgs.push(Message::SaveStateFile(state.user.state_file.clone()));
            }
            ShortcutAction::GoToTop => {
                msgs.push(Message::ScrollToItem(0));
            }
            ShortcutAction::GoToBottom => {
                if let Some(waves) = &state.user.waves {
                    if waves.displayed_items.len() > 1 {
                        msgs.push(Message::ScrollToItem(waves.displayed_items.len() - 1));
                    }
                }
            }
            ShortcutAction::GroupNew => {
                msgs.push(Message::GroupNew {
                    name: None,
                    before: None,
                    items: None,
                });
                msgs.push(Message::ShowCommandPrompt("item_rename ".to_owned(), None));
            }
            ShortcutAction::ItemFocus => {
                msgs.push(Message::ShowCommandPrompt("item_focus ".to_string(), None));
            }
            ShortcutAction::SelectAll => {
                msgs.push(Message::ItemSelectAll);
            }
            ShortcutAction::SelectToggle => {
                msgs.push(Message::ToggleItemSelected(None));
            }
            ShortcutAction::ReloadWaveform => {
                msgs.push(Message::ReloadWaveform(
                    state.user.config.behavior.keep_during_reload,
                ));
            }
            ShortcutAction::ZoomIn => {
                msgs.push(Message::CanvasZoom {
                    mouse_ptr: None,
                    delta: 0.5,
                    viewport_idx: 0,
                });
            }
            ShortcutAction::ZoomOut => {
                msgs.push(Message::CanvasZoom {
                    mouse_ptr: None,
                    delta: 2.0,
                    viewport_idx: 0,
                });
            }
            ShortcutAction::UiZoomIn => {
                let mut next_factor = 0f32;
                for factor in &state.user.config.layout.zoom_factors {
                    if *factor < state.ui_zoom_factor() && *factor > next_factor {
                        next_factor = *factor;
                    }
                }
                if next_factor > 0f32 {
                    msgs.push(Message::SetUIZoomFactor(next_factor));
                }
            }
            ShortcutAction::UiZoomOut => {
                let mut next_factor = 1f32;
                for factor in &state.user.config.layout.zoom_factors {
                    if *factor > state.ui_zoom_factor() && *factor < next_factor {
                        next_factor = *factor;
                    }
                }
                if next_factor < 1f32 {
                    msgs.push(Message::SetUIZoomFactor(next_factor));
                }
            }
        }
    }

    pub fn update(&self, ctx: &egui::Context, msgs: &mut Vec<Message>, state: &SystemState) {
        // Execute actions matching pressed shortcuts using cached dispatch table
        for entry in &self.cached_dispatch_table {
            let shortcuts = self.shortcuts_for_action(entry.action);

            if self.pressed(ctx, shortcuts) {
                self.execute_action(entry.action, msgs, state);
            }
        }
    }
}

impl Default for SurferShortcuts {
    fn default() -> Self {
        Self::new(false).expect("Failed to load default config")
    }
}

// Custom serialization/deserialization for Vec<KeyboardShortcut>
mod keyboard_shortcuts_serde {
    use egui::Key;
    use serde::{Deserializer, Serializer};

    use super::*;

    pub fn serialize<S>(shortcuts: &Vec<KeyboardShortcut>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let bindings: Vec<String> = shortcuts
            .iter()
            .map(|s| format_binding(s.modifiers, s.logical_key))
            .collect();
        bindings.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<KeyboardShortcut>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bindings: Vec<String> = Vec::deserialize(deserializer)?;
        bindings
            .iter()
            .map(|s| parse_binding(s).map_err(serde::de::Error::custom))
            .collect()
    }

    fn format_binding(modifiers: Modifiers, logical_key: Key) -> String {
        const MODIFIER_NAMES: &[(Modifiers, &str)] = &[
            (Modifiers::CTRL, "Ctrl"),
            (Modifiers::SHIFT, "Shift"),
            (Modifiers::ALT, "Alt"),
            (Modifiers::MAC_CMD, "Mac_cmd"),
            (Modifiers::COMMAND, "Command"),
        ];

        // Pre-allocate with capacity for max 6 items (5 modifiers + key)
        let mut parts = Vec::with_capacity(6);

        for (modifier, name) in MODIFIER_NAMES {
            if modifiers.contains(*modifier) {
                parts.push(*name);
            }
        }
        let key_name = format!("{:?}", logical_key);
        parts.push(&key_name);
        parts.join("+")
    }

    fn parse_binding(binding: &str) -> Result<KeyboardShortcut, String> {
        const MODIFIER_MAP: &[(&str, Modifiers)] = &[
            ("ctrl", Modifiers::CTRL),
            ("shift", Modifiers::SHIFT),
            ("alt", Modifiers::ALT),
            ("mac_cmd", Modifiers::MAC_CMD),
            ("command", Modifiers::COMMAND),
            ("cmd", Modifiers::COMMAND),
        ];

        let parts: Vec<&str> = binding.split('+').map(|s| s.trim()).collect();

        // Use slice pattern to extract key and modifiers
        let (modifier_parts, key_str) = match parts.as_slice() {
            [modifiers @ .., key] => (modifiers, *key),
            [] => return Err("Empty binding".to_string()),
        };

        let logical_key =
            Key::from_name(key_str).ok_or_else(|| format!("Unknown key: {}", key_str))?;

        // Use fold to accumulate modifiers
        let modifiers = modifier_parts
            .iter()
            .try_fold(Modifiers::NONE, |acc, &modifier_str| {
                let lower = modifier_str.to_lowercase();
                MODIFIER_MAP
                    .iter()
                    .find(|(name, _)| name == &lower)
                    .map(|(_, mod_bit)| acc | *mod_bit)
                    .ok_or_else(|| format!("Unknown modifier: {}", modifier_str))
            })?;

        Ok(KeyboardShortcut::new(modifiers, logical_key))
    }
}
