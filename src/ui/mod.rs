use crate::sim::{AgentNode, CrystalGrid, LinkKind, TileState};
use crate::verilog::{export_verilog_files, generate_verilog, VerilogBundle};
use eframe::egui;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Ru,
    En,
}

impl Lang {
    fn t<'a>(&self, ru: &'a str, en: &'a str) -> &'a str {
        match self {
            Lang::Ru => ru,
            Lang::En => en,
        }
    }
}

pub struct FrostbiteApp {
    grid: CrystalGrid,
    running: bool,
    auto_stop: bool,
    steps_per_frame: usize,
    grid_size: usize,
    expr_input: String,
    selected_tile: Option<usize>,
    show_verilog: bool,
    verilog_bundle: Option<VerilogBundle>,
    verilog_status: Option<String>,
    lang: Lang,
}

impl Default for FrostbiteApp {
    fn default() -> Self {
        let mut grid = CrystalGrid::new(4);
        let default_expr = "fib(7) + (15 * 4) - 10".to_string();
        grid.submit_expression(&default_expr, 0, 0);
        Self {
            grid,
            running: false,
            auto_stop: true,
            steps_per_frame: 1,
            grid_size: 4,
            expr_input: default_expr,
            selected_tile: Some(0),
            show_verilog: false,
            verilog_bundle: None,
            verilog_status: None,
            lang: Lang::Ru,
        }
    }
}

impl eframe::App for FrostbiteApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.running {
            for _ in 0..self.steps_per_frame {
                if self.auto_stop && self.grid.final_result.is_some() {
                    self.running = false;
                    break;
                }
                self.grid.step();
                if self.auto_stop && self.grid.final_result.is_some() {
                    self.running = false;
                    break;
                }
            }
            ctx.request_repaint();
        }

        let l = self.lang;

        egui::SidePanel::left("control_panel")
            .min_width(275.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Frostbite Graph Sim");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.selectable_value(&mut self.lang, Lang::En, "EN");
                        ui.selectable_value(&mut self.lang, Lang::Ru, "RU");
                    });
                });
                ui.label("Interaction Nets + Work-Stealing");
                ui.separator();

                ui.label(l.t(
                    "Задача (арифметика, fib(n), fact(n)):",
                    "Task (arithmetic, fib(n), fact(n)):",
                ));
                let text_resp = ui.add(
                    egui::TextEdit::singleline(&mut self.expr_input)
                        .hint_text("fib(8) * 2 + 14")
                        .desired_width(f32::INFINITY),
                );
                let enter_pressed =
                    text_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

                ui.horizontal(|ui| {
                    if ui.button(l.t("⚡ Загрузить в [0,0]", "⚡ Load at [0,0]")).clicked()
                        || enter_pressed
                    {
                        self.grid = CrystalGrid::new(self.grid_size);
                        self.grid.submit_expression(&self.expr_input, 0, 0);
                        self.running = false;
                    }
                    ui.checkbox(&mut self.auto_stop, l.t("Автостоп", "Auto-stop"));
                });

                ui.horizontal_wrapped(|ui| {
                    for preset in ["fib(8)", "fact(7)", "fib(6) + fib(7)"] {
                        if ui.small_button(preset).clicked() {
                            self.expr_input = preset.to_string();
                            self.grid = CrystalGrid::new(self.grid_size);
                            self.grid.submit_expression(&self.expr_input, 0, 0);
                            self.running = false;
                        }
                    }
                });

                if let Some(res) = self.grid.final_result {
                    ui.add_space(4.0);
                    ui.group(|ui| {
                        ui.colored_label(
                            egui::Color32::from_rgb(72, 187, 120),
                            format!("{}: {}", l.t("✅ Ответ кристалла", "✅ Crystal result"), res),
                        );
                    });
                }
                if let Some(err) = &self.grid.last_error {
                    ui.colored_label(egui::Color32::LIGHT_RED, format!("❌ {}", err));
                }

                ui.separator();
                ui.horizontal(|ui| {
                    let play_lbl = if self.running {
                        l.t("⏸ Пауза", "⏸ Pause")
                    } else {
                        l.t("▶ Старт", "▶ Start")
                    };
                    if ui.button(play_lbl).clicked() {
                        self.running = !self.running;
                    }
                    if ui.button(l.t("⏭ Шаг", "⏭ Step")).clicked() {
                        self.running = false;
                        self.grid.step();
                    }
                    if ui.button(l.t("🔄 Сброс", "🔄 Reset")).clicked() {
                        self.grid = CrystalGrid::new(self.grid_size);
                        self.running = false;
                    }
                });
                ui.add_space(4.0);
                ui.add(
                    egui::Slider::new(&mut self.steps_per_frame, 1..=15)
                        .text(l.t("Тактов / кадр", "Ticks / frame")),
                );

                ui.horizontal(|ui| {
                    ui.label(l.t("Матрица:", "Matrix:"));
                    for sz in [4usize, 6, 8] {
                        if ui
                            .selectable_label(self.grid_size == sz, format!("{sz}x{sz}"))
                            .clicked()
                        {
                            self.grid_size = sz;
                            self.grid = CrystalGrid::new(sz);
                            self.grid.submit_expression(&self.expr_input, 0, 0);
                            self.running = false;
                        }
                    }
                });

                ui.separator();
                ui.heading(l.t("Телеметрия кристалла", "Crystal Telemetry"));
                ui.label(format!("{}: {}", l.t("Такт (Tick)", "Tick"), self.grid.tick));
                ui.label(format!(
                    "{}: {}",
                    l.t("Редукций графа", "Graph reductions"),
                    self.grid.total_reductions
                ));
                ui.label(format!(
                    "{}: {:.1} pJ",
                    l.t("Энергия Frostbite", "Frostbite energy"),
                    self.grid.total_energy
                ));
                ui.label(format!(
                    "{}: {:.1} pJ",
                    l.t("Классический CPU", "Classical CPU"),
                    self.grid.classical_cpu_energy
                ));

                let ratio = if self.grid.total_energy > 0.0 {
                    self.grid.classical_cpu_energy / self.grid.total_energy
                } else {
                    0.0
                };
                ui.colored_label(
                    egui::Color32::LIGHT_GREEN,
                    format!("{}: {:.2}x", l.t("Выигрыш по энергии", "Energy gain"), ratio),
                );

                ui.separator();
                ui.label(l.t("Легенда сетки и потоков:", "Grid & flow legend:"));
                ui.colored_label(
                    egui::Color32::from_rgb(80, 95, 118),
                    l.t("■ Сон / Ожидание", "■ Sleeping / Idle"),
                );
                ui.colored_label(
                    egui::Color32::from_rgb(56, 178, 172),
                    l.t("■ Редукция узла", "■ Node reduction"),
                );
                ui.colored_label(
                    egui::Color32::from_rgb(237, 137, 54),
                    l.t("■ Перехват (Work-Stealing)", "■ Work-Stealing"),
                );
                ui.colored_label(
                    egui::Color32::from_rgb(246, 173, 85),
                    l.t("➔ Переброска ветки графа", "➔ Branch transfer"),
                );
                ui.colored_label(
                    egui::Color32::from_rgb(79, 209, 197),
                    l.t("➔ Возврат результата", "➔ Result return"),
                );

                ui.separator();
                if ui
                    .button(l.t(
                        "🛠 Сгенерировать Verilog (RTL + TB)",
                        "🛠 Generate Verilog (RTL + TB)",
                    ))
                    .clicked()
                {
                    let bundle = generate_verilog(self.grid_size, &self.expr_input);
                    self.verilog_status = export_verilog_files("verilog_out", &bundle).ok();
                    self.verilog_bundle = Some(bundle);
                    self.show_verilog = true;
                }
                if let Some(status) = &self.verilog_status {
                    ui.colored_label(egui::Color32::LIGHT_GREEN, format!("✔ {status}"));
                }
            });
        // ПРАВАЯ ПАНЕЛЬ: ЖИВОЙ ГРАФ INTERACTION NETS
        egui::SidePanel::right("graph_panel")
            .min_width(340.0)
            .show(ctx, |ui| {
                ui.heading(l.t("Живой граф Interaction Nets", "Live Interaction Nets Graph"));
                if let Some(sel) = self.selected_tile {
                    let (sx, sy) = self.grid.coords(sel);
                    if let Some(t) = self.grid.tiles.get(sel) {
                        ui.label(format!(
                            "{} [{sx},{sy}] | {}: {} | {}: {} | {:.2} pJ",
                            l.t("Выбрана Tile", "Selected Tile"),
                            l.t("Редукций", "Reductions"),
                            t.reductions_done,
                            l.t("Пересылок", "Transfers"),
                            t.transfers_done,
                            t.energy_used
                        ));
                    }
                }
                ui.separator();

                let mut all_nodes: Vec<(usize, &AgentNode)> = Vec::new();
                for (t_idx, tile) in self.grid.tiles.iter().enumerate() {
                    for node in &tile.tasks {
                        all_nodes.push((t_idx, node));
                    }
                }
                all_nodes.sort_by_key(|(_, n)| n.id);

                let avail = ui.available_size();
                let (resp, painter) = ui.allocate_painter(
                    egui::vec2(avail.x, avail.y.max(220.0)),
                    egui::Sense::hover(),
                );
                let rect = resp.rect;
                painter.rect_filled(rect, 6.0, egui::Color32::from_rgb(22, 27, 36));

                if all_nodes.is_empty() {
                    let msg = if let Some(res) = self.grid.final_result {
                        format!(
                            "{}\n{}: {}",
                            l.t("Граф полностью редуцирован!", "Graph fully reduced!"),
                            l.t("Итоговый ответ", "Final result"),
                            res
                        )
                    } else {
                        l.t("Граф пуст — загрузите задачу", "Graph empty — load a task")
                            .to_string()
                    };
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        msg,
                        egui::FontId::proportional(15.0),
                        egui::Color32::from_rgb(160, 174, 192),
                    );
                } else {
                    let max_draw = all_nodes.len().min(36);
                    let cols = (max_draw as f32).sqrt().ceil() as usize;
                    let rows = (max_draw + cols - 1) / cols;
                    let cell_w = rect.width() / cols as f32;
                    let cell_h = rect.height() / rows as f32;

                    let mut positions: HashMap<u64, egui::Pos2> = HashMap::new();
                    for (i, (_, node)) in all_nodes.iter().take(max_draw).enumerate() {
                        let c = i % cols;
                        let r = i / cols;
                        positions.insert(
                            node.id,
                            egui::pos2(
                                rect.min.x + (c as f32 + 0.5) * cell_w,
                                rect.min.y + (r as f32 + 0.5) * cell_h,
                            ),
                        );
                    }

                    for (_, node) in all_nodes.iter().take(max_draw) {
                        if let (Some((pid, _)), Some(&from_p)) =
                            (node.parent, positions.get(&node.id))
                        {
                            if let Some(&to_p) = positions.get(&pid) {
                                painter.line_segment(
                                    [from_p, to_p],
                                    egui::Stroke::new(1.5, egui::Color32::from_rgb(100, 120, 150)),
                                );
                            }
                        }
                    }

                    for (t_idx, node) in all_nodes.iter().take(max_draw) {
                        if let Some(&pos) = positions.get(&node.id) {
                            let (tx, ty) = self.grid.coords(*t_idx);
                            let fill = if self.selected_tile == Some(*t_idx) {
                                egui::Color32::from_rgb(49, 151, 149)
                            } else {
                                egui::Color32::from_rgb(45, 55, 72)
                            };
                            painter.circle_filled(pos, 22.0, fill);
                            painter.circle_stroke(
                                pos,
                                22.0,
                                egui::Stroke::new(1.5, egui::Color32::from_rgb(130, 150, 180)),
                            );
                            let args_str: Vec<String> = node
                                .args
                                .iter()
                                .map(|a| a.map(|v| v.to_string()).unwrap_or_else(|| "?".into()))
                                .collect();
                            painter.text(
                                pos - egui::vec2(0.0, 5.0),
                                egui::Align2::CENTER_CENTER,
                                format!("{}({})", node.op.label(), args_str.join(",")),
                                egui::FontId::monospace(10.5),
                                egui::Color32::WHITE,
                            );
                            painter.text(
                                pos + egui::vec2(0.0, 8.0),
                                egui::Align2::CENTER_CENTER,
                                format!("[{tx},{ty}]"),
                                egui::FontId::monospace(9.5),
                                egui::Color32::from_rgb(246, 173, 85),
                            );
                        }
                    }
                }
            });

        // ЦЕНТРАЛЬНАЯ ПАНЕЛЬ: МАТРИЦА ПЛИТОК
        egui::CentralPanel::default().show(ctx, |ui| {
            let avail = ui.available_size();
            let side = avail.x.min(avail.y) - 20.0;
            let n = self.grid.size as f32;
            let gap = 8.0;
            let tile_size = ((side - gap * (n - 1.0)) / n).max(30.0);

            let (response, painter) =
                ui.allocate_painter(egui::vec2(side, side), egui::Sense::click());
            let origin = response.rect.min;
            let mut centers = vec![egui::Pos2::ZERO; self.grid.tiles.len()];

            for y in 0..self.grid.size {
                for x in 0..self.grid.size {
                    let min_x = origin.x + x as f32 * (tile_size + gap);
                    let min_y = origin.y + y as f32 * (tile_size + gap);
                    let rect = egui::Rect::from_min_size(
                        egui::pos2(min_x, min_y),
                        egui::vec2(tile_size, tile_size),
                    );
                    let id = self.grid.idx(x, y);
                    centers[id] = rect.center();

                    if response.clicked() {
                        if let Some(pos) = response.interact_pointer_pos() {
                            if rect.contains(pos) {
                                self.selected_tile = Some(id);
                                self.grid = CrystalGrid::new(self.grid_size);
                                self.grid.submit_expression(&self.expr_input, x, y);
                                self.running = false;
                            }
                        }
                    }

                    let tile = &self.grid.tiles[id];
                    let base_color = match tile.state {
                        TileState::Sleeping => egui::Color32::from_rgb(38, 46, 61),
                        TileState::Computing => egui::Color32::from_rgb(45, 165, 158),
                        TileState::Stealing => egui::Color32::from_rgb(225, 130, 48),
                    };
                    painter.rect_filled(rect, 8.0, base_color);
                    let border = if self.selected_tile == Some(id) {
                        egui::Stroke::new(2.5, egui::Color32::WHITE)
                    } else {
                        egui::Stroke::new(1.5, egui::Color32::from_rgb(90, 105, 130))
                    };
                    painter.rect_stroke(rect, 8.0, border);
                    painter.text(
                        rect.center() - egui::vec2(0.0, 8.0),
                        egui::Align2::CENTER_CENTER,
                        format!("Tile [{x},{y}]"),
                        egui::FontId::proportional(13.0),
                        egui::Color32::WHITE,
                    );
                    painter.text(
                        rect.center() + egui::vec2(0.0, 10.0),
                        egui::Align2::CENTER_CENTER,
                        format!("{}: {}", l.t("Узлов", "Nodes"), tile.tasks.len()),
                        egui::FontId::monospace(12.0),
                        egui::Color32::from_rgb(230, 235, 245),
                    );
                }
            }

            for link in &self.grid.active_links {
                let p1 = centers[link.from_tile];
                let p2 = centers[link.to_tile];
                let color = match link.kind {
                    LinkKind::Steal => egui::Color32::from_rgb(246, 173, 85),
                    LinkKind::Result => egui::Color32::from_rgb(79, 209, 197),
                };
                painter.line_segment([p1, p2], egui::Stroke::new(3.0, color));
                let dir = (p2 - p1).normalized();
                let perp = egui::vec2(-dir.y, dir.x);
                let tip = p2 - dir * 14.0;
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        tip,
                        tip - dir * 10.0 + perp * 5.0,
                        tip - dir * 10.0 - perp * 5.0,
                    ],
                    color,
                    egui::Stroke::NONE,
                ));
            }
        });

        // ОКНО ЭКСПОРТА VERILOG
        if self.show_verilog {
            let mut open = self.show_verilog;
            egui::Window::new(l.t(
                "Экспорт синтезируемого SystemVerilog",
                "Synthesizable SystemVerilog Export",
            ))
            .open(&mut open)
            .default_size([620.0, 440.0])
            .show(ctx, |ui| {
                if let Some(bundle) = &self.verilog_bundle {
                    ui.label(l.t(
                        "Файлы сохранены в ./verilog_out/ (готовы для Verilator / Icarus Verilog / FPGA):",
                        "Saved to ./verilog_out/ (ready for Verilator / Icarus Verilog / FPGA):",
                    ));
                    ui.separator();
                    egui::ScrollArea::vertical().max_height(340.0).show(ui, |ui| {
                        ui.heading("frostbite_grid.sv (RTL)");
                        ui.add(
                            egui::TextEdit::multiline(&mut bundle.rtl_code.as_str())
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY),
                        );
                        ui.separator();
                        ui.heading("frostbite_tb.sv (Testbench)");
                        ui.add(
                            egui::TextEdit::multiline(&mut bundle.tb_code.as_str())
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY),
                        );
                    });
                }
            });
            self.show_verilog = open;
        }
    }
}

