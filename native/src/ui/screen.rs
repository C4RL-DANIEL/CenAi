//! Native CenAI screens. No WebView or Compose UI is used in this module.
use crate::ui::model::{AppState, Destination, MessageRole};
use std::cell::Cell;

thread_local! { static LIGHT_MODE: Cell<bool> = const { Cell::new(false) }; }

fn accent() -> egui::Color32 {
    egui::Color32::from_rgb(139, 155, 255)
}

fn text(value: impl Into<String>, size: f32, color: egui::Color32) -> egui::RichText {
    let color = LIGHT_MODE.with(|light| {
        if !light.get() { return color; }
        let spread = color.r().max(color.g()).max(color.b()) - color.r().min(color.g()).min(color.b());
        if spread < 65 && color.r() > 145 {
            egui::Color32::from_rgb(65, 75, 98)
        } else if color == egui::Color32::WHITE {
            egui::Color32::from_rgb(27, 35, 55)
        } else {
            color
        }
    });
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
    let light = LIGHT_MODE.with(Cell::get);
    painter.rect_filled(rect, 0.0, if light { egui::Color32::from_rgb(241, 245, 253) } else { egui::Color32::from_rgb(8, 11, 22) });
    let drift = (time * 0.16).sin();
    let glow_a = egui::pos2(rect.width() * (0.22 + drift * 0.06), rect.height() * 0.22);
    let glow_b = egui::pos2(rect.width() * 0.84, rect.height() * (0.65 + drift * 0.04));
    for (center, color) in [
        (glow_a, if light { egui::Color32::from_rgba_unmultiplied(115, 137, 240, 24) } else { egui::Color32::from_rgba_unmultiplied(73, 94, 226, 20) }),
        (glow_b, if light { egui::Color32::from_rgba_unmultiplied(185, 123, 220, 20) } else { egui::Color32::from_rgba_unmultiplied(142, 65, 210, 18) }),
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
            if state.mfa_login_required && destination != Destination::Settings { continue; }
            let selected = state.destination == destination;
            let fill = if selected {
                egui::Color32::from_rgb(49, 59, 104)
            } else {
                egui::Color32::TRANSPARENT
            };
            let color = if selected { egui::Color32::WHITE } else { egui::Color32::from_rgb(145, 154, 180) };
            if ui.add_sized(
                [if state.mfa_login_required { ui.available_width() } else { ui.available_width() / 4.0 }, 54.0],
                egui::Button::new(text(format!("{glyph}\n{label}"), 11.0, color))
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
    if !state.agent_action_status.is_empty() {
        ui.add_space(6.0);
        surface(ui, |ui| {
            ui.label(text(&state.agent_action_status, 10.0, egui::Color32::from_rgb(119, 220, 183)));
        });
    }
    ui.add_space(16.0);

    let history_items: Vec<(String, String)> = state.conversation_history.iter()
        .map(|item| (item.id.clone(), item.title.clone()))
        .collect();
    surface(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(text("RECENT CONVERSATIONS", 9.0, egui::Color32::from_rgb(143, 154, 180)).strong());
            if ui.add_enabled(!state.conversation.sending, egui::Button::new("+ New chat")).clicked() {
                state.pending_new_conversation = true;
            }
        });
        if history_items.is_empty() {
            ui.label(text("Your saved conversations will appear here.", 10.0, egui::Color32::from_rgb(143, 154, 180)));
        } else {
            ui.label(text(format!("{} saved conversations", history_items.len()), 9.0, egui::Color32::from_rgb(143, 154, 180)));
            egui::ScrollArea::vertical().max_height(150.0).show_rows(ui, 28.0, history_items.len(), |ui, rows| {
                for index in rows {
                    let (id, title) = &history_items[index];
                    let label = if title.trim().is_empty() { "New chat" } else { title.as_str() };
                    let selected = state.active_conversation_id == *id;
                    if !state.conversation.sending && ui.selectable_label(selected, label).clicked() && !selected {
                        state.pending_conversation_switch = Some(id.clone());
                    }
                }
            });
        }
    });
    ui.add_space(8.0);

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

    let latest_assistant = state.conversation.messages()
        .filter(|message| message.role == MessageRole::Assistant && !message.body.trim().is_empty() && !message.streaming)
        .map(|message| message.body.clone()).last();
    if let Some(response) = latest_assistant {
        surface(ui, |ui| {
            ui.horizontal(|ui| {
                let voice_ready = state.voice_service_status.starts_with("Ready ·")
                    || state.voice_service_status.starts_with("Connected ·");
                if ui.add_enabled(!state.account_email.is_empty() && voice_ready, egui::Button::new("Read aloud · CenAI Voice")).clicked() {
                    state.pending_speech_text = Some(response);
                    state.speech_status = "Preparing speech playback…".into();
                }
                if ui.button("Stop speech").clicked() {
                    state.pending_speech_stop = true;
                }
            });
            if !state.speech_status.is_empty() {
                ui.label(text(&state.speech_status, 10.0, egui::Color32::from_rgb(119, 220, 183)));
            }
            ui.label(text("CenAI Voice sends your text to Cloudflare for English speech synthesis. Your account quota is checked before each request; audio is returned for local playback.", 9.0, egui::Color32::from_rgb(143, 154, 180)));
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
        ui.horizontal(|ui| {
            if state.voice_active {
                if ui.button("Stop & transcribe").clicked() {
                    state.pending_voice_stop = true;
                    state.voice_status = "Finishing your recording…".into();
                }
            } else if state.pending_voice_start {
                if ui.button("Cancel microphone request").clicked() {
                    state.pending_voice_stop = true;
                    state.voice_status = "Cancelling voice input…".into();
                }
            } else if ui.add_enabled(!state.account_email.is_empty(), egui::Button::new("🎙 Voice input")).clicked() {
                state.pending_voice_start = true;
                state.voice_status = "Requesting microphone access…".into();
            }
            if !state.voice_status.is_empty() {
                ui.label(text(&state.voice_status, 10.0, egui::Color32::from_rgb(119, 220, 183)));
            }
            ui.label(text("Voice transcription sends your recording to Google Gemini using your configured Gemini key. The transcript stays in the draft until you review and send it.", 9.0, egui::Color32::from_rgb(143, 154, 180)));
        });
        ui.horizontal(|ui| {
            if ui.add_enabled(!state.account_email.is_empty(), egui::Button::new("＋ Add attachment")).clicked() {
                state.pending_attachment_picker = true;
                state.attachment_status = "Opening Android file picker…".into();
            }
            if !state.attachment_status.is_empty() {
                ui.label(text(&state.attachment_status, 10.0, egui::Color32::from_rgb(119, 220, 183)));
            }
        });
        if !state.attachments.is_empty() {
            ui.add(egui::TextEdit::singleline(&mut state.attachment_prompt_draft)
                .hint_text("What should CenAI inspect in these files?").desired_width(f32::INFINITY));
            let mut analyze_attachment = None;
            let mut delete_attachment = None;
            for attachment in &state.attachments {
                ui.horizontal(|ui| {
                    ui.label(text(format!("{} · {:.1} KB", attachment.file_name, attachment.byte_length as f64 / 1024.0), 10.0, egui::Color32::from_rgb(190, 201, 224)));
                    if ui.small_button("Analyze").clicked() {
                        analyze_attachment = Some(attachment.id.clone());
                    }
                    if ui.small_button("Remove").clicked() {
                        delete_attachment = Some(attachment.id.clone());
                    }
                });
            }
            if let Some(attachment_id) = analyze_attachment {
                state.pending_attachment_analysis = Some((attachment_id, state.attachment_prompt_draft.trim().to_owned()));
                state.attachment_status = "Preparing attachment analysis…".into();
            }
            if let Some(attachment_id) = delete_attachment {
                state.pending_attachment_delete = Some(attachment_id);
                state.attachment_status = "Removing attachment…".into();
            }
        }
        ui.horizontal(|ui| {
            if ui.add_enabled(!state.account_email.is_empty(), egui::Button::new("＋ Attach file")).clicked() {
                state.pending_attachment_picker = true;
                state.attachment_status = "Opening Android document picker…".into();
            }
            if !state.attachments.is_empty() {
                ui.label(text(format!("{} attached", state.attachments.len()), 10.0, accent()));
            }
        });
        let attachment_items = state.attachments.clone();
        if !attachment_items.is_empty() {
            egui::CollapsingHeader::new(format!("Attachments · {}", attachment_items.len()))
                .default_open(false)
                .show(ui, |ui| {
                    ui.add(egui::TextEdit::singleline(&mut state.attachment_prompt_draft)
                        .hint_text("What should CenAI extract or explain?")
                        .desired_width(f32::INFINITY));
                    ui.label(text("File contents are sent to Google Gemini only when you choose Analyze. Images, PDFs, text files and WAV audio are supported.", 9.0, egui::Color32::from_rgb(143, 154, 180)));
                    for attachment in &attachment_items {
                        ui.horizontal(|ui| {
                            ui.label(text(
                                format!("{} · {:.1} KB", attachment.file_name, attachment.byte_length as f64 / 1024.0),
                                10.0,
                                egui::Color32::from_rgb(190, 199, 220),
                            ));
                            let supported = attachment.mime_type.starts_with("image/")
                                || attachment.mime_type == "application/pdf"
                                || attachment.mime_type.starts_with("text/")
                                || (matches!(attachment.mime_type.as_str(), "audio/wav" | "audio/x-wav")
                                    && attachment.byte_length <= 20 * 1024 * 1024);
                            if ui.add_enabled(supported && !state.account_email.is_empty(), egui::Button::new("Analyze")).clicked() {
                                state.pending_attachment_analysis = Some((
                                    attachment.id.clone(),
                                    state.attachment_prompt_draft.clone(),
                                ));
                                state.attachment_status = format!("Analyzing {}…", attachment.file_name);
                                state.attachment_analysis_result.clear();
                            }
                            if ui.button("Remove").clicked() {
                                state.pending_attachment_delete = Some(attachment.id.clone());
                            }
                        });
                    }
                    if !state.attachment_status.is_empty() {
                        ui.label(text(&state.attachment_status, 10.0, egui::Color32::from_rgb(119, 220, 183)));
                    }
                    if !state.attachment_analysis_result.is_empty() {
                        ui.separator();
                        ui.label(text("Analysis result", 12.0, egui::Color32::WHITE).strong());
                        egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                            ui.label(text(&state.attachment_analysis_result, 11.0, egui::Color32::from_rgb(190, 199, 220)));
                        });
                        if ui.button("Insert analysis into message").clicked() {
                            if !state.conversation.draft.trim().is_empty() {
                                state.conversation.draft.push_str("\n\n");
                            }
                            state.conversation.draft.push_str(&state.attachment_analysis_result);
                        }
                    }
                });
        }
        if !state.attachment_status.is_empty() && state.attachments.is_empty() {
            ui.label(text(&state.attachment_status, 10.0, egui::Color32::from_rgb(119, 220, 183)));
        }
        ui.horizontal(|ui| {
            ui.checkbox(&mut state.planning_mode, "Plan task first");
            ui.add_enabled_ui(state.personal_memory_enabled, |ui| {
                ui.checkbox(&mut state.include_memory_for_request, "Use my private memory for this message");
            });
            if state.conversation.sending { ui.label(text("Working…", 10.0, accent())); }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(text("Sensitive actions always need approval", 9.0, egui::Color32::from_rgb(124, 136, 165)));
            });
        });
    });
}

fn work(ui: &mut egui::Ui, state: &mut AppState) {
    ui.add_space(10.0);
    ui.label(text("Your Work", 26.0, egui::Color32::WHITE).strong());
    ui.label(text("Projects, saved artifacts and task plans in your private account workspace.", 13.0, egui::Color32::from_rgb(163, 174, 203)));
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
    let mut open_artifact = None;
    let mut delete_artifact = None;
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
                for artifact in &project.artifacts {
                    ui.horizontal(|ui| {
                        ui.label(text(format!("▤  {}", artifact.file_name), 11.0, egui::Color32::from_rgb(165, 184, 220)));
                        if ui.small_button("Open").clicked() {
                            open_artifact = Some(artifact.id.clone());
                        }
                        if ui.small_button("Delete").clicked() {
                            delete_artifact = Some(artifact.id.clone());
                        }
                    });
                }
            });
            ui.add_space(8.0);
        }
    }
    ui.add_space(12.0);
    surface(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(text("Agent task plans", 17.0, egui::Color32::WHITE).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(text(format!("{} saved", state.task_plans.len()), 10.0, egui::Color32::from_rgb(140, 154, 190)));
            });
        });
        ui.label(text("Central Brain proposals are saved here for review. Saving a plan never executes its steps.", 11.0, egui::Color32::from_rgb(163, 174, 203)));
        if state.task_plans.is_empty() {
            ui.label(text("No agent plans yet. Turn on Planning mode in chat to create a structured proposal.", 11.0, egui::Color32::from_rgb(140, 154, 190)));
        }
    });
    let plans = state.task_plans.clone();
    for plan in plans {
        surface(ui, |ui| {
            ui.label(text(&plan.goal, 14.0, egui::Color32::WHITE).strong());
            let completed = plan.steps.iter().filter(|step| step.status == crate::core::agents::StepStatus::Completed).count();
            ui.label(text(
                if plan.stopped { format!("Stopped · {completed}/{} steps complete", plan.steps.len()) }
                else if plan.is_paused() { format!("Paused · {completed}/{} steps complete", plan.steps.len()) }
                else { format!("{completed}/{} steps complete · approval-controlled workflow", plan.steps.len()) },
                10.0,
                if plan.stopped { egui::Color32::from_rgb(235, 156, 130) }
                else if plan.is_paused() { egui::Color32::from_rgb(224, 190, 120) }
                else { egui::Color32::from_rgb(119, 220, 183) },
            ));
            if !plan.stopped {
                let mut background_authorized = plan.background_authorized;
                if ui.checkbox(&mut background_authorized, "Allow background cognitive processing").changed() {
                    state.pending_plan_action = Some((plan.id.clone(), 0, if background_authorized {
                        "background_enable".into()
                    } else {
                        "background_disable".into()
                    }));
                }
                ui.label(text("Only research, analysis and writing steps run in the background. No device controls or external actions. Android scheduling is best-effort.", 9.5, egui::Color32::from_rgb(145, 158, 186)));
            }
            let ready_count = plan.ready_steps().len();
            if ready_count > 0 && !plan.stopped && ui.button(format!("Run {ready_count} ready step(s) in parallel")).clicked() {
                state.pending_plan_action = Some((plan.id.clone(), 0, "execute_ready".into()));
            }
            let interrupted_count = plan.interrupted_steps_ready_to_retry().len();
            if interrupted_count > 0 && !plan.stopped
                && ui.button(format!("Retry {interrupted_count} interrupted step(s)")).clicked()
            {
                state.pending_plan_action = Some((plan.id.clone(), 0, "retry_interrupted".into()));
            }
            if !plan.events.is_empty() {
                ui.collapsing(format!("Execution history · {} events", plan.events.len()), |ui| {
                    for event in plan.events.iter().rev().take(12) {
                        let step_label = event.step_id.map(|id| format!("Step #{id}")).unwrap_or_else(|| "Plan".into());
                        let detail = event.detail.as_deref().unwrap_or("");
                        ui.label(text(
                            format!("{} · {} · {:?}{}", event.sequence, step_label, event.kind,
                                if detail.is_empty() { String::new() } else { format!(" — {detail}") }),
                            9.0, egui::Color32::from_rgb(163, 174, 203),
                        ));
                    }
                    if plan.events.len() > 12 {
                        ui.label(text("Showing the latest 12 events; older events remain saved until history rolls over.", 9.0, egui::Color32::from_rgb(140, 154, 190)));
                    }
                });
            }
            for step in &plan.steps {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(text(format!("#{} · {:?}", step.id, step.role), 10.0, egui::Color32::from_rgb(165, 184, 220)));
                    ui.label(text(format!("{:?}", step.status), 10.0, egui::Color32::from_rgb(190, 198, 220)));
                });
                ui.label(text(&step.title, 11.0, egui::Color32::WHITE));
                if !step.depends_on.is_empty() {
                    ui.label(text(
                        format!("Depends on step(s): {}", step.depends_on.iter().map(u64::to_string).collect::<Vec<_>>().join(", ")),
                        9.0,
                        egui::Color32::from_rgb(140, 154, 190),
                    ));
                }
                let ready = !plan.is_paused() && step.status == crate::core::agents::StepStatus::Pending
                    && step.depends_on.iter().all(|dependency| plan.steps.iter().any(|candidate|
                        candidate.id == *dependency && candidate.status == crate::core::agents::StepStatus::Completed));
                if let Some(output) = step.output.as_deref() {
                    let heading = match step.status {
                        crate::core::agents::StepStatus::Blocked => "Blocked · required handoff",
                        crate::core::agents::StepStatus::Failed if output.starts_with("Execution was interrupted") => "Execution interrupted · manual retry available",
                        _ => "AI step result",
                    };
                    ui.collapsing(heading, |ui| { ui.label(text(output, 11.0, egui::Color32::from_rgb(210, 220, 232))); });
                }
                ui.horizontal_wrapped(|ui| {
                    match step.status {
                        crate::core::agents::StepStatus::Pending if ready && !plan.stopped => {
                            if ui.button("Run with AI").clicked() {
                                state.pending_plan_action = Some((plan.id.clone(), step.id, "execute".into()));
                            }
                            if ui.button("Mark started").clicked() {
                                state.pending_plan_action = Some((plan.id.clone(), step.id, "start".into()));
                            }
                            if ui.button("Request approval").clicked() {
                                state.pending_plan_action = Some((plan.id.clone(), step.id, "request_approval".into()));
                            }
                        }
                        crate::core::agents::StepStatus::WaitingForApproval if !plan.stopped => {
                            if ui.button("Approve step").clicked() {
                                state.pending_plan_action = Some((plan.id.clone(), step.id, "approve".into()));
                            }
                            if ui.button("Decline").clicked() {
                                state.pending_plan_action = Some((plan.id.clone(), step.id, "deny".into()));
                            }
                        }
                        crate::core::agents::StepStatus::Blocked if !plan.stopped && !plan.is_paused() => {
                            if ui.button("Retry after resolving blocker").clicked() {
                                state.pending_plan_action = Some((plan.id.clone(), step.id, "retry_blocked".into()));
                            }
                        }
                        crate::core::agents::StepStatus::Failed if !plan.stopped && !plan.is_paused() => {
                            if ui.button("Retry failed step").clicked() {
                                state.pending_plan_action = Some((plan.id.clone(), step.id, "retry_failed".into()));
                            }
                        }
                        crate::core::agents::StepStatus::Running if !plan.stopped => {
                            if ui.button("Mark complete").clicked() {
                                state.pending_plan_action = Some((plan.id.clone(), step.id, "complete".into()));
                            }
                            if ui.button("Mark failed").clicked() {
                                state.pending_plan_action = Some((plan.id.clone(), step.id, "fail".into()));
                            }
                        }
                        _ => {}
                    }
                });
            }
            let has_active_work = plan.steps.iter().any(|step| matches!(step.status,
                crate::core::agents::StepStatus::Pending | crate::core::agents::StepStatus::Running
                | crate::core::agents::StepStatus::WaitingForApproval));
            if !plan.stopped && (plan.is_paused() || has_active_work) {
                if plan.is_paused() {
                    if ui.button("Resume plan").clicked() {
                        state.pending_plan_action = Some((plan.id.clone(), 0, "resume".into()));
                    }
                } else if ui.button("Pause plan").clicked() {
                    state.pending_plan_action = Some((plan.id.clone(), 0, "pause".into()));
                }
            }
            if !plan.stopped && plan.steps.iter().any(|step| matches!(step.status,
                crate::core::agents::StepStatus::Pending | crate::core::agents::StepStatus::Running
                | crate::core::agents::StepStatus::WaitingForApproval))
                && ui.button("Stop this plan").clicked()
            {
                state.pending_plan_action = Some((plan.id.clone(), 0, "stop".into()));
            }
        });
        ui.add_space(6.0);
    }
    if !state.plan_status.is_empty() {
        surface(ui, |ui| {
            ui.label(text(&state.plan_status, 10.0, egui::Color32::from_rgb(119, 220, 183)));
        });
    }
    if let Some(artifact_id) = open_artifact {
        state.pending_artifact_open = Some(artifact_id);
        state.artifact_preview_status = "Opening artifact…".into();
    }
    if let Some(artifact_id) = delete_artifact {
        state.confirm_artifact_delete = Some(artifact_id);
    }
    if let Some(artifact_id) = state.confirm_artifact_delete.clone() {
        let mut open = true;
        egui::Window::new("Delete workspace artifact").collapsible(false).resizable(false).open(&mut open).show(ui.ctx(), |ui| {
            ui.label(text("This permanently deletes the selected artifact from this account's private workspace.", 12.0, egui::Color32::WHITE));
            ui.horizontal(|ui| {
                if ui.button("Delete permanently").clicked() {
                    state.pending_artifact_delete = Some(artifact_id.clone());
                    state.confirm_artifact_delete = None;
                }
                if ui.button("Cancel").clicked() {
                    state.confirm_artifact_delete = None;
                }
            });
        });
        if !open { state.confirm_artifact_delete = None; }
    }
    if !state.artifact_preview_name.is_empty() || !state.artifact_preview_status.is_empty() {
        ui.add_space(10.0);
        surface(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(text(if state.artifact_preview_name.is_empty() { "Artifact preview" } else { &state.artifact_preview_name }, 15.0, egui::Color32::WHITE).strong());
                if ui.small_button("Close").clicked() {
                    state.artifact_preview_id.clear();
                    state.artifact_preview_project_id.clear();
                    state.artifact_preview_name.clear();
                    state.artifact_preview_content.clear();
                    state.artifact_preview_status.clear();
                }
            });
            if !state.artifact_preview_status.is_empty() {
                ui.label(text(&state.artifact_preview_status, 10.0, egui::Color32::from_rgb(220, 184, 121)));
            }
            if !state.artifact_preview_content.is_empty() {
                egui::ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
                    ui.add(egui::TextEdit::multiline(&mut state.artifact_preview_content)
                        .desired_rows(10)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace));
                });
            }
            if !state.artifact_preview_id.is_empty() && ui.button("Save changes to this artifact").clicked() {
                state.pending_artifact = Some((
                    state.artifact_preview_project_id.clone(),
                    state.artifact_preview_id.clone(),
                    state.artifact_preview_name.clone(),
                    state.artifact_preview_content.clone(),
                ));
                state.artifact_status = "Saving artifact changes…".into();
            }
        });
    }
    ui.add_space(10.0);
    if state.task_plans.is_empty() {
        surface(ui, |ui| {
            ui.label(text("Planned tasks", 16.0, egui::Color32::WHITE).strong());
            ui.label(text("Turn on “Plan task first” in Conversation to create a plan. Plans are saved here; nothing executes without separate approval.", 11.0, egui::Color32::from_rgb(163, 174, 203)));
        });
    } else {
        for plan in &state.task_plans {
            surface(ui, |ui| {
                ui.label(text(&plan.goal, 15.0, egui::Color32::WHITE).strong());
                ui.label(text(format!("{} steps · {}", plan.steps.len(), if plan.stopped { "Stopped" } else { "Tracked in approval workflow" }), 10.0, egui::Color32::from_rgb(140, 154, 190)));
                for step in &plan.steps {
                    ui.label(text(format!("○  {:?} — {}", step.role, step.title), 11.0, egui::Color32::from_rgb(163, 174, 203)));
                }
            });
            ui.add_space(8.0);
        }
    }
    ui.add_space(12.0);
    surface(ui, |ui| {
        ui.label(text("Automations", 17.0, egui::Color32::WHITE).strong());
        ui.label(text("Create account-owned UTC schedules. Every due run is saved for your review; no action runs without your approval.", 11.0, egui::Color32::from_rgb(163, 174, 203)));
        ui.add(egui::TextEdit::singleline(&mut state.automation_id_draft).hint_text("Schedule ID (daily-report)").desired_width(f32::INFINITY));
        ui.add(egui::TextEdit::singleline(&mut state.automation_title_draft).hint_text("What should CenAI prepare?").desired_width(f32::INFINITY));
        ui.add(egui::TextEdit::singleline(&mut state.automation_schedule_draft).hint_text("Cron schedule in UTC (0 9 * * 1-5)").desired_width(f32::INFINITY));
        ui.label(text("Every scheduled task is prepared for review. CenAI never executes a scheduled action without your approval.", 10.0, egui::Color32::from_rgb(220, 184, 121)));
        if ui.button("Save schedule").clicked() {
            if state.automation_id_draft.trim().is_empty() || state.automation_title_draft.trim().is_empty() || state.automation_schedule_draft.trim().is_empty() {
                state.automation_status = "Enter a schedule ID, task description and five-field UTC cron expression.".into();
            } else {
                state.pending_automation_save = true;
                state.automation_status = "Saving automation…".into();
            }
        }
        if !state.automation_status.is_empty() {
            ui.label(text(&state.automation_status, 10.0, egui::Color32::from_rgb(119, 220, 183)));
        }
    });
    let automations = state.automations.clone();
    for automation in automations {
        surface(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(text(&automation.title, 14.0, egui::Color32::WHITE).strong());
                ui.label(text(if automation.enabled { "Enabled" } else { "Paused" }, 10.0, if automation.enabled { egui::Color32::from_rgb(119, 220, 183) } else { egui::Color32::from_rgb(220, 184, 121) }));
            });
            ui.label(text(format!("{} · UTC · {}", automation.id, automation.schedule), 10.0, egui::Color32::from_rgb(163, 174, 203)));
            if let Some(pending_at) = automation.pending_scheduled_at {
                let due = chrono::DateTime::<chrono::Utc>::from_timestamp(pending_at, 0)
                    .map(|time| time.format("%Y-%m-%d %H:%M UTC").to_string())
                    .unwrap_or_else(|| "unknown time".into());
                ui.label(text(format!("Due · awaiting review ({due})"), 10.0, egui::Color32::from_rgb(220, 184, 121)));
                if ui.button("Review in conversation").clicked() {
                    if let Some(prompt) = automation.review_prompt() {
                        if !state.conversation.draft.trim().is_empty() {
                            state.conversation.draft.push_str("\n\n");
                        }
                        state.conversation.draft.push_str(&prompt);
                        state.destination = Destination::Conversation;
                        state.automation_status = "Review prompt prepared. Read it and press Send when you are ready; no action has been approved or executed.".into();
                    } else {
                        state.automation_status = "This scheduled occurrence is no longer eligible for review.".into();
                    }
                }
            } else if let Ok(Some(next)) = automation.next_run_after(chrono::Utc::now()) {
                ui.label(text(format!("Next run: {}", next.format("%Y-%m-%d %H:%M UTC")), 10.0, egui::Color32::from_rgb(140, 154, 190)));
            }
            ui.horizontal(|ui| {
                if ui.button(if automation.enabled { "Pause" } else { "Resume" }).clicked() {
                    state.pending_automation_toggle = Some((automation.id.clone(), !automation.enabled));
                }
                if ui.button("Delete").clicked() {
                    state.pending_automation_delete = Some(automation.id.clone());
                }
            });
        });
        ui.add_space(8.0);
    }
    if !state.projects.is_empty() {
        ui.add_space(12.0);
        surface(ui, |ui| {
            ui.label(text("Create a text artifact", 16.0, egui::Color32::WHITE).strong());
            egui::ComboBox::from_id_salt("artifact_project")
                .selected_text(state.projects.iter().find(|project| project.id == state.artifact_project_id)
                    .map(|project| project.name.as_str()).unwrap_or("Choose project"))
                .show_ui(ui, |ui| {
                    for project in &state.projects {
                        ui.selectable_value(&mut state.artifact_project_id, project.id.clone(), &project.name);
                    }
                });
            ui.add(egui::TextEdit::singleline(&mut state.artifact_name_draft).hint_text("Artifact filename").desired_width(f32::INFINITY));
            ui.add(egui::TextEdit::multiline(&mut state.artifact_content_draft).hint_text("Write a note or draft…").desired_width(f32::INFINITY).desired_rows(4));
            if ui.button("Save artifact").clicked() {
                state.pending_artifact = Some((
                    state.artifact_project_id.clone(),
                    format!("artifact-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()),
                    state.artifact_name_draft.trim().to_owned(),
                    state.artifact_content_draft.clone(),
                ));
                state.artifact_status = "Saving artifact…".into();
            }
            if !state.artifact_status.is_empty() {
                ui.label(text(&state.artifact_status, 10.0, egui::Color32::from_rgb(119, 220, 183)));
            }
        });
    }
}

fn connections(ui: &mut egui::Ui, state: &mut AppState) {
    ui.add_space(10.0);
    ui.label(text("Connections", 26.0, egui::Color32::WHITE).strong());
    ui.label(text("Bring your models, tools and services together.", 13.0, egui::Color32::from_rgb(163, 174, 203)));
    ui.add_space(18.0);
    surface(ui, |ui| {
        ui.label(text("Ecosystem Engine", 17.0, egui::Color32::WHITE).strong());
        ui.label(text("Register compatible apps and manage separate, revocable permissions for this account. App access never crosses user accounts.", 11.0, egui::Color32::from_rgb(163, 174, 203)));
        ui.label(text("Only publisher accounts approved by CenAI can register manifests. Other users can review and grant permissions to trusted apps below.", 10.0, egui::Color32::from_rgb(163, 174, 203)));
        ui.separator();
        ui.label(text("Register an app manifest", 13.0, egui::Color32::WHITE).strong());
        ui.add(egui::TextEdit::singleline(&mut state.ecosystem_app_id_draft).hint_text("App ID (com.example.app)").desired_width(f32::INFINITY));
        ui.add(egui::TextEdit::singleline(&mut state.ecosystem_publisher_id_draft).hint_text("Trusted publisher ID").desired_width(f32::INFINITY));
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut state.ecosystem_version_draft).hint_text("Version"));
        });
        ui.add(egui::TextEdit::singleline(&mut state.ecosystem_description_draft).hint_text("Short app description").desired_width(f32::INFINITY));
        ui.add(egui::TextEdit::singleline(&mut state.ecosystem_capabilities_draft).hint_text("Capabilities, comma separated (notes.read)").desired_width(f32::INFINITY));
        ui.add(egui::TextEdit::singleline(&mut state.ecosystem_scopes_draft).hint_text("Requested scopes, comma separated").desired_width(f32::INFINITY));
        if ui.button("Register app manifest").clicked() {
            let capabilities = state.ecosystem_capabilities_draft.split(',').map(str::trim)
                .filter(|value| !value.is_empty()).map(str::to_owned).collect::<Vec<_>>();
            let requested_scopes = state.ecosystem_scopes_draft.split(',').map(str::trim)
                .filter(|value| !value.is_empty()).map(str::to_owned).collect::<Vec<_>>();
            if state.ecosystem_app_id_draft.trim().is_empty()
                || state.ecosystem_publisher_id_draft.trim().is_empty()
                || state.ecosystem_version_draft.trim().is_empty()
                || capabilities.is_empty() || requested_scopes.is_empty() {
                state.ecosystem_status = "Complete the app identity, publisher, version, capabilities and requested scopes.".into();
            } else {
                state.pending_ecosystem_register = Some(serde_json::json!({
                    "appId": state.ecosystem_app_id_draft.trim(),
                    "publisherId": state.ecosystem_publisher_id_draft.trim(),
                    "version": state.ecosystem_version_draft.trim(),
                    "protocolVersion": 1,
                    "description": state.ecosystem_description_draft.trim(),
                    "capabilities": capabilities,
                    "requestedScopes": requested_scopes
                }));
                state.ecosystem_status = "Submitting manifest to the trusted-publisher registry…".into();
            }
        }
    });
    surface(ui, |ui| {
        ui.label(text("Connect a remote MCP server", 16.0, egui::Color32::WHITE).strong());
        ui.label(text("Add a server endpoint over HTTPS. Discovery does not grant permission to run its tools.", 11.0, egui::Color32::from_rgb(163, 174, 203)));
        ui.add(egui::TextEdit::singleline(&mut state.mcp_server_id_draft).hint_text("Server ID (for example, org.example.tools)").desired_width(f32::INFINITY));
        ui.add(egui::TextEdit::singleline(&mut state.mcp_endpoint_draft).hint_text("HTTPS endpoint URL").desired_width(f32::INFINITY));
        ui.add(egui::TextEdit::singleline(&mut state.mcp_publisher_draft).hint_text("Publisher name").desired_width(f32::INFINITY));
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut state.mcp_version_draft).hint_text("Version"));
            ui.add(egui::TextEdit::singleline(&mut state.mcp_license_draft).hint_text("License"));
        });
        ui.add(egui::TextEdit::singleline(&mut state.mcp_capabilities_draft).hint_text("Declared capability names, comma separated").desired_width(f32::INFINITY));
        ui.add(egui::TextEdit::singleline(&mut state.mcp_bearer_token_draft).password(true).hint_text("Optional server bearer token (encrypted on this device)").desired_width(f32::INFINITY));
        if ui.add_enabled(!state.mcp_server_id_draft.trim().is_empty() && !state.mcp_bearer_token_draft.is_empty(), egui::Button::new("Save encrypted server token")).clicked() {
            state.pending_mcp_token_save = Some((state.mcp_server_id_draft.trim().to_owned(), state.mcp_bearer_token_draft.clone()));
            state.mcp_status = "Saving server token to the Android Keystore…".into();
        }
        if ui.add_enabled(!state.mcp_server_id_draft.trim().is_empty(), egui::Button::new("Remove saved server token")).clicked() {
            state.pending_mcp_token_save = Some((state.mcp_server_id_draft.trim().to_owned(), String::new()));
            state.mcp_status = "Removing saved server token…".into();
        }
        ui.checkbox(&mut state.mcp_reviewed, "I reviewed the publisher, endpoint and requested capabilities");
        if ui.button("Discover tools").clicked() {
            let capabilities = state.mcp_capabilities_draft.split(',')
                .map(str::trim).filter(|name| !name.is_empty())
                .map(|name| crate::core::mcp::McpCapability {
                    name: name.to_owned(),
                    description: "Declared capability; inspect remote tool schema before use".into(),
                    requires_sensitive_read: true,
                    requires_external_write: true,
                    requested_scopes: vec![name.to_owned()],
                }).collect::<Vec<_>>();
            if !state.mcp_reviewed {
                state.mcp_status = "Review the publisher and requested capabilities first.".into();
            } else {
                state.pending_mcp_connect = Some(crate::core::mcp::McpServerManifest {
                    server_id: state.mcp_server_id_draft.trim().to_owned(),
                    version: state.mcp_version_draft.trim().to_owned(),
                    endpoint_url: state.mcp_endpoint_draft.trim().to_owned(),
                    protocol_version: 1,
                    publisher: state.mcp_publisher_draft.trim().to_owned(),
                    publisher_verified: false,
                    license_id: state.mcp_license_draft.trim().to_owned(),
                    declared_dependencies: Vec::new(),
                    capabilities,
                });
                state.mcp_status = "Connecting and inspecting tool schemas…".into();
            }
        }
        if !state.mcp_status.is_empty() {
            ui.label(text(&state.mcp_status, 10.0, egui::Color32::from_rgb(220, 184, 121)));
        }
        for tool in &state.mcp_tools {
            ui.collapsing(&tool.name, |ui| {
                if let Some(description) = &tool.description {
                    ui.label(text(description, 11.0, egui::Color32::from_rgb(163, 174, 203)));
                }
                ui.label(text(tool.input_schema.to_string(), 10.0, egui::Color32::from_rgb(170, 190, 225)));
                if ui.button("Review and approve this tool").clicked() {
                    state.mcp_pending_approval = Some(tool.name.clone());
                }
            });
        }
    });
    surface(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(text("Ecosystem Engine", 16.0, egui::Color32::WHITE).strong());
            if ui.button("Refresh catalog").clicked() {
                state.pending_ecosystem_refresh = true;
                state.ecosystem_status = "Loading the app catalog for this account…".into();
            }
        });
        ui.label(text("Connect other apps to CenAI with explicit, account-specific permissions. Every grant can be revoked independently.", 11.0, egui::Color32::from_rgb(163, 174, 203)));
        if state.account_email.is_empty() {
            ui.label(text("Sign in to view your private app connections.", 11.0, egui::Color32::from_rgb(220, 184, 121)));
        } else if state.ecosystem_apps.is_empty() {
            ui.label(text("No compatible apps are currently registered. Refresh to check the catalog.", 11.0, egui::Color32::from_rgb(163, 174, 203)));
        }
        let apps = state.ecosystem_apps.clone();
        for app in apps {
            ui.separator();
            ui.label(text(&app.app_id, 13.0, egui::Color32::WHITE).strong());
            ui.label(text(format!("Publisher: {} · Version {}", app.publisher_id, app.version), 10.0, accent()));
            if !app.description.is_empty() {
                ui.label(text(&app.description, 11.0, egui::Color32::from_rgb(163, 174, 203)));
            }
            let active_grant = !app.revoked && !app.granted_scopes.is_empty();
            if active_grant {
                ui.label(text(format!("Granted: {}", app.granted_scopes.join(", ")), 10.0, egui::Color32::from_rgb(119, 220, 183)));
            } else {
                ui.label(text("No active permissions granted.", 10.0, egui::Color32::from_rgb(163, 174, 203)));
            }
            if state.ecosystem_selected_app != app.app_id {
                if ui.button("Review permissions").clicked() {
                    state.ecosystem_selected_app = app.app_id.clone();
                    state.ecosystem_grant_scopes_draft.clear();
                }
            } else {
                ui.label(text("Choose only the permissions this app needs:", 10.0, egui::Color32::from_rgb(163, 174, 203)));
                let mut selected = state.ecosystem_grant_scopes_draft.split(',')
                    .map(str::trim).filter(|value| !value.is_empty()).map(str::to_owned).collect::<std::collections::HashSet<_>>();
                for scope in &app.requested_scopes {
                    let mut checked = selected.contains(scope);
                    if ui.checkbox(&mut checked, scope).changed() {
                        if checked { selected.insert(scope.clone()); } else { selected.remove(scope); }
                    }
                }
                state.ecosystem_grant_scopes_draft = selected.into_iter().collect::<Vec<_>>().join(",");
                ui.horizontal(|ui| {
                    let scopes = state.ecosystem_grant_scopes_draft.split(',')
                        .map(str::trim).filter(|value| !value.is_empty()).map(str::to_owned).collect::<Vec<_>>();
                    if ui.add_enabled(!scopes.is_empty(), egui::Button::new("Grant selected permissions")).clicked() {
                        state.pending_ecosystem_grant = Some((app.app_id.clone(), scopes));
                        state.ecosystem_status = "Saving this account's app permissions…".into();
                    }
                    if active_grant && ui.button("Revoke access").clicked() {
                        state.pending_ecosystem_revoke = Some(app.app_id.clone());
                        state.ecosystem_status = "Revoking this app's permissions…".into();
                    }
                    if ui.button("Close").clicked() {
                        state.ecosystem_selected_app.clear();
                        state.ecosystem_grant_scopes_draft.clear();
                    }
                });
            }
        }
        if !state.ecosystem_status.is_empty() {
            ui.label(text(&state.ecosystem_status, 10.0, egui::Color32::from_rgb(119, 220, 183)));
        }
    });
    surface(ui, |ui| {
        ui.label(text("Managed model routing", 16.0, egui::Color32::WHITE).strong());
        let managed_status = if state.account_email.is_empty() {
            "Sign in to your CenAI account to use managed Cloud AI."
        } else {
            "CenAI Cloud is available for this signed-in account; server-side quotas apply."
        };
        ui.label(text(managed_status, 11.0, egui::Color32::from_rgb(163, 174, 203)));
        ui.separator();
        ui.label(text("Bring your own provider", 16.0, egui::Color32::WHITE).strong());
        ui.label(text("Configure OpenAI-compatible or DeepSeek credentials in Settings. Keys are stored in Android Keystore and used only for the selected provider route.", 11.0, egui::Color32::from_rgb(163, 174, 203)));
        ui.separator();
        ui.label(text("Remote MCP tools", 16.0, egui::Color32::WHITE).strong());
        let tool_status = if state.mcp_tools.is_empty() {
            "No tools discovered. Connect an HTTPS MCP server above."
                .to_owned()
        } else {
            format!("{} tools discovered. Discovery does not grant execution permission.", state.mcp_tools.len())
        };
        ui.label(text(tool_status, 11.0, egui::Color32::from_rgb(163, 174, 203)));
    });
    if !state.mcp_call_result.is_empty() {
        surface(ui, |ui| {
            ui.label(text("Last tool result", 13.0, egui::Color32::WHITE).strong());
            ui.label(text(&state.mcp_call_result, 10.0, egui::Color32::from_rgb(163, 174, 203)));
        });
    }
    if let Some(tool_name) = state.mcp_pending_approval.clone() {
        let mut open = true;
        egui::Window::new("Approve MCP action").collapsible(false).resizable(true).open(&mut open).show(ui.ctx(), |ui| {
            ui.label(text("This sends the following request to an external MCP server.", 12.0, egui::Color32::WHITE));
            ui.label(text(format!("Server: {}", state.mcp_server_id_draft), 11.0, egui::Color32::from_rgb(163, 174, 203)));
            ui.label(text(format!("Tool: {tool_name}"), 12.0, egui::Color32::WHITE).strong());
            ui.add(egui::TextEdit::multiline(&mut state.mcp_call_arguments_draft).desired_rows(5).desired_width(360.0));
            ui.horizontal(|ui| {
                if ui.button("Approve once and run").clicked() {
                    state.pending_mcp_call = Some((state.mcp_server_id_draft.clone(), tool_name.clone(), state.mcp_call_arguments_draft.clone()));
                    state.mcp_pending_approval = None;
                    state.mcp_status = "Running the approved MCP action…".into();
                }
                if ui.button("Cancel").clicked() { state.mcp_pending_approval = None; }
            });
        });
        if !open { state.mcp_pending_approval = None; }
    }
}

fn settings(ui: &mut egui::Ui, state: &mut AppState) {
    surface(ui, |ui| {
        ui.label(text("CenAI Voice", 18.0, egui::Color32::WHITE).strong());
        ui.label(text("Cloud speech powered by Deepgram Aura-1 through Cloudflare Workers AI.", 11.0, egui::Color32::from_rgb(163, 174, 203)));
        ui.label(text("Primary profile: CenAI Original · English · en-US", 11.0, egui::Color32::from_rgb(190, 201, 224)));
        ui.label(text("Server model: Deepgram Aura-1 · Cloudflare Workers AI", 10.0, egui::Color32::from_rgb(190, 201, 224)));
        ui.label(text("Validated language: English. Additional languages will appear only after quality testing.", 10.0, egui::Color32::from_rgb(190, 201, 224)));
        ui.label(text("Speech language: English", 11.0, egui::Color32::from_rgb(190, 201, 224)));
        ui.label(text("Aura-1 currently supports the English voice profile in CenAI.", 9.0, egui::Color32::from_rgb(163, 174, 203)));
        ui.label(text("No speech model is bundled in the app. Speech text is sent to the authenticated CenAI Worker and synthesized by Cloudflare Workers AI. A daily per-account and shared service quota protects the free allowance.", 10.0, egui::Color32::from_rgb(119, 220, 183)));
        ui.separator();
        ui.label(text("Cloud voice service", 12.0, egui::Color32::WHITE).strong());
        ui.label(text(&state.voice_service_status, 10.0, egui::Color32::from_rgb(190, 201, 224)));
        if ui.button("Check cloud voice service").clicked() {
            state.pending_voice_service_check = true;
            state.voice_service_status = "Checking CenAI Voice Workers AI service…".into();
        }
        if ui.small_button("View CenAI Voice implementation plan").clicked() {
            state.pending_open_url = Some("https://github.com/C4RL-DANIEL/CenAi/blob/rewrite/rust-native-core/docs/CENAI-VOICE.md".into());
        }
    });
    ui.add_space(8.0);
    if state.mfa_login_required {
        surface(ui, |ui| {
            ui.label(text("Secure sign-in", 19.0, egui::Color32::WHITE).strong());
            ui.label(text("This account has a verified authenticator. CenAI remains locked until you complete the second factor.", 11.0, egui::Color32::from_rgb(163, 174, 203)));
            if let Some(factor) = state.mfa_factors.iter().find(|factor| factor.id == state.mfa_enrollment_factor_id) {
                ui.label(text(format!("Authenticator: {}", factor.friendly_name), 11.0, egui::Color32::WHITE));
            }
            ui.add(egui::TextEdit::singleline(&mut state.mfa_code_draft)
                .password(false).char_limit(6).hint_text("6-digit authenticator code")
                .desired_width(f32::INFINITY));
            ui.horizontal(|ui| {
                if ui.button("Verify and unlock CenAI").clicked() {
                    state.pending_mfa_verify = true;
                    state.mfa_status = "Verifying your authenticator code…".into();
                }
                if ui.button("Refresh authenticator").clicked() {
                    state.pending_mfa_refresh = true;
                    state.mfa_status = "Checking your authenticator status…".into();
                }
                if ui.button("Sign out").clicked() {
                    state.pending_sign_out = true;
                }
            });
            if !state.mfa_status.is_empty() {
                ui.label(text(&state.mfa_status, 10.0, egui::Color32::from_rgb(220, 184, 121)));
            }
            ui.label(text("Only this verification step is available until the account's second factor succeeds.", 9.0, egui::Color32::from_rgb(143, 154, 180)));
        });
        return;
    }
    ui.add_space(10.0);
    ui.label(text("Settings", 26.0, egui::Color32::WHITE).strong());
    ui.label(text("Make CenAI feel right for you.", 13.0, egui::Color32::from_rgb(163, 174, 203)));
    ui.add_space(12.0);
    surface(ui, |ui| {
        ui.label(text("Permissions & responsible use", 16.0, egui::Color32::WHITE).strong());
        ui.label(text("CenAI requests access only when a feature needs it. Microphone access is requested when you start voice input. Files are selected through Android's system picker; CenAI does not request broad storage access.", 10.0, egui::Color32::from_rgb(190, 201, 224)));
        ui.label(text("Network access is used for sign-in, configured AI providers and cloud features. Connected-app permissions are separate, account-specific and revocable. Sensitive or destructive agent actions still require your approval.", 10.0, egui::Color32::from_rgb(190, 201, 224)));
        ui.label(text("USE AT YOUR OWN RISK: AI output can be incorrect or incomplete. Review responses and verify important information before relying on it. You are responsible for permissions you grant and actions you approve.", 10.0, egui::Color32::from_rgb(240, 190, 132)).strong());
    });
    ui.add_space(10.0);
    surface(ui, |ui| {
        ui.label(text("Appearance & accessibility", 16.0, egui::Color32::WHITE).strong());
        ui.checkbox(&mut state.light_mode, "Use light appearance");
        ui.checkbox(&mut state.reduced_motion, "Reduce interface motion");
        let previous_guardian_consent = state.guardian_consent;
        ui.checkbox(&mut state.guardian_consent, "Allow minimal Guardian diagnostics");
        if previous_guardian_consent != state.guardian_consent {
            state.pending_guardian_consent = Some(state.guardian_consent);
        }
        ui.label(text("Only a coarse diagnostic category may be uploaded, and only with your consent plus server-side rollout approval. Prompts, message text, file contents, paths and credentials are never sent.", 10.0, egui::Color32::from_rgb(163, 174, 203)));
        ui.label(text("The interface respects this setting when animating transitions.", 11.0, egui::Color32::from_rgb(163, 174, 203)));
        if let Some(snapshot) = &state.guardian_snapshot {
            let ram_used = snapshot.ram_total_kb.saturating_sub(snapshot.ram_available_kb);
            let ram_percent = ram_used.saturating_mul(100).checked_div(snapshot.ram_total_kb).unwrap_or(0);
            ui.separator();
            ui.label(text("Local device health", 13.0, egui::Color32::WHITE).strong());
            ui.horizontal_wrapped(|ui| {
                ui.label(text(format!("CPU: {}%", snapshot.cpu_usage_percent.map_or_else(|| "sampling".into(), |value| value.to_string())), 10.0, accent()));
                ui.label(text(format!("RAM: {}% · {:.0}/{:.0} MB", ram_percent, ram_used as f64 / 1024.0, snapshot.ram_total_kb as f64 / 1024.0), 10.0, accent()));
                ui.label(text(format!("Storage free: {:.1} GB", snapshot.storage_available_bytes as f64 / 1_073_741_824.0), 10.0, accent()));
                if let Some(battery) = snapshot.battery_percent {
                    ui.label(text(format!("Battery: {battery}% · {}", snapshot.battery_status.as_deref().unwrap_or("status unavailable")), 10.0, accent()));
                }
            });
            ui.label(text(format!("Network totals · RX {:.1} MB · TX {:.1} MB", snapshot.network_rx_bytes as f64 / 1_048_576.0, snapshot.network_tx_bytes as f64 / 1_048_576.0), 10.0, egui::Color32::from_rgb(163, 174, 203)));
                        let low_memory = snapshot.ram_available_kb > 0 && snapshot.ram_available_kb < 256 * 1024;
            let low_storage = snapshot.storage_available_bytes > 0 && snapshot.storage_available_bytes < 512 * 1_048_576;
            let low_battery = snapshot.battery_percent.is_some_and(|value| value < 15)
                && !snapshot.battery_status.as_deref().is_some_and(|status| status.eq_ignore_ascii_case("charging"));
            let high_cpu = snapshot.cpu_usage_percent.is_some_and(|value| value >= 90);
            if low_memory || low_storage || low_battery || high_cpu {
                ui.label(text("Guardian noticed a device resource warning:", 10.0, egui::Color32::from_rgb(240, 178, 126)).strong());
                if low_memory { ui.label(text("Available memory is low.", 10.0, egui::Color32::from_rgb(240, 178, 126))); }
                if low_storage { ui.label(text("Device storage is nearly full.", 10.0, egui::Color32::from_rgb(240, 178, 126))); }
                if low_battery { ui.label(text("Battery is critically low.", 10.0, egui::Color32::from_rgb(240, 178, 126))); }
                if high_cpu { ui.label(text("CPU usage is unusually high.", 10.0, egui::Color32::from_rgb(240, 178, 126))); }
            }
ui.label(text("System metrics are sampled locally and are never uploaded as Guardian telemetry.", 9.0, egui::Color32::from_rgb(163, 174, 203)));
        }
        ui.separator();
        ui.label(text("Central Brain memory", 15.0, egui::Color32::WHITE).strong());
        let previous_memory_enabled = state.personal_memory_enabled;
        ui.checkbox(&mut state.personal_memory_enabled, "Enable my private personal memory");
        if previous_memory_enabled != state.personal_memory_enabled {
            state.pending_memory_enabled = Some(state.personal_memory_enabled);
        }
        ui.label(text("Memories are stored under your authenticated account only. The Brain can use them only when you explicitly allow personal memory for a request. Disabling memory prevents retrieval and new saves.", 10.0, egui::Color32::from_rgb(163, 174, 203)));
        egui::ComboBox::from_id_salt("cenai_memory_category")
            .selected_text(match state.memory_category_draft.as_str() {
                "profile" => "Profile",
                "project" => "Project",
                "workflow" => "Workflow",
                "fact" => "Fact",
                _ => "Preference",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut state.memory_category_draft, "preference".into(), "Preference");
                ui.selectable_value(&mut state.memory_category_draft, "profile".into(), "Profile");
                ui.selectable_value(&mut state.memory_category_draft, "project".into(), "Project");
                ui.selectable_value(&mut state.memory_category_draft, "workflow".into(), "Workflow");
                ui.selectable_value(&mut state.memory_category_draft, "fact".into(), "Fact");
            });
        ui.add(egui::TextEdit::multiline(&mut state.memory_content_draft)
            .hint_text("Add a fact or preference to remember (up to 8,000 characters)")
            .desired_rows(3).desired_width(f32::INFINITY));
        if ui.add_enabled(state.personal_memory_enabled && !state.memory_content_draft.trim().is_empty(), egui::Button::new("Save private memory")).clicked() {
            state.pending_memory_save = Some((state.memory_category_draft.clone(), state.memory_content_draft.trim().to_owned()));
            state.personal_memory_status = "Saving to your private Central Brain memory…".into();
        }
        if !state.memory_content_draft.is_empty() && !state.personal_memory_enabled {
            ui.label(text("Enable personal memory before saving.", 10.0, egui::Color32::from_rgb(220, 184, 121)));
        }
        if !state.personal_memory_status.is_empty() {
            ui.label(text(&state.personal_memory_status, 10.0, egui::Color32::from_rgb(119, 220, 183)));
        }
        if ui.small_button("Delete all saved memories").clicked() {
            state.confirm_memory_clear = true;
        }
        if state.confirm_memory_clear {
            ui.label(text("Permanently delete every saved memory for this account?", 10.0, egui::Color32::from_rgb(235, 166, 145)));
            ui.horizontal(|ui| {
                if ui.button("Delete all memories").clicked() {
                    state.pending_memory_clear = true;
                    state.confirm_memory_clear = false;
                }
                if ui.button("Cancel").clicked() {
                    state.confirm_memory_clear = false;
                }
            });
        }
        if state.personal_memory_enabled && !state.personal_memories.is_empty() {
            ui.separator();
            ui.label(text("Saved memories", 13.0, egui::Color32::WHITE).strong());
            let mut delete_memory = None;
            for memory in &state.personal_memories {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(text(memory.category.to_uppercase(), 9.0, accent()).strong());
                        ui.add(egui::Label::new(text(&memory.content, 10.0, egui::Color32::from_rgb(190, 201, 224))).wrap());
                    });
                    if ui.small_button("Delete").clicked() {
                        delete_memory = Some(memory.id.clone());
                    }
                });
                ui.add_space(4.0);
            }
            if delete_memory.is_some() {
                state.pending_memory_delete = delete_memory;
            }
        }
    });
    ui.add_space(8.0);
    surface(ui, |ui| {
        ui.label(text("Offline AI · Lightning", 16.0, egui::Color32::WHITE).strong());
        ui.checkbox(&mut state.offline_ai_enabled, "Use Lightning for all new chats");
        ui.label(text(
            "When enabled, new chat prompts go directly to the bundled Qwen3 model on this device. CenAI does not send those prompts to cloud AI providers. Offline mode is available without signing in; signed-in accounts can save this preference.",
            11.0,
            egui::Color32::from_rgb(163, 174, 203),
        ));
        ui.label(text(
            if state.offline_ai_enabled { "Local-only chat routing is enabled." } else { "Cloud routing is available when an account or provider is connected; Lightning remains the offline fallback." },
            10.0,
            if state.offline_ai_enabled { egui::Color32::from_rgb(119, 220, 183) } else { egui::Color32::from_rgb(163, 174, 203) },
        ));
    });
    ui.add_space(8.0);
    surface(ui, |ui| {
        ui.label(text("Application updates", 16.0, egui::Color32::WHITE).strong());
        ui.label(text("CenAI checks official GitHub releases, verifies the APK SHA-256, and checks the installed signing certificate before opening Android's installer. Installation always requires your confirmation.", 10.0, egui::Color32::from_rgb(163, 174, 203)));
        ui.horizontal(|ui| {
            if ui.add_enabled(!state.pending_update_check && !state.pending_update_download, egui::Button::new("Check for updates")).clicked() {
                state.pending_update_check = true;
                state.update_status = "Checking official CenAI releases…".into();
                state.available_update = None;
                state.update_download_path.clear();
            }
            if let Some(update) = state.available_update.as_ref() {
                if ui.add_enabled(!state.pending_update_download && state.update_download_path.is_empty(), egui::Button::new("Download verified APK")).clicked() {
                    state.pending_update_download = true;
                    state.update_status = format!("Downloading CenAI {}…", update.version);
                }
            }
            if !state.update_download_path.is_empty() && ui.button("Install update").clicked() {
                state.pending_update_install = true;
                state.update_status = "Verifying the installed signing certificate and opening Android installer…".into();
            }
        });
        if let Some(update) = state.available_update.as_ref() {
            ui.label(text(format!("Version {} is available.", update.version), 11.0, egui::Color32::from_rgb(119, 220, 183)));
            if !update.notes.is_empty() {
                ui.label(text(&update.notes, 10.0, egui::Color32::from_rgb(163, 174, 203)));
            }
        }
        if !state.update_status.is_empty() {
            ui.label(text(&state.update_status, 10.0, egui::Color32::from_rgb(119, 220, 183)));
        }
    });
    ui.add_space(8.0);
    surface(ui, |ui| {
        ui.label(text("AI provider keys", 16.0, egui::Color32::WHITE).strong());
        ui.label(text("Your key is encrypted with Android Keystore and stays on this device.", 11.0, egui::Color32::from_rgb(163, 174, 203)));
        egui::ComboBox::from_id_salt("cenai_provider_key")
            .selected_text(match state.provider_key_provider.as_str() {
                "deepseek" => "DeepSeek",
                "anthropic" => "Anthropic Claude",
                "gemini" => "Google Gemini",
                _ => "OpenAI-compatible",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut state.provider_key_provider, "openai".into(), "OpenAI-compatible");
                ui.selectable_value(&mut state.provider_key_provider, "deepseek".into(), "DeepSeek");
                ui.selectable_value(&mut state.provider_key_provider, "anthropic".into(), "Anthropic Claude");
                ui.selectable_value(&mut state.provider_key_provider, "gemini".into(), "Google Gemini");
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
        ui.label(text("Google Gemini account connection", 15.0, egui::Color32::WHITE).strong());
        ui.label(text("Use Google's OAuth authorization-code flow with PKCE. Your refresh token is encrypted in the account-scoped Android Keystore vault.", 10.0, egui::Color32::from_rgb(163, 174, 203)));
        ui.add(egui::TextEdit::singleline(&mut state.gemini_oauth_client_id_draft)
            .hint_text("Google OAuth client ID (.apps.googleusercontent.com)")
            .desired_width(f32::INFINITY));
        ui.horizontal(|ui| {
            if ui.button("Connect Google account").clicked() {
                state.pending_gemini_oauth_start = Some(state.gemini_oauth_client_id_draft.trim().to_owned());
                state.gemini_oauth_status = "Preparing secure Google sign-in…".into();
            }
            if ui.button("Disconnect Google").clicked() {
                state.pending_gemini_oauth_disconnect = true;
                state.gemini_oauth_status = "Disconnecting Google account…".into();
            }
        });
        if !state.gemini_oauth_status.is_empty() {
            ui.label(text(&state.gemini_oauth_status, 10.0, egui::Color32::from_rgb(119, 220, 183)));
        }
    });
    ui.add_space(8.0);
    surface(ui, |ui| {
        ui.label(text("CenAI account", 16.0, egui::Color32::WHITE).strong());
        if state.account_email.is_empty() {
            ui.label(text("Sign in with an existing CenAI account. New accounts are provisioned through the authorized CenAI Discord process; registration is not available in this app.", 11.0, egui::Color32::from_rgb(163, 174, 203)));
            ui.add(egui::TextEdit::singleline(&mut state.auth_email_draft).hint_text("Account email").desired_width(f32::INFINITY));
            ui.add(egui::TextEdit::singleline(&mut state.auth_password_draft).password(true).hint_text("Password").desired_width(f32::INFINITY));
            if ui.button("Sign in securely").clicked() {
                state.pending_sign_in = Some((state.auth_email_draft.trim().to_owned(), state.auth_password_draft.clone()));
                state.auth_password_draft.clear();
                state.auth_status = "Signing in…".into();
            }
        } else {
            ui.label(text(format!("Signed in as {}", state.account_email), 12.0, egui::Color32::from_rgb(119, 220, 183)));
            if state.email_verified {
                ui.label(text("Email verified", 11.0, egui::Color32::from_rgb(119, 220, 183)));
            } else {
                ui.label(text("Email verification required for protected account features.", 11.0, egui::Color32::from_rgb(220, 184, 121)));
                ui.horizontal(|ui| {
                    if ui.button("Send verification code").clicked() { state.pending_send_otp = true; state.otp_status = "Sending verification code…".into(); }
                });
                ui.add(egui::TextEdit::singleline(&mut state.otp_draft).hint_text("Six-digit email code").desired_width(f32::INFINITY));
                if ui.button("Verify email").clicked() { state.pending_verify_otp = Some(state.otp_draft.trim().to_owned()); state.otp_status = "Verifying email…".into(); }
                if !state.otp_status.is_empty() { ui.label(text(&state.otp_status, 10.0, egui::Color32::from_rgb(220, 184, 121))); }
            }
            ui.separator();
            ui.label(text("Multi-factor authentication", 14.0, egui::Color32::WHITE).strong());
            ui.label(text("Protect this CenAI account with a time-based authenticator code. Your authenticator secret is shown only during enrollment.", 10.0, egui::Color32::from_rgb(163, 174, 203)));
            ui.add(egui::TextEdit::singleline(&mut state.mfa_friendly_name_draft)
                .hint_text("Authenticator device name").desired_width(f32::INFINITY));
            ui.horizontal(|ui| {
                if ui.button("Refresh authenticator status").clicked() {
                    state.pending_mfa_refresh = true;
                    state.mfa_status = "Loading authenticator status…".into();
                }
                if ui.add_enabled(!state.mfa_login_required, egui::Button::new("Enroll authenticator")).clicked() {
                    state.pending_mfa_enroll = true;
                    state.mfa_status = "Creating authenticator setup…".into();
                }
            });
            if !state.mfa_enrollment_secret.is_empty() {
                ui.label(text("Add this setup key in your authenticator app:", 10.0, egui::Color32::from_rgb(220, 184, 121)));
                ui.add(egui::TextEdit::singleline(&mut state.mfa_enrollment_secret).interactive(false).desired_width(f32::INFINITY));
            }
            if !state.mfa_enrollment_factor_id.is_empty() {
                ui.label(text("Selected authenticator is ready for verification.", 10.0, egui::Color32::from_rgb(163, 174, 203)));
                ui.add(egui::TextEdit::singleline(&mut state.mfa_code_draft).hint_text("Six-digit authenticator code").desired_width(f32::INFINITY));
                if ui.add_enabled(state.mfa_code_draft.len() == 6 && !state.mfa_code_draft.chars().any(|character| !character.is_ascii_digit()), egui::Button::new("Verify authenticator code")).clicked() {
                    state.pending_mfa_verify = true;
                    state.mfa_status = "Verifying authenticator code…".into();
                }
            }
            if !state.mfa_factors.is_empty() {
                let mut selected_factor = None;
                let mut removed_factor = None;
                for factor in &state.mfa_factors {
                    ui.horizontal(|ui| {
                        ui.label(text(format!("{} · {}", factor.friendly_name, factor.status), 10.0, egui::Color32::from_rgb(190, 201, 224)));
                        if ui.small_button("Use").clicked() { selected_factor = Some(factor.id.clone()); }
                        if ui.add_enabled(!state.mfa_login_required, egui::Button::new("Remove")).clicked() { removed_factor = Some(factor.id.clone()); }
                    });
                }
                if let Some(factor_id) = selected_factor {
                    state.mfa_enrollment_factor_id = factor_id;
                    state.mfa_enrollment_secret.clear();
                    state.mfa_status = "Enter a current code from this authenticator to verify it.".into();
                }
                if let Some(factor_id) = removed_factor {
                    state.pending_mfa_unenroll = Some(factor_id);
                    state.mfa_status = "Removing authenticator…".into();
                }
            } else {
                ui.label(text("No authenticator factors were returned. Refresh status after signing in.", 10.0, egui::Color32::from_rgb(163, 174, 203)));
            }
            if !state.mfa_status.is_empty() {
                ui.label(text(&state.mfa_status, 10.0, egui::Color32::from_rgb(119, 220, 183)));
            }
            if ui.button("Sign out").clicked() {
                state.pending_sign_out = true;
                state.auth_status = "Signing out…".into();
            }
        }
        if !state.auth_status.is_empty() {
            ui.label(text(&state.auth_status, 10.0, egui::Color32::from_rgb(220, 184, 121)));
        }
    });
    if state.mfa_login_required { return; }
    ui.add_space(8.0);
    surface(ui, |ui| {
        ui.label(text("CenAI membership", 16.0, egui::Color32::WHITE).strong());
        ui.label(text("Manage membership through the secure CenAI billing service.", 11.0, egui::Color32::from_rgb(163, 174, 203)));
        ui.horizontal(|ui| {
            if ui.button("Supporter").clicked() { state.pending_checkout_plan = Some("supporter".into()); state.checkout_status = "Preparing secure checkout…".into(); }
            if ui.button("Pro").clicked() { state.pending_checkout_plan = Some("pro".into()); state.checkout_status = "Preparing secure checkout…".into(); }
        });
        if !state.checkout_status.is_empty() { ui.label(text(&state.checkout_status, 10.0, egui::Color32::from_rgb(119, 220, 183))); }
    });
    ui.add_space(8.0);
    surface(ui, |ui| {
        ui.label(text("Encrypted workspace backup", 16.0, egui::Color32::WHITE).strong());
        ui.label(text("Back up projects, tasks, automations, conversations and files. The backup is encrypted on this device before upload; CenAI storage only receives the encrypted envelope.", 11.0, egui::Color32::from_rgb(163, 174, 203)));
        ui.add(egui::TextEdit::singleline(&mut state.backup_passphrase_draft).password(true)
            .hint_text("Backup passphrase (at least 12 characters)").desired_width(f32::INFINITY));
        ui.horizontal(|ui| {
            if ui.add_enabled(!state.account_email.is_empty(), egui::Button::new("Upload encrypted backup")).clicked() {
                if state.backup_passphrase_draft.len() < 12 {
                    state.backup_status = "Use a backup passphrase of at least 12 bytes.".into();
                } else {
                    state.pending_backup_upload = true;
                    state.backup_status = "Encrypting your workspace before upload…".into();
                }
            }
            if ui.add_enabled(!state.account_email.is_empty(), egui::Button::new("Restore from cloud")).clicked() {
                if state.backup_passphrase_draft.len() < 12 {
                    state.backup_status = "Enter the original backup passphrase (at least 12 bytes).".into();
                } else if !state.backup_restore_confirmed {
                    state.backup_status = "Confirm that restoring replaces the current local workspace.".into();
                } else {
                    state.pending_backup_download = true;
                    state.backup_status = "Downloading encrypted backup…".into();
                }
            }
            if ui.add_enabled(!state.account_email.is_empty(), egui::Button::new("Restore from file")).clicked() {
                if state.backup_passphrase_draft.len() < 12 {
                    state.backup_status = "Enter the original backup passphrase (at least 12 bytes).".into();
                } else if !state.backup_restore_confirmed {
                    state.backup_status = "Confirm that restoring replaces the current local workspace.".into();
                } else {
                    state.pending_backup_file_picker = true;
                    state.backup_status = "Choose your encrypted CenAI backup file…".into();
                }
            }
        });
        ui.checkbox(&mut state.backup_restore_confirmed, "I understand restore replaces this account's current local workspace");
        if !state.backup_status.is_empty() {
            ui.label(text(&state.backup_status, 10.0, egui::Color32::from_rgb(119, 220, 183)));
        }
    });
    ui.add_space(8.0);
    surface(ui, |ui| {
        ui.label(text("Cloud code sandbox", 16.0, egui::Color32::WHITE).strong());
        ui.label(text("Run development commands in a separate Linux microVM with a private, per-account snapshot workspace. Runtime limits: 1 vCPU, 2 GB RAM and 55 seconds per command. Network access is restricted to approved development registries and GitHub. Credentials and environment variables are never forwarded.", 10.0, egui::Color32::from_rgb(163, 174, 203)));
        ui.label(text("Command", 11.0, egui::Color32::WHITE));
        ui.text_edit_singleline(&mut state.sandbox_command_draft);
        ui.label(text("Arguments (JSON string array)", 11.0, egui::Color32::WHITE));
        ui.add(egui::TextEdit::multiline(&mut state.sandbox_arguments_draft).desired_rows(2));
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(text("Project ID", 10.0, egui::Color32::from_rgb(163, 174, 203)));
                ui.text_edit_singleline(&mut state.sandbox_project_id_draft);
            });
            ui.vertical(|ui| {
                ui.label(text("Working directory", 10.0, egui::Color32::from_rgb(163, 174, 203)));
                ui.text_edit_singleline(&mut state.sandbox_working_directory_draft);
            });
        });
        if ui.button("Run in isolated cloud sandbox").clicked() {
            state.pending_sandbox_run = true;
            state.sandbox_status = "Waiting to start isolated execution…".into();
        }
        if !state.sandbox_status.is_empty() {
            ui.label(text(&state.sandbox_status, 10.0, egui::Color32::from_rgb(119, 220, 183)));
        }
        if !state.sandbox_output.is_empty() {
            egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| ui.monospace(&state.sandbox_output));
            if ui.small_button("Clear sandbox output").clicked() { state.sandbox_output.clear(); }
        }
    });
    ui.add_space(8.0);
    surface(ui, |ui| {
        ui.label(text("Privacy & security", 16.0, egui::Color32::WHITE).strong());
        ui.label(text("Per-user isolation, encrypted credentials and explicit approvals are part of the runtime design.", 12.0, egui::Color32::from_rgb(163, 174, 203)));
    });
}

pub fn render(ctx: &egui::Context, state: &mut AppState, time_seconds: f32) {
    if state.mfa_login_required { state.destination = Destination::Settings; }
    LIGHT_MODE.with(|light| light.set(state.light_mode));
    ctx.set_visuals(if state.light_mode { egui::Visuals::light() } else { egui::Visuals::dark() });
    ctx.style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        style.visuals.panel_fill = egui::Color32::TRANSPARENT;
        style.visuals.window_fill = if state.light_mode { egui::Color32::from_rgb(247, 249, 255) } else { egui::Color32::from_rgb(18, 23, 40) };
        style.visuals.widgets.inactive.bg_fill = if state.light_mode { egui::Color32::from_rgb(232, 237, 249) } else { egui::Color32::from_rgb(31, 38, 62) };
        style.visuals.widgets.hovered.bg_fill = if state.light_mode { egui::Color32::from_rgb(218, 226, 249) } else { egui::Color32::from_rgb(53, 64, 104) };
        style.visuals.widgets.active.bg_fill = if state.light_mode { egui::Color32::from_rgb(202, 214, 248) } else { egui::Color32::from_rgb(76, 91, 157) };
        style.visuals.selection.bg_fill = egui::Color32::from_rgb(76, 91, 157);
    });
    background(ctx, time_seconds);

    egui::TopBottomPanel::top("cenai_header")
        .frame(egui::Frame::new().fill(if state.light_mode { egui::Color32::from_rgba_unmultiplied(250, 252, 255, 236) } else { egui::Color32::from_rgba_unmultiplied(12, 16, 30, 224) }).inner_margin(egui::Margin::symmetric(18, 14)))
        .show(ctx, top_bar);

    egui::TopBottomPanel::bottom("cenai_navigation")
        .frame(egui::Frame::new().fill(if state.light_mode { egui::Color32::from_rgba_unmultiplied(250, 252, 255, 242) } else { egui::Color32::from_rgba_unmultiplied(12, 16, 30, 240) }).inner_margin(egui::Margin::symmetric(8, 8)))
        .show(ctx, |ui| navigation(ui, state));

    egui::CentralPanel::default()
        .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(18, 10)))
        .show(ctx, |ui| {
            let available_width = ui.available_width();
            let content_width = available_width.min(780.0);
            ui.horizontal(|ui| {
                ui.add_space(((available_width - content_width) * 0.5).max(0.0));
                ui.allocate_ui_with_layout(
                    egui::vec2(content_width, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                            match state.destination {
                                Destination::Conversation => conversation(ui, state),
                                Destination::YourWork => work(ui, state),
                                Destination::Connections => connections(ui, state),
                                Destination::Settings => settings(ui, state),
                            }
                        });
                    },
                );
            });
        });

    if let Some((tool_name, arguments)) = state.pending_agent_action.clone() {
        let mut open = true;
        let mut approved = false;
        let mut denied = false;
        let is_sandbox = tool_name == "sandbox_run";
        egui::Window::new(if is_sandbox { "Approve cloud sandbox command" } else { "Approve Android action" })
            .collapsible(false)
            .resizable(true)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(text(if is_sandbox { "CenAI wants to execute a command in your private isolated cloud workspace." } else { "CenAI is requesting permission to perform this action on your device." }, 12.0, egui::Color32::WHITE));
                ui.add_space(6.0);
                ui.label(text(format!("Action: {tool_name}"), 11.0, accent()).strong());
                let mut details = serde_json::to_string_pretty(&arguments).unwrap_or_else(|_| "{}".into());
                egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                    ui.add(egui::TextEdit::multiline(&mut details).desired_width(360.0).interactive(false));
                });
                ui.add_space(8.0);
                ui.label(text(if is_sandbox { "This runs remotely, not on your Android device. It can modify this account's saved workspace, has restricted network access, and receives no device permissions or credentials." } else { "Approve only if the destination and content are correct. CenAI will not place calls; the dialer opens without calling." }, 10.0, egui::Color32::from_rgb(220, 184, 121)));
                ui.horizontal(|ui| {
                    if ui.button("Approve once").clicked() { approved = true; }
                    if ui.button("Deny").clicked() { denied = true; }
                });
            });
        if approved {
            state.pending_agent_action_execute = Some((tool_name, arguments));
            state.pending_agent_action = None;
            state.agent_action_status = if is_sandbox { "Cloud sandbox command approved once. Starting remote execution…".into() } else { "Action approved once. Waiting for Android to open the requested destination…".into() };
        } else if denied || !open {
            state.pending_agent_action = None;
            state.agent_action_status = if is_sandbox { "Cloud sandbox command denied. Nothing was executed.".into() } else { "Android action denied. Nothing was opened or shared.".into() };
        }
    }
}
