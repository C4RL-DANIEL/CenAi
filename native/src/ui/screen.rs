//! Native CenAI screens. No WebView or Compose UI is used in this module.
use crate::ui::model::{AppState, Destination, MessageRole};

fn accent() -> egui::Color32 {
    egui::Color32::from_rgb(139, 155, 255)
}

fn text(value: impl Into<String>, size: f32, color: egui::Color32) -> egui::RichText {
    egui::RichText::new(value).size(size).color(color)
}

fn surface<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> egui::InnerResponse<R> {
    egui::Frame::new()
        .fill(egui::Color32::from_rgba_unmultiplied(24, 29, 49, 232))
        .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(53, 62, 91)))
        .inner_margin(egui::Margin::same(16))
        .corner_radius(egui::CornerRadius::same(20))
        .show(ui, add)
}

fn background(ctx: &egui::Context, time: f32) {
    let painter = ctx.layer_painter(egui::LayerId::background());
    let rect = ctx.input(|input| input.screen_rect());
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(8, 11, 22));
    let drift = (time * 0.16).sin();
    let glow_a = egui::pos2(rect.width() * (0.22 + drift * 0.06), rect.height() * 0.22);
    let glow_b = egui::pos2(rect.width() * 0.84, rect.height() * (0.65 + drift * 0.04));
    for (center, color) in [
        (glow_a, egui::Color32::from_rgba_unmultiplied(73, 94, 226, 20)),
        (glow_b, egui::Color32::from_rgba_unmultiplied(142, 65, 210, 18)),
    ] {
        for layer in (1..=8).rev() {
            let radius = rect.width().min(rect.height()) * (0.06 + layer as f32 * 0.035);
            let alpha = (10.0 - layer as f32).max(1.0) as u8;
            painter.circle_filled(center, radius, egui::Color32::from_rgba_unmultiplied(
                color.r(), color.g(), color.b(), alpha,
            ));
        }
    }
}

fn top_bar(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(text("Cen", 23.0, egui::Color32::WHITE).strong());
        ui.label(text("AI", 23.0, accent()).strong());
        ui.add_space(8.0);
        ui.label(text("PERSONAL INTELLIGENCE", 9.0, egui::Color32::from_rgb(139, 151, 184)));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(text("●  READY", 10.0, egui::Color32::from_rgb(119, 220, 183)));
        });
    });
}

fn navigation(ui: &mut egui::Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        let items = [
            (Destination::Conversation, "◉", "Chat"),
            (Destination::YourWork, "▦", "Work"),
            (Destination::Connections, "⌘", "Connect"),
            (Destination::Settings, "⚙", "Settings"),
        ];
        for (destination, glyph, label) in items {
            let selected = state.destination == destination;
            let fill = if selected {
                egui::Color32::from_rgb(49, 59, 104)
            } else {
                egui::Color32::TRANSPARENT
            };
            let color = if selected { egui::Color32::WHITE } else { egui::Color32::from_rgb(145, 154, 180) };
            if ui.add_sized(
                [ui.available_width() / 4.0, 54.0],
                egui::Button::new(egui::RichText::new(format!("{glyph}\n{label}")).size(11.0).color(color))
                    .fill(fill)
                    .stroke(egui::Stroke::NONE),
            ).clicked() {
                state.destination = destination;
            }
        }
    });
}

fn markdown(ui: &mut egui::Ui, source: &str, user_message: bool) {
    if user_message {
        ui.label(text(source, 14.0, egui::Color32::from_rgb(231, 235, 247)));
        return;
    }
    let mut code_mode = false;
    let mut code = String::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            if code_mode {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(10, 14, 26))
                    .inner_margin(egui::Margin::same(10))
                    .corner_radius(egui::CornerRadius::same(10))
                    .show(ui, |ui| {
                        ui.add(egui::Label::new(egui::RichText::new(code.trim_end()).monospace().size(12.0).color(egui::Color32::from_rgb(190, 211, 255))).wrap());
                    });
                code.clear();
                code_mode = false;
            } else {
                code_mode = true;
            }
            continue;
        }
        if code_mode {
            code.push_str(line);
            code.push('\n');
        } else if let Some(heading) = trimmed.strip_prefix("### ") {
            ui.label(text(heading, 16.0, egui::Color32::WHITE).strong());
        } else if let Some(heading) = trimmed.strip_prefix("## ") {
            ui.label(text(heading, 19.0, egui::Color32::WHITE).strong());
        } else if let Some(heading) = trimmed.strip_prefix("# ") {
            ui.label(text(heading, 22.0, egui::Color32::WHITE).strong());
        } else if let Some(item) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
            ui.horizontal(|ui| {
                ui.label(text("•", 14.0, accent()));
                ui.label(text(item, 13.0, egui::Color32::from_rgb(224, 230, 245)));
            });
        } else if trimmed.is_empty() {
            ui.add_space(4.0);
        } else {
            ui.label(text(trimmed, 13.0, egui::Color32::from_rgb(224, 230, 245)));
        }
    }
    if code_mode && !code.is_empty() {
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(10, 14, 26))
            .inner_margin(egui::Margin::same(10))
            .corner_radius(egui::CornerRadius::same(10))
            .show(ui, |ui| {
                ui.add(egui::Label::new(egui::RichText::new(code).monospace().size(12.0).color(egui::Color32::from_rgb(190, 211, 255))).wrap());
            });
    }
}

fn conversation(ui: &mut egui::Ui, state: &mut AppState) {
    ui.add_space(8.0);
    ui.label(text("A little more than a chat.", 25.0, egui::Color32::WHITE).strong());
    ui.label(text("Think it through. Build it. Get it done.", 13.0, egui::Color32::from_rgb(163, 174, 203)));
    ui.add_space(16.0);

    if state.conversation.messages().next().is_none() {
        surface(ui, |ui| {
            ui.label(text("What would you like to work on?", 17.0, egui::Color32::WHITE).strong());
            ui.add_space(6.0);
            ui.label(text("CenAI can help you research, plan, create, code and manage ongoing work.", 12.0, egui::Color32::from_rgb(166, 178, 207)));
            ui.add_space(12.0);
            ui.horizontal_wrapped(|ui| {
                for prompt in ["Research a topic", "Plan a project", "Help me build an app"] {
                    if ui.button(prompt).clicked() {
                        state.conversation.draft = prompt.to_owned();
                    }
                }
            });
        });
    } else {
        egui::ScrollArea::vertical().stick_to_bottom(true).max_height(ui.available_height() - 104.0).show(ui, |ui| {
            for message in state.conversation.messages() {
                let is_user = message.role == MessageRole::User;
                surface(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(text(if is_user { "YOU" } else { "CENAI" }, 9.0, if is_user { accent() } else { egui::Color32::from_rgb(119, 220, 183) }).strong());
                        if message.streaming {
                            ui.label(text("GENERATING", 9.0, egui::Color32::from_rgb(183, 168, 255)));
                        }
                    });
                    markdown(ui, &message.body, is_user);
                });
                ui.add_space(8.0);
            }
        });
    }

    ui.add_space(10.0);
    surface(ui, |ui| {
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut state.conversation.draft)
                .hint_text("Message CenAI…")
                .desired_width(ui.available_width() - 82.0)
                .margin(egui::vec2(8.0, 12.0)));
            let can_send = !state.conversation.draft.trim().is_empty() && !state.conversation.sending;
            if ui.add_enabled(can_send, egui::Button::new(text("Send  ↑", 13.0, egui::Color32::WHITE))
                .fill(egui::Color32::from_rgb(86, 103, 207))).clicked() {
                state.conversation.submit_draft();
            }
        });
        ui.label(text("Enter to send  ·  Your work stays yours", 9.0, egui::Color32::from_rgb(124, 136, 165)));
    });
}

fn work(ui: &mut egui::Ui, state: &mut AppState) {
    ui.add_space(10.0);
    ui.label(text("Your Work", 26.0, egui::Color32::WHITE).strong());
    ui.label(text("Projects, files and tasks in one persistent space.", 13.0, egui::Color32::from_rgb(163, 174, 203)));
    ui.add_space(18.0);
    surface(ui, |ui| {
        ui.label(text("Create a project", 17.0, egui::Color32::WHITE).strong());
        ui.add(egui::TextEdit::singleline(&mut state.project_name_draft).hint_text("Project name").desired_width(f32::INFINITY));
        ui.add(egui::TextEdit::multiline(&mut state.project_description_draft).hint_text("What are you working on?").desired_width(f32::INFINITY).desired_rows(2));
        if ui.button("Create project").clicked() {
            state.pending_project_create = Some((state.project_name_draft.trim().to_owned(), state.project_description_draft.trim().to_owned()));
            state.project_status = "Saving project…".into();
        }
        if !state.project_status.is_empty() {
            ui.label(text(&state.project_status, 10.0, egui::Color32::from_rgb(119, 220, 183)));
        }
    });
    ui.add_space(10.0);
    if state.projects.is_empty() {
        surface(ui, |ui| {
            ui.label(text("Projects", 17.0, egui::Color32::WHITE).strong());
            ui.label(text("Your saved projects will appear here.", 12.0, egui::Color32::from_rgb(163, 174, 203)));
        });
    } else {
        for project in &state.projects {
            surface(ui, |ui| {
                ui.label(text(&project.name, 16.0, egui::Color32::WHITE).strong());
                if !project.description.is_empty() {
                    ui.label(text(&project.description, 12.0, egui::Color32::from_rgb(163, 174, 203)));
                }
                ui.label(text(format!("{} saved artifacts", project.artifacts.len()), 10.0, egui::Color32::from_rgb(140, 154, 190)));
            });
            ui.add_space(8.0);
        }
    }
    ui.add_space(10.0);
    surface(ui, |ui| {
        ui.label(text("Recent activity", 17.0, egui::Color32::WHITE).strong());
        ui.label(text("No saved activity yet.", 12.0, egui::Color32::from_rgb(163, 174, 203)));
    });
    ui.add_space(10.0);
    surface(ui, |ui| {
        ui.label(text("Files and artifacts", 17.0, egui::Color32::WHITE).strong());
        ui.label(text("Documents, generated files and checkpoints will be organized here.", 12.0, egui::Color32::from_rgb(163, 174, 203)));
    });
}

fn connections(ui: &mut egui::Ui) {
    ui.add_space(10.0);
    ui.label(text("Connections", 26.0, egui::Color32::WHITE).strong());
    ui.label(text("Bring your models, tools and services together.", 13.0, egui::Color32::from_rgb(163, 174, 203)));
    ui.add_space(18.0);
    for (name, description, status) in [
        ("Cloud AI", "CenAI managed model routing", "Backend integration pending"),
        ("Your API keys", "Use your own provider credentials", "Secure vault integration pending"),
        ("MCP tools", "Discover and authorize external capabilities", "Permission system in progress"),
    ] {
        surface(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(text(name, 16.0, egui::Color32::WHITE).strong());
                    ui.label(text(description, 12.0, egui::Color32::from_rgb(163, 174, 203)));
                    ui.label(text(status, 10.0, egui::Color32::from_rgb(220, 184, 121)));
                });
            });
        });
        ui.add_space(8.0);
    }
}

fn settings(ui: &mut egui::Ui, state: &mut AppState) {
    ui.add_space(10.0);
    ui.label(text("Settings", 26.0, egui::Color32::WHITE).strong());
    ui.label(text("Make CenAI feel right for you.", 13.0, egui::Color32::from_rgb(163, 174, 203)));
    ui.add_space(18.0);
    surface(ui, |ui| {
        ui.label(text("Appearance & accessibility", 16.0, egui::Color32::WHITE).strong());
        ui.checkbox(&mut state.reduced_motion, "Reduce interface motion");
        ui.label(text("The interface respects this setting when animating transitions.", 11.0, egui::Color32::from_rgb(163, 174, 203)));
    });
    ui.add_space(8.0);
    surface(ui, |ui| {
        ui.label(text("AI provider keys", 16.0, egui::Color32::WHITE).strong());
        ui.label(text("Your key is encrypted with Android Keystore and stays on this device.", 11.0, egui::Color32::from_rgb(163, 174, 203)));
        egui::ComboBox::from_id_salt("cenai_provider_key")
            .selected_text(match state.provider_key_provider.as_str() {
                "deepseek" => "DeepSeek",
                _ => "OpenAI-compatible",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut state.provider_key_provider, "openai".into(), "OpenAI-compatible");
                ui.selectable_value(&mut state.provider_key_provider, "deepseek".into(), "DeepSeek");
            });
        ui.add(egui::TextEdit::singleline(&mut state.provider_key_draft)
            .password(true)
            .hint_text("Paste provider API key")
            .desired_width(f32::INFINITY));
        ui.horizontal(|ui| {
            if ui.button("Save key securely").clicked() {
                state.pending_provider_key = Some((state.provider_key_provider.clone(), state.provider_key_draft.clone()));
                state.provider_key_status = "Saving to Android Keystore…".into();
            }
            if ui.button("Remove key").clicked() {
                state.pending_provider_key = Some((state.provider_key_provider.clone(), String::new()));
                state.provider_key_status = "Removing stored key…".into();
            }
        });
        if !state.provider_key_status.is_empty() {
            ui.label(text(&state.provider_key_status, 10.0, egui::Color32::from_rgb(119, 220, 183)));
        }
    });
    ui.add_space(8.0);
    surface(ui, |ui| {
        ui.label(text("Privacy & security", 16.0, egui::Color32::WHITE).strong());
        ui.label(text("Per-user isolation, encrypted credentials and explicit approvals are part of the runtime design.", 12.0, egui::Color32::from_rgb(163, 174, 203)));
    });
}

pub fn render(ctx: &egui::Context, state: &mut AppState, time_seconds: f32) {
    ctx.set_visuals(egui::Visuals::dark());
    ctx.style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        style.visuals.panel_fill = egui::Color32::TRANSPARENT;
        style.visuals.window_fill = egui::Color32::from_rgb(18, 23, 40);
        style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(31, 38, 62);
        style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(53, 64, 104);
        style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(76, 91, 157);
        style.visuals.selection.bg_fill = egui::Color32::from_rgb(76, 91, 157);
    });
    background(ctx, time_seconds);

    egui::TopBottomPanel::top("cenai_header")
        .frame(egui::Frame::new().fill(egui::Color32::from_rgba_unmultiplied(12, 16, 30, 224)).inner_margin(egui::Margin::symmetric(18, 14)))
        .show(ctx, |ui| top_bar(ui));

    egui::TopBottomPanel::bottom("cenai_navigation")
        .frame(egui::Frame::new().fill(egui::Color32::from_rgba_unmultiplied(12, 16, 30, 240)).inner_margin(egui::Margin::symmetric(8, 8)))
        .show(ctx, |ui| navigation(ui, state));

    egui::CentralPanel::default()
        .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(18, 10)))
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                match state.destination {
                    Destination::Conversation => conversation(ui, state),
                    Destination::YourWork => work(ui, state),
                    Destination::Connections => connections(ui),
                    Destination::Settings => settings(ui, state),
                }
            });
        });
}
