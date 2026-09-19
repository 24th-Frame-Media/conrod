//! Conrod, the native app.
//!
//! Scan a folder with a scan type, watch the cull stream in, review it on
//! the keyboard. Everything running shows in the status area, top right.

#![windows_subsystem = "windows"]

use conrod_core::profile::ScanProfile;
use conrod_core::settings::Settings;
use conrod_core::tasks::{State, TaskHub};
use conrod_engine::FrameResult;
use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Conrod")
            .with_inner_size([1440.0, 920.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Conrod",
        options,
        Box::new(|cc| Ok(Box::new(App::new(&cc.egui_ctx)))),
    )
}

#[derive(PartialEq, Clone, Copy)]
enum Screen {
    Scan,
    Review,
    Settings,
}

struct App {
    screen: Screen,
    hub: TaskHub,
    settings: Settings,
    folder: String,
    profile: ScanProfile,
    results: Arc<Mutex<Vec<FrameResult>>>,
    scan: Option<conrod_engine::Scan>,
    textures: HashMap<usize, egui::TextureHandle>,
    selected: usize,
    min_stars: u8,
    /// Hand ratings and rejects, by result index. They win over the cull.
    stars: HashMap<usize, u8>,
    rejected: HashSet<usize>,
    ctx: egui::Context,
}

impl App {
    fn new(ctx: &egui::Context) -> App {
        let settings = Settings::load(&Settings::path());
        let profile = ScanProfile::parse(&settings.scan_profile);
        App {
            screen: Screen::Scan,
            hub: TaskHub::new(),
            settings,
            folder: String::new(),
            profile,
            results: Arc::default(),
            scan: None,
            textures: HashMap::new(),
            selected: 0,
            min_stars: 0,
            stars: HashMap::new(),
            rejected: HashSet::new(),
            ctx: ctx.clone(),
        }
    }

    fn start_scan(&mut self) {
        self.results.lock().unwrap().clear();
        self.textures.clear();
        self.stars.clear();
        self.rejected.clear();
        self.selected = 0;
        let results = self.results.clone();
        let ctx = self.ctx.clone();
        self.scan = Some(conrod_engine::scan(
            PathBuf::from(self.folder.trim()),
            self.settings.clone(),
            self.profile,
            self.hub.clone(),
            move |frame| {
                results.lock().unwrap().push(frame);
                ctx.request_repaint();
            },
        ));
        self.screen = Screen::Review;
    }

    fn stars_of(&self, i: usize, frame: &FrameResult) -> u8 {
        self.stars.get(&i).copied().unwrap_or(frame.stars)
    }

    fn texture(&mut self, i: usize, frame: &FrameResult) -> egui::TextureHandle {
        self.textures
            .entry(i)
            .or_insert_with(|| {
                let t = &frame.thumb;
                let image = egui::ColorImage::from_rgb([t.width, t.height], &t.data);
                self.ctx
                    .load_texture(format!("thumb{i}"), image, egui::TextureOptions::LINEAR)
            })
            .clone()
    }

    // --- status area --------------------------------------------------------

    fn status(&self, ui: &mut egui::Ui) {
        let tasks = self.hub.snapshot();
        let running: Vec<_> = tasks
            .iter()
            .filter(|t| matches!(t.state, State::Running | State::Paused))
            .collect();
        let failed = tasks.iter().any(|t| t.state == State::Failed);
        let pill = match running.first() {
            Some(t) if t.total > 0 => format!("⟳ {} {}/{}", t.label, t.done, t.total),
            Some(t) => format!("⟳ {}", t.label),
            None if failed => "⚠ Needs attention".to_string(),
            None => "✔ Idle".to_string(),
        };
        ui.menu_button(pill, |ui| {
            ui.set_min_width(420.0);
            ui.label(egui::RichText::new(format!("Scan type: {}", self.profile.label())).weak());
            ui.separator();
            if tasks.is_empty() {
                ui.label("Nothing has run yet.");
            }
            for t in &tasks {
                let mark = match t.state {
                    State::Running => "⟳",
                    State::Paused => "⏸",
                    State::Done => "✔",
                    State::Failed => "⚠",
                };
                ui.horizontal(|ui| {
                    ui.label(format!("{mark} {}", t.label));
                    if t.total > 0 {
                        ui.add(
                            egui::ProgressBar::new(t.done as f32 / t.total as f32)
                                .desired_width(120.0)
                                .text(format!("{}/{}", t.done, t.total)),
                        );
                    }
                    if let Some(eta) = t.eta {
                        ui.label(format!("{}s left", eta.as_secs()));
                    }
                });
                if let Some(e) = &t.error {
                    ui.label(egui::RichText::new(e).color(egui::Color32::LIGHT_RED));
                }
            }
            ui.separator();
            ui.collapsing("Log", |ui| {
                for line in self.hub.log().iter().rev().take(40) {
                    ui.label(egui::RichText::new(line).small());
                }
            });
        });
        // Keep the pill live while anything runs.
        if !running.is_empty() {
            self.ctx
                .request_repaint_after(std::time::Duration::from_millis(250));
        }
    }

    // --- screens ------------------------------------------------------------

    fn scan_screen(&mut self, ui: &mut egui::Ui) {
        ui.heading("Scan a shoot");
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.folder)
                    .hint_text("D:\\Shoots\\2026\\09\\19")
                    .desired_width(520.0),
            );
            if ui.button("Browse…").clicked() {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    self.folder = dir.display().to_string();
                }
            }
        });
        ui.add_space(8.0);
        ui.label("What kind of shoot is it?");
        ui.horizontal(|ui| {
            for p in ScanProfile::ALL {
                ui.selectable_value(&mut self.profile, p, p.label());
            }
        });
        ui.label(
            egui::RichText::new(match self.profile {
                ScanProfile::Motorsport => {
                    "Judges the car; forgives a streaked background on a pan."
                }
                ScanProfile::Portrait => {
                    "Judges faces and eyes; soft backgrounds are bokeh, not blur."
                }
                ScanProfile::Event => {
                    "Judges eyes, faces, people and vehicles, whichever is there."
                }
                ScanProfile::Mix => "Everything, frame by frame.",
            })
            .weak(),
        );
        ui.add_space(12.0);
        let running = self.scan.is_some()
            && self
                .hub
                .snapshot()
                .iter()
                .any(|t| t.label.starts_with("Culling") && t.state == State::Running);
        ui.horizontal(|ui| {
            let ready =
                !self.folder.trim().is_empty() && PathBuf::from(self.folder.trim()).is_dir();
            if ui
                .add_enabled(ready && !running, egui::Button::new("Start scan"))
                .clicked()
            {
                self.start_scan();
            }
            if running && ui.button("Stop").clicked() {
                if let Some(scan) = &self.scan {
                    scan.stop.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
        });
    }

    fn review_screen(&mut self, ui: &mut egui::Ui) {
        let frames: Vec<FrameResult> = self.results.lock().unwrap().clone();
        let visible: Vec<usize> = (0..frames.len())
            .filter(|&i| self.stars_of(i, &frames[i]) >= self.min_stars)
            .collect();
        self.keys(ui, &frames, &visible);

        ui.horizontal(|ui| {
            ui.label(format!("{} frames", frames.len()));
            ui.separator();
            ui.label("Show from");
            for s in 0..=5u8 {
                let text = if s == 0 {
                    "all".to_string()
                } else {
                    format!("{s}★")
                };
                ui.selectable_value(&mut self.min_stars, s, text);
            }
            ui.separator();
            ui.label(egui::RichText::new("←→ move · 0–5 rate · X reject").weak());
        });
        ui.separator();

        egui::Panel::right("viewer")
            .default_size(560.0)
            .show(ui, |ui| {
                if let Some(frame) = frames.get(self.selected) {
                    let texture = self.texture(self.selected, frame);
                    let size = texture.size_vec2();
                    let scale = (ui.available_width() / size.x).min(1.6);
                    let response = ui.image((texture.id(), size * scale));
                    // The subjects' boxes, drawn over the photo.
                    let rect = response.rect;
                    let (fw, fh) = (frame.size.0 as f32, frame.size.1 as f32);
                    for s in &frame.subjects {
                        let b = s.bbox;
                        let r = egui::Rect::from_min_max(
                            rect.min
                                + egui::vec2(
                                    b[0] as f32 / fw * rect.width(),
                                    b[1] as f32 / fh * rect.height(),
                                ),
                            rect.min
                                + egui::vec2(
                                    b[2] as f32 / fw * rect.width(),
                                    b[3] as f32 / fh * rect.height(),
                                ),
                        );
                        let colour = if s.cull_reason.is_empty() {
                            egui::Color32::GREEN
                        } else {
                            egui::Color32::RED
                        };
                        ui.painter()
                            .rect_stroke(r, 2.0, (2.0, colour), egui::StrokeKind::Outside);
                    }
                    ui.label(frame.path.display().to_string());
                    ui.label(format!(
                        "{} · {}★",
                        frame.camera,
                        self.stars_of(self.selected, frame)
                    ));
                    for s in &frame.subjects {
                        let mut line =
                            format!("{} — sharpness {:.2}, {}★", s.class, s.sharpness, s.stars);
                        if s.panning {
                            line += " · pan";
                        }
                        if !s.cull_reason.is_empty() {
                            line += &format!(" · culled: {}", s.cull_reason);
                        }
                        ui.label(line);
                    }
                    if let Some(w) = frame.whole {
                        ui.label(format!("no subject — whole frame {w:.2}"));
                    }
                    if self.rejected.contains(&self.selected) {
                        ui.colored_label(egui::Color32::LIGHT_RED, "Rejected");
                    }
                } else {
                    ui.label("Frames appear here as they are culled.");
                }
            });

        let tile = 180.0;
        let columns = ((ui.available_width() / (tile + 8.0)).floor() as usize).max(1);
        let rows = visible.len().div_ceil(columns);
        egui::ScrollArea::vertical().show_rows(ui, tile + 24.0, rows, |ui, range| {
            for row in range {
                ui.horizontal(|ui| {
                    for &i in visible.iter().skip(row * columns).take(columns) {
                        let frame = &frames[i];
                        let texture = self.texture(i, frame);
                        let size = texture.size_vec2();
                        let scale = tile / size.x.max(size.y);
                        ui.vertical(|ui| {
                            ui.set_width(tile);
                            let image = egui::Image::new((texture.id(), size * scale))
                                .sense(egui::Sense::click());
                            let response = ui.add(image);
                            if i == self.selected {
                                ui.painter().rect_stroke(
                                    response.rect,
                                    2.0,
                                    (3.0, egui::Color32::LIGHT_BLUE),
                                    egui::StrokeKind::Outside,
                                );
                            }
                            if self.rejected.contains(&i) {
                                ui.painter().rect_filled(
                                    response.rect,
                                    0.0,
                                    egui::Color32::from_black_alpha(150),
                                );
                            }
                            if response.clicked() {
                                self.selected = i;
                            }
                            let mut caption = "★".repeat(self.stars_of(i, frame) as usize);
                            if frame.subjects.iter().any(|s| s.panning) {
                                caption += " pan";
                            }
                            ui.label(caption);
                        });
                    }
                });
            }
        });
    }

    fn keys(&mut self, ui: &egui::Ui, frames: &[FrameResult], visible: &[usize]) {
        if ui.ctx().memory(|m| m.focused().is_some()) || visible.is_empty() {
            return;
        }
        let at = visible
            .iter()
            .position(|&i| i == self.selected)
            .unwrap_or(0);
        ui.input(|input| {
            if input.key_pressed(egui::Key::ArrowRight) || input.key_pressed(egui::Key::J) {
                self.selected = visible[(at + 1).min(visible.len() - 1)];
            }
            if input.key_pressed(egui::Key::ArrowLeft) || input.key_pressed(egui::Key::K) {
                self.selected = visible[at.saturating_sub(1)];
            }
            let digits = [
                egui::Key::Num0,
                egui::Key::Num1,
                egui::Key::Num2,
                egui::Key::Num3,
                egui::Key::Num4,
                egui::Key::Num5,
            ];
            for (n, key) in digits.iter().enumerate() {
                if input.key_pressed(*key) && self.selected < frames.len() {
                    if n == 0 {
                        self.stars.remove(&self.selected); // back to the cull's rating
                    } else {
                        self.stars.insert(self.selected, n as u8);
                    }
                }
            }
            if input.key_pressed(egui::Key::X) && !self.rejected.remove(&self.selected) {
                self.rejected.insert(self.selected);
            }
        });
    }

    fn settings_screen(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings");
        let s = &mut self.settings;
        egui::Grid::new("settings").num_columns(2).show(ui, |ui| {
            ui.label("Detector confidence");
            ui.add(egui::Slider::new(&mut s.detect_conf, 0.05..=0.95));
            ui.end_row();
            ui.label("Reject below stars");
            ui.add(egui::Slider::new(&mut s.auto_reject_below_stars, 0..=5));
            ui.end_row();
            ui.label("Cull blurred frames");
            ui.checkbox(&mut s.cull_blurred, "");
            ui.end_row();
            ui.label("Include cars / bikes / trucks");
            ui.horizontal(|ui| {
                ui.checkbox(&mut s.include_cars, "cars");
                ui.checkbox(&mut s.include_bikes, "bikes");
                ui.checkbox(&mut s.include_trucks, "trucks");
            });
            ui.end_row();
        });
        if ui.button("Save").clicked() {
            self.settings.scan_profile = self.profile.name().into();
            let task = self.hub.start("Saving settings", 0);
            match self.settings.save(&Settings::path()) {
                Ok(()) => task.finish(),
                Err(e) => task.fail(e.to_string()),
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("bar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Conrod");
                ui.separator();
                ui.selectable_value(&mut self.screen, Screen::Scan, "Scan");
                ui.selectable_value(&mut self.screen, Screen::Review, "Review");
                ui.selectable_value(&mut self.screen, Screen::Settings, "Settings");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    self.status(ui);
                });
            });
        });
        egui::CentralPanel::default().show(ui, |ui| match self.screen {
            Screen::Scan => self.scan_screen(ui),
            Screen::Review => self.review_screen(ui),
            Screen::Settings => self.settings_screen(ui),
        });
    }
}
