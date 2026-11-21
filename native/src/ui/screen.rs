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
            let selected = state.destination == destination;
            let fill = if selected {
                egui::Color32::from_rgb(49, 59, 104)
            } else {
                egui::Color32::TRANSPARENT
            };
            let color = if selected { egui::Color32::WHITE } else { egui::Color32::from_rgb(145, 154, 180) };
            if ui.add_sized(
                [ui.available_width() / 4.0, 54.0],
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
                    });
                }
            });
            ui.add_space(8.0);
        }
    }
    if let Some(artifact_id) = open_artifact {
        state.pending_artifact_open = Some(artifact_id);
        state.artifact_preview_status = "Opening artifact…".into();
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
                ui.label(text(format!("{} steps · Plan only · Not executed", plan.steps.len()), 10.0, egui::Color32::from_rgb(140, 154, 190)));
                for step in &plan.steps {
                    ui.label(text(format!("○  {:?} — {}", step.role, step.title), 11.0, egui::Color32::from_rgb(163, 174, 203)));
                }
            });
            ui.add_space(8.0);
        }
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
                    requested_scopes: Vec::new(),
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
    ui.add_space(10.0);
    ui.label(text("Settings", 26.0, egui::Color32::WHITE).strong());
    ui.label(text("Make CenAI feel right for you.", 13.0, egui::Color32::from_rgb(163, 174, 203)));
    ui.add_space(18.0);
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
            let ram_percent = if snapshot.ram_total_kb > 0 { ram_used.saturating_mul(100) / snapshot.ram_total_kb } else { 0 };
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
            if ui.button("Sign out").clicked() {
                state.pending_sign_out = true;
                state.auth_status = "Signing out…".into();
            }
        }
        if !state.auth_status.is_empty() {
            ui.label(text(&state.auth_status, 10.0, egui::Color32::from_rgb(220, 184, 121)));
        }
    });
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
        ui.label(text("Privacy & security", 16.0, egui::Color32::WHITE).strong());
        ui.label(text("Per-user isolation, encrypted credentials and explicit approvals are part of the runtime design.", 12.0, egui::Color32::from_rgb(163, 174, 203)));
    });
}

pub fn render(ctx: &egui::Context, state: &mut AppState, time_seconds: f32) {
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
}
