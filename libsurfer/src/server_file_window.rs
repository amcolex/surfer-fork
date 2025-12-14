use egui::{Context, Key, ScrollArea, TextWrapMode, Window};

use crate::{SystemState, message::Message, wave_source::LoadOptions};

impl SystemState {
    pub fn draw_surver_file_window(&self, ctx: &Context, msgs: &mut Vec<Message>) {
        let mut open = true;
        let file_infos = self.user.surver_file_infos.as_ref();
        let file_list: Vec<(String, bool)> = file_infos
            .map(|infos| {
                infos
                    .iter()
                    .map(|info| (format!("{}", info.filename), info.last_load_ok))
                    .collect()
            })
            .unwrap_or_default();

        let mut selected_file_idx = *self.surver_selected_file.borrow();
        let mut should_load = false;

        Window::new("Select wave file")
            .resizable(true)
            .open(&mut open)
            .show(ctx, |ui| {
                ScrollArea::both().id_salt("file_list").show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
                        for (i, (file, can_select)) in file_list.iter().enumerate() {
                            // Only make item selectable if last_load_ok is true
                            ui.add_enabled_ui(*can_select, |ui| {
                                let response = ui.selectable_label(
                                    Some(i) == selected_file_idx && *can_select,
                                    file,
                                );

                                // Handle single click to select
                                if response.clicked() && *can_select {
                                    selected_file_idx = Some(i);
                                    *self.surver_selected_file.borrow_mut() = Some(i);
                                }

                                // Handle double-click to select and load
                                if response.double_clicked() && *can_select {
                                    selected_file_idx = Some(i);
                                    *self.surver_selected_file.borrow_mut() = Some(i);
                                    should_load = true;
                                }
                            });
                        }
                    });
                });

                // Handle keyboard navigation
                if ui.input(|i| i.key_pressed(Key::Escape)) {
                    msgs.push(Message::SetServerFileWindowVisible(false));
                    return;
                }

                if ui.input(|i| i.key_pressed(Key::Enter)) && selected_file_idx.is_some() {
                    should_load = true;
                }

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        msgs.push(Message::SetServerFileWindowVisible(false));
                    }

                    // Disable Select button when nothing is selected
                    ui.add_enabled_ui(selected_file_idx.is_some(), |ui| {
                        if ui.button("Select").clicked() {
                            should_load = true;
                        }
                    });
                });
            });

        // Handle file loading
        if should_load {
            if let Some(file_idx) = selected_file_idx {
                msgs.push(Message::SetServerFileWindowVisible(false));
                msgs.push(Message::LoadAndSetSurverFileIndex(
                    Some(file_idx),
                    LoadOptions::Clear,
                ));
            }
        }

        if !open {
            msgs.push(Message::SetServerFileWindowVisible(false))
        }
    }
}
