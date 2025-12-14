use egui::{Context, ScrollArea, TextWrapMode, Window};

use crate::{SystemState, message::Message, wave_source::LoadOptions};

impl SystemState {
    pub fn draw_surver_file_window(&self, ctx: &Context, msgs: &mut Vec<Message>) {
        let mut open = true;
        let file_list = if let Some(file_infos) = &self.user.surver_file_infos {
            file_infos
                .iter()
                .map(|info| format!("{}", info.filename))
                .collect::<Vec<_>>()
        } else {
            vec![]
        };

        let selected_file_idx = *self.surver_selected_file.borrow();

        Window::new("Select wave file")
            .resizable(true)
            .open(&mut open)
            .show(ctx, |ui| {
                ScrollArea::both().id_salt("file_list").show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
                        for (i, file) in file_list.iter().enumerate() {
                            if ui
                                .selectable_label(Some(i) == selected_file_idx, file)
                                .clicked()
                            {
                                *self.surver_selected_file.borrow_mut() = Some(i);
                            }
                        }
                    });
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        msgs.push(Message::SetServerFileWindowVisible(false));
                    }
                    if ui.button("Select").clicked() {
                        if let Some(file_idx) = *self.surver_selected_file.borrow() {
                            msgs.push(Message::SetServerFileWindowVisible(false));
                            msgs.push(Message::LoadAndSetSurverFileIndex(
                                Some(file_idx),
                                LoadOptions::Clear,
                            ));
                        }
                    }
                });
            });
        if !open {
            msgs.push(Message::SetServerFileWindowVisible(false))
        }
    }
}
