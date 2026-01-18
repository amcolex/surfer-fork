//! SUMP Control Panel UI
//!
//! Provides the egui panel for ILA control and status display.

use egui::{
    Align2, Area, Button, CollapsingHeader, Color32, ComboBox, Context, Frame, Grid, Order,
    RichText, ScrollArea, SidePanel, TextEdit, Ui, Vec2,
};
use egui_remixicon::icons;

use super::{ConnectionState, SumpState};
use crate::message::Message;

/// Draw the SUMP control panel on the right side (collapsible)
pub fn draw_sump_panel(ctx: &Context, sump: &mut SumpState, msgs: &mut Vec<Message>) {
    let (min_w, max_w, default_w) = if sump.panel_visible {
        (200.0, 380.0, 280.0)
    } else {
        (36.0, 36.0, 36.0)  // Fixed width when collapsed
    };
    
    let panel_frame = Frame::side_top_panel(&ctx.style())
        .inner_margin(if sump.panel_visible { 8.0 } else { 4.0 })
        .fill(ctx.style().visuals.window_fill);
    
    SidePanel::right("sump_panel")
        .default_width(default_w)
        .min_width(min_w)
        .max_width(max_w)
        .resizable(sump.panel_visible)
        .frame(panel_frame)
        .show(ctx, |ui| {
            // Paint a solid background rect to cover any bleeding graphics
            let rect = ui.max_rect();
            ui.painter().rect_filled(rect, 0.0, ctx.style().visuals.window_fill);
            if sump.panel_visible {
                // Expanded view
                ui.horizontal(|ui| {
                    if ui
                        .button(RichText::new(icons::ARROW_RIGHT_S_LINE).size(18.0))
                        .on_hover_text("Collapse panel")
                        .clicked()
                    {
                        sump.panel_visible = false;
                    }
                    ui.add_space(4.0);
                    ui.label(RichText::new("SUMP3 ILA").size(16.0).strong());

                    // Connection status dot
                    let color = match &sump.connection {
                        ConnectionState::Disconnected => Color32::GRAY,
                        ConnectionState::Connecting => Color32::YELLOW,
                        ConnectionState::Connected => Color32::GREEN,
                        ConnectionState::Error(_) => Color32::RED,
                    };
                    ui.label(RichText::new("●").color(color).size(14.0));
                });
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);
                        draw_panel_contents(ui, sump, msgs);
                    });
            } else {
                // Collapsed view
                ui.vertical_centered(|ui| {
                    ui.add_space(4.0);
                    if ui
                        .button(RichText::new(icons::ARROW_LEFT_S_LINE).size(18.0))
                        .on_hover_text("Expand SUMP panel")
                        .clicked()
                    {
                        sump.panel_visible = true;
                    }
                    ui.add_space(8.0);

                    // Vertical "SUMP" text
                    for c in ['S', 'U', 'M', 'P'] {
                        ui.label(RichText::new(c).size(12.0).strong());
                    }

                    ui.add_space(12.0);

                    // Status indicator
                    let color = match &sump.connection {
                        ConnectionState::Disconnected => Color32::GRAY,
                        ConnectionState::Connecting => Color32::YELLOW,
                        ConnectionState::Connected => Color32::GREEN,
                        ConnectionState::Error(_) => Color32::RED,
                    };
                    ui.label(RichText::new("●").color(color).size(14.0));
                });
            }
        });
}

fn draw_panel_contents(ui: &mut Ui, sump: &mut SumpState, msgs: &mut Vec<Message>) {
    // Connection section
    draw_connection_section(ui, sump, msgs);

    if !sump.is_connected() {
        return;
    }

    ui.add_space(8.0);

    // ILA Info
    CollapsingHeader::new(RichText::new("ILA Status").size(14.0).strong())
        .default_open(true)
        .show(ui, |ui| {
            ui.add_space(4.0);
            draw_status_section(ui, sump, msgs);
        });

    ui.add_space(4.0);

    // Capture Source
    CollapsingHeader::new(RichText::new("Capture Source").size(14.0).strong())
        .default_open(true)
        .show(ui, |ui| {
            ui.add_space(4.0);
            draw_source_section(ui, sump);
        });

    ui.add_space(4.0);

    // Trigger Configuration
    CollapsingHeader::new(RichText::new("Trigger").size(14.0).strong())
        .default_open(true)
        .show(ui, |ui| {
            ui.add_space(4.0);
            draw_trigger_section(ui, sump);
        });

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    // Capture controls
    draw_capture_section(ui, sump, msgs);
}

fn draw_connection_section(ui: &mut Ui, sump: &mut SumpState, msgs: &mut Vec<Message>) {
    let is_connected = sump.is_connected();
    let is_connecting = matches!(sump.connection, ConnectionState::Connecting);

    if let ConnectionState::Error(err) = &sump.connection {
        ui.label(RichText::new(err).color(Color32::RED).size(12.0));
        ui.add_space(4.0);
    }

    if is_connected {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(&sump.server_url)
                    .size(11.0)
                    .monospace()
                    .color(Color32::GRAY),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Disconnect").clicked() {
                    msgs.push(Message::SumpDisconnect);
                }
            });
        });
    } else {
        ui.horizontal(|ui| {
            let response = TextEdit::singleline(&mut sump.url_input)
                .hint_text("http://192.168.2.1:8082")
                .desired_width(170.0)
                .show(ui);

            if response.response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                if !sump.url_input.is_empty() {
                    msgs.push(Message::SumpConnect(sump.url_input.clone()));
                }
            }

            if is_connecting {
                ui.spinner();
            } else if ui.button("Connect").clicked() && !sump.url_input.is_empty() {
                msgs.push(Message::SumpConnect(sump.url_input.clone()));
            }
        });
    }
}

fn draw_status_section(ui: &mut Ui, sump: &mut SumpState, msgs: &mut Vec<Message>) {
    if let Some(info) = &sump.ila_info {
        Grid::new("ila_info_grid")
            .num_columns(2)
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
                ui.label("HW ID");
                ui.label(
                    RichText::new(format!("{} rev{}", info.hw_id, info.revision)).monospace(),
                );
                ui.end_row();

                ui.label("Hubs");
                ui.label(RichText::new(format!("{}", info.hub_count)).monospace());
                ui.end_row();

                ui.label("Address");
                ui.label(RichText::new(&info.base_addr).monospace());
                ui.end_row();
            });

        if let Some(status) = &sump.capture_status {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                status_indicator(ui, "Armed", status.armed);
                ui.add_space(4.0);
                status_indicator(ui, "Trig", status.triggered);
                ui.add_space(4.0);
                status_indicator(ui, "Acq", status.acquired);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(RichText::new(icons::REFRESH_LINE).size(14.0))
                        .on_hover_text("Refresh status")
                        .clicked()
                    {
                        msgs.push(Message::SumpConnect(sump.server_url.clone()));
                    }
                });
            });
        }
    }
}

fn status_indicator(ui: &mut Ui, label: &str, active: bool) {
    let (color, bg) = if active {
        (Color32::WHITE, Color32::from_rgb(34, 139, 34))
    } else {
        (Color32::GRAY, Color32::from_rgb(60, 60, 60))
    };
    Frame::none()
        .fill(bg)
        .inner_margin(4.0)
        .corner_radius(3.0)
        .show(ui, |ui| {
            ui.label(RichText::new(label).color(color).size(11.0));
        });
}

fn draw_source_section(ui: &mut Ui, sump: &mut SumpState) {
    let sources = sump.available_sources();
    if sources.is_empty() {
        ui.label(RichText::new("No sources available").italics());
        return;
    }

    let current_source = sources
        .iter()
        .find(|(h, p, _)| *h == sump.selected_hub && *p == sump.selected_pod)
        .map(|(_, _, name)| name.clone())
        .unwrap_or_else(|| "Select...".to_string());

    ComboBox::from_id_salt("capture_source")
        .selected_text(&current_source)
        .width(ui.available_width() - 8.0)
        .show_ui(ui, |ui: &mut Ui| {
            for (hub, pod, name) in sources {
                if ui
                    .selectable_label(
                        hub == sump.selected_hub && pod == sump.selected_pod,
                        &name,
                    )
                    .clicked()
                {
                    sump.selected_hub = hub;
                    sump.selected_pod = pod;
                    // Set sample count to max for this pod
                    if let Some(info) = &sump.ila_info {
                        if let Some(hub_info) = info.hubs.get(hub as usize) {
                            if let Some(pod_info) = hub_info.pods.get(pod as usize) {
                                sump.sample_count = pod_info.ram_depth;
                            }
                        }
                    }
                }
            }
        });

    if let Some(pod) = sump.selected_pod_info() {
        ui.add_space(4.0);
        ui.label(
            RichText::new(format!(
                "{} | {} bits | {} samples",
                if pod.rle_disable { "Stream" } else { "RLE" },
                pod.data_bits,
                pod.ram_depth
            ))
            .size(11.0)
            .color(Color32::GRAY),
        );

        if !pod.signals.is_empty() {
            ui.add_space(2.0);
            ui.horizontal_wrapped(|ui| {
                for sig in &pod.signals {
                    ui.label(
                        RichText::new(&sig.name)
                            .size(11.0)
                            .monospace()
                            .color(Color32::LIGHT_BLUE),
                    );
                }
            });
        }
    }
}

fn draw_trigger_section(ui: &mut Ui, sump: &mut SumpState) {
    // Trigger type
    ui.horizontal(|ui| {
        ui.label("Type:");
        ComboBox::from_id_salt("trigger_type")
            .selected_text(trigger_type_label(&sump.trigger_config.trigger_type))
            .width(130.0)
            .show_ui(ui, |ui: &mut Ui| {
                for (value, label) in [
                    ("or_rising", "OR Rising"),
                    ("or_falling", "OR Falling"),
                    ("external", "External"),
                ] {
                    if ui
                        .selectable_label(sump.trigger_config.trigger_type == value, label)
                        .clicked()
                    {
                        sump.trigger_config.trigger_type = value.to_string();
                    }
                }
            });
    });

    ui.add_space(6.0);

    // Trigger bits - show as signal toggles if we have signal info
    if let Some(pod) = sump.selected_pod_info().cloned() {
        if !pod.signals.is_empty() {
            ui.label(RichText::new("Trigger Bits:").size(12.0));
            ui.add_space(4.0);

            // Show each signal with bit toggles
            for sig in &pod.signals {
                let width = sig.bit_high - sig.bit_low + 1;
                
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(&sig.name)
                            .size(11.0)
                            .monospace()
                            .color(Color32::LIGHT_BLUE),
                    );
                });

                // Show bit toggles in a row
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    
                    // Show bits from high to low
                    for bit_offset in (0..width).rev() {
                        let actual_bit = sig.bit_low + bit_offset;
                        let is_set = (sump.trigger_config.trigger_bits >> actual_bit) & 1 == 1;

                        let btn = Button::new(
                            RichText::new(format!("{}", bit_offset))
                                .size(10.0)
                                .monospace()
                                .color(if is_set { Color32::WHITE } else { Color32::GRAY }),
                        )
                        .fill(if is_set {
                            Color32::from_rgb(70, 130, 180)
                        } else {
                            Color32::from_rgb(50, 50, 50)
                        })
                        .min_size(Vec2::new(18.0, 18.0));

                        if ui.add(btn).on_hover_text(format!("Bit {} (global {})", bit_offset, actual_bit)).clicked() {
                            sump.trigger_config.trigger_bits ^= 1 << actual_bit;
                            // Update hex input
                            sump.trigger_bits_input = format!("{:08x}", sump.trigger_config.trigger_bits);
                        }
                    }
                });
                ui.add_space(4.0);
            }

            // Show hex value (read-only display)
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Hex:").size(11.0).color(Color32::GRAY));
                ui.label(
                    RichText::new(format!("0x{:08x}", sump.trigger_config.trigger_bits))
                        .monospace()
                        .size(11.0),
                );
            });
        } else {
            // Fallback to hex input if no signal info
            draw_hex_trigger_input(ui, sump);
        }
    } else {
        draw_hex_trigger_input(ui, sump);
    }

    ui.add_space(6.0);

    // Post-trigger and samples in a grid
    Grid::new("trigger_params")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label("Post-trigger:");
            ui.add(
                egui::DragValue::new(&mut sump.trigger_config.post_trigger)
                    .range(1..=512)
                    .speed(1),
            );
            ui.end_row();
        });
}

fn draw_hex_trigger_input(ui: &mut Ui, sump: &mut SumpState) {
    ui.horizontal(|ui| {
        ui.label("Bits (hex):");
        let response = TextEdit::singleline(&mut sump.trigger_bits_input)
            .desired_width(100.0)
            .font(egui::TextStyle::Monospace)
            .show(ui);

        if response.response.changed() {
            if let Ok(bits) =
                u32::from_str_radix(sump.trigger_bits_input.trim_start_matches("0x"), 16)
            {
                sump.trigger_config.trigger_bits = bits;
            }
        }
    });
}

fn trigger_type_label(t: &str) -> &str {
    match t {
        "or_rising" => "OR Rising",
        "or_falling" => "OR Falling",
        "external" => "External",
        _ => "Unknown",
    }
}

fn draw_capture_section(ui: &mut Ui, sump: &mut SumpState, msgs: &mut Vec<Message>) {
    // Get max samples from selected pod
    let max_samples = sump.selected_pod_info()
        .map(|p| p.ram_depth)
        .unwrap_or(4096);
    
    ui.horizontal(|ui| {
        ui.label("Samples:");
        ui.add(
            egui::DragValue::new(&mut sump.sample_count)
                .range(8..=max_samples)
                .speed(8),
        );
        ui.label(RichText::new(format!("/ {}", max_samples)).size(11.0).color(Color32::GRAY));
    });

    ui.add_space(8.0);

    ui.horizontal(|ui| {
        // Main capture button
        let capture_btn = Button::new(
            RichText::new(format!("{} Capture", icons::FLASHLIGHT_LINE))
                .color(Color32::WHITE)
                .size(14.0),
        )
        .fill(Color32::from_rgb(34, 139, 34))
        .min_size(Vec2::new(100.0, 28.0));

        if ui.add(capture_btn).clicked() {
            msgs.push(Message::SumpCapture(
                sump.selected_hub,
                sump.selected_pod,
                sump.sample_count,
            ));
        }

        ui.add_space(8.0);

        if ui
            .button(RichText::new(icons::RESTART_LINE).size(16.0))
            .on_hover_text("Reset ILA")
            .clicked()
        {
            msgs.push(Message::SumpReset);
        }

        if ui
            .button(RichText::new(icons::PLAY_LINE).size(16.0))
            .on_hover_text("Init RAM")
            .clicked()
        {
            msgs.push(Message::SumpInit);
        }
    });

    // Last capture info
    if let Some(capture) = &sump.capture_data {
        ui.add_space(8.0);
        let hub_info = sump.selected_hub_info();
        let freq = hub_info.map(|h| h.freq_mhz).unwrap_or(100);

        ui.label(
            RichText::new(format!("{} samples @ {} MHz", capture.samples.len(), freq))
                .monospace(),
        );

        if !capture.samples.is_empty() {
            let min_ts = capture.samples.iter().map(|s| s.timestamp).min().unwrap_or(0);
            let max_ts = capture.samples.iter().map(|s| s.timestamp).max().unwrap_or(0);
            let span = max_ts - min_ts;
            let span_ns = (span as u64 * 1000) / freq as u64;

            ui.label(
                RichText::new(format!("{} clocks ({} ns)", span, span_ns))
                    .size(11.0)
                    .color(Color32::GRAY),
            );
        }
    }
}
