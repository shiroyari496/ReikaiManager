mod data;
mod loader;
mod rules;
mod terminal;

use eframe::egui;
use std::sync::{Arc, Mutex};
// use std::thread;
use std::collections::HashMap;
use std::time::Instant;

use crate::data::{Player, Question, PlayerStatus, QuestionStatus, Event, SharedQuizState, PlayerId};
use crate::loader::{load_players, load_questions, write_log_head, write_log_line};
use crate::rules::{FreeBatting, QuizRule};
use crate::terminal::{read_line, show_prompt, display_players, display_question, display_scores, 
                     handle_set_command, handle_answer_command};

fn main() -> eframe::Result<()> {
    // データ読み込み（エラーハンドリング付き）
    let players = load_players("data/players.csv")
        .expect("Failed to load players from data/players.csv");
    let questions = load_questions("data/questions.csv")
        .expect("Failed to load questions from data/questions.csv");

    // 共有状態の作成
    let shared_state = Arc::new(Mutex::new(SharedQuizState::new(players.clone())));

    // // 【サブスレッド】ターミナル操作ロジック
    // let state_for_thread = Arc::clone(&shared_state);
    // thread::spawn(move || {
    //     if let Err(e) = run_terminal_loop(state_for_thread, players, questions) {
    //         eprintln!("Error in terminal loop: {}", e);
    //     }
    // });

    // 【メインスレッド】GUI起動
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1000.0, 600.0]),
        ..Default::default()
    };

    let shared_state_for_gui = Arc::clone(&shared_state); // GUI用にクローン

    eframe::run_native(
        "Quiz Scoreboard Display",
        native_options,
        Box::new(move |cc| {
            // ScoreboardApp::new に cc と shared_state を渡す
            Box::new(ScoreboardApp::new(cc, shared_state_for_gui))
        }),
    )
}

fn run_terminal_loop(
    state: Arc<Mutex<SharedQuizState>>,
    players: Vec<Player>,
    questions: Vec<Question>,
) -> Result<(), Box<dyn std::error::Error>> {
    let rule = FreeBatting;
    let question_num = questions.len();

    let mut player_statuses: HashMap<PlayerId, PlayerStatus> = HashMap::new();

    println!("Players:");
    for p in &players {
        println!(
            "{}:\t{}\t({}\t{})",
            p.id, p.name, p.affiliation.as_deref().unwrap_or("-"), p.grade.as_deref().unwrap_or("-")
        );
    }
    write_log_head("data/log.csv", &players)?;

    for q in 1..=question_num {
        // GUI側に現在の問題番号を通知
        {
            state.lock().unwrap().current_question = q as u32;
        }

        // 活動中のプレイヤーをフィルタリング
        let active_players: Vec<&Player> = players
            .iter()
            .filter(|p| {
                let status = player_statuses.get(&p.id);
                !status.map(|s| s.is_winner).unwrap_or(false)
                    && !status.map(|s| s.is_eliminated).unwrap_or(false)
                    && !status
                        .map(|s| s.frozen_until)
                        .flatten()
                        .map_or(false, |f| f > q as u32)
            })
            .collect();

        if active_players.is_empty() {
            println!("No active players remaining. Ending quiz.");
            break;
        }

        let mut question_status = QuestionStatus::new();
        let mut player_events: HashMap<PlayerId, Vec<Event>> = HashMap::new();

        display_question(q, &questions[q - 1].text);

        loop {
            show_prompt();
            let input = read_line();

            if input == "next" {
                break;
            }

            if input == "pass" {
                question_status.finished = true;
            }

            if question_status.finished {
                println!("Question finished");
                continue;
            }

            let parts: Vec<&str> = input.split_whitespace().collect();

            // `set` コマンド処理
            if parts.len() == 3 && parts[0] == "set" {
                match handle_set_command(&parts, &players, &mut player_statuses, &mut player_events) {
                    Ok(()) => {}
                    Err(e) => {
                        println!("Error: {}", e);
                        display_players(&players);
                    }
                }
                continue;
            }

            // buzz/correct/wrong コマンド処理
            if parts.len() == 2 && matches!(parts[0], "buzz" | "correct" | "wrong") {
                match handle_answer_command(
                    &parts,
                    &players,
                    &mut player_statuses,
                    &mut player_events,
                    &mut question_status,
                ) {
                    Ok(()) => {}
                    Err(e) => {
                        println!("Error: {}", e);
                        display_players(&players);
                    }
                }
                continue;
            }

            // コマンド不正
            println!("format: buzz <id> / correct <id> / wrong <id> / set <id> <score> / pass / next");
        }

        // ルールを適用してスコアを更新
        rule.apply(&mut player_statuses, &mut player_events, &mut question_status);

        display_scores(&players, &player_statuses);

        // GUI側の状態も更新
        {
            let mut data = state.lock().unwrap();
            for (player_id, status) in &player_statuses {
                if let Some(shared_status) = data.display_statuses.get_mut(player_id) {
                    *shared_status = status.clone();
                }
            }
        }

        write_log_line("data/log.csv", q, &players, &player_events)?;
    }

    Ok(())
}

fn setup_custom_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        "my_font".to_owned(),
        egui::FontData::from_static(include_bytes!(
            "../assets/fonts/Noto_Sans_JP/static/NotoSansJP-Regular.ttf"
        )),
    );

    fonts
        .families
        .get_mut(&egui::FontFamily::Proportional)
        .unwrap()
        .insert(0, "my_font".to_owned());
    fonts
        .families
        .get_mut(&egui::FontFamily::Monospace)
        .unwrap()
        .insert(0, "my_font".to_owned());

    ctx.set_fonts(fonts);
}

// --- GUI アプリケーション構造体 ---
struct ScoreboardApp {
    state: Arc<Mutex<SharedQuizState>>,
    is_3d_mode: bool,
    // 前回のスコアを保持して変更を検知する
    last_scores: HashMap<PlayerId, i32>,
    // 変更があった時刻を保持（アニメーション用）
    last_change_times: HashMap<PlayerId, Instant>,
}

impl ScoreboardApp {
    fn new(cc: &eframe::CreationContext<'_>, state: Arc<Mutex<SharedQuizState>>) -> Self {
        setup_custom_fonts(&cc.egui_ctx);
        Self {
            state,
            is_3d_mode: false,
            last_scores: HashMap::new(),
            last_change_times: HashMap::new(),
        }
    }

    /// 汎用的な3Dカード描画コンポーネント
    fn ui_3d_card(
        &self, 
        ui: &mut egui::Ui, 
        text: &str, 
        size: egui::Vec2, 
        font_size: f32,
        change_time: Option<std::time::Instant>
    ) {
        // --- アニメーション計算 ---
        let t = change_time.map_or(0.0, |inst| {
            let elapsed = inst.elapsed().as_secs_f32();
            let duration = 0.6; // 回転スピード
            if elapsed < duration {
                (1.0 - (elapsed / duration * std::f32::consts::PI).cos()) / 2.0
            } else { 0.0 }
        });

        let angle = t * std::f32::consts::PI;
        let cos_a = angle.cos();
        let sin_a = angle.sin();

        // 描画領域の確保
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let painter = ui.painter();
        let center = rect.center();
        let hw = size.x / 2.0 * cos_a.abs(); // 回転による幅の圧縮
        let hh = size.y / 2.0;
        let thickness = 6.0 * sin_a.abs(); // 回転中だけ厚みが見える演出

        // --- 頂点定義 ---
        let top = center.y - hh;
        let bottom = center.y + hh;
        let left = center.x - hw;
        let right = center.x + hw;
        let off = if cos_a > 0.0 { thickness } else { -thickness };

        // 側面（厚み）
        painter.add(egui::Shape::convex_polygon(
            vec![
                egui::pos2(right, top),
                egui::pos2(right + off, top),
                egui::pos2(right + off, bottom),
                egui::pos2(right, bottom),
            ],
            egui::Color32::from_rgb(40, 40, 50),
            egui::Stroke::NONE,
        ));

        // 表面
        painter.add(egui::Shape::convex_polygon(
            vec![
                egui::pos2(left, top),
                egui::pos2(right, top),
                egui::pos2(right, bottom),
                egui::pos2(left, bottom),
            ],
            egui::Color32::from_rgb(60, 60, 70),
            egui::Stroke::new(1.0, egui::Color32::GRAY),
        ));

        // 文字（反転中は表示しない）
        if cos_a.abs() > 0.4 {
            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::proportional(font_size * cos_a.abs()),
                egui::Color32::WHITE,
            );
        }
    }

    fn render_classic_grid(&mut self, ui: &mut egui::Ui, players: &[Player], statuses: &HashMap<PlayerId, PlayerStatus>) {
        ui.heading(egui::RichText::new(format!("Question #{}", self.state.lock().unwrap().current_question)).size(40.0));
        ui.add_space(20.0);

        egui::ScrollArea::horizontal().show(ui, |ui| {
            egui::Grid::new("score_grid")
                .striped(true)
                .spacing([30.0, 20.0])
                .show(ui, |ui| {
                    let header_size = 24.0;
                    let body_size = 30.0;
                    let score_size = 60.0;

                    // --- Name 行 ---
                    ui.label(egui::RichText::new("Name").size(header_size));
                    for p in players {
                        ui.label(egui::RichText::new(&p.name).size(body_size).strong());
                    }
                    ui.end_row();

                    ui.label("Affiliation");
                    for p in players {
                        ui.label(p.affiliation.as_deref().unwrap_or("-"));
                    }
                    ui.end_row();

                    ui.label("Grade");
                    for p in players {
                        ui.label(p.grade.as_deref().unwrap_or("-"));
                    }
                    ui.end_row();

                    ui.end_row();
                    ui.separator();
                    for _ in players {
                        ui.separator();
                    }
                    ui.end_row();

                    // --- SCORE 行 ---
                    ui.label(
                        egui::RichText::new("SCORE")
                            .strong()
                            .size(header_size)
                            .color(egui::Color32::LIGHT_BLUE),
                    );
                    for p in players {
                        let s = &statuses[&p.id];
                        ui.label(egui::RichText::new(s.score.to_string()).size(score_size).strong());
                    }
                    ui.end_row();

                    ui.label("Correct (○)");
                    for p in players {
                        ui.label(
                            egui::RichText::new(statuses[&p.id].correct_count.to_string())
                                .color(egui::Color32::GREEN),
                        );
                    }
                    ui.end_row();

                    ui.label("Wrong (×)");
                    for p in players {
                        ui.label(
                            egui::RichText::new(statuses[&p.id].wrong_count.to_string())
                                .color(egui::Color32::RED),
                        );
                    }
                    ui.end_row();
                });
        });
    }
}

impl eframe::App for ScoreboardApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1. 共有データの取得
        let (current_question, players, display_statuses) = {
            let mut data = self.state.lock().unwrap();
            (data.current_question, data.players.clone(), data.display_statuses.clone())
        };

        // 2. スコア変更の検知とアニメーション更新
        for p in &players {
            let current_score = display_statuses[&p.id].score;
            let last_score = self.last_scores.entry(p.id).or_insert(current_score);
            if *last_score != current_score {
                self.last_change_times.insert(p.id, std::time::Instant::now());
                *last_score = current_score;
            }
        }

        // 3. メインUIの描画
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading(format!("Question #{}", current_question));
                ui.separator();
                ui.checkbox(&mut self.is_3d_mode, "3D Mode");
            });

            ui.add_space(20.0);

            if self.is_3d_mode {
                egui::Grid::new("3d_grid").spacing([20.0, 20.0]).show(ui, |ui| {
                    ui.label("Name");
                    for p in &players {
                        self.ui_3d_card(ui, &p.name, egui::vec2(120.0, 40.0), 20.0, None);
                    }
                    ui.end_row();

                    ui.label("Score");
                    for p in &players {
                        let score_str = display_statuses[&p.id].score.to_string();
                        let change = self.last_change_times.get(&p.id).cloned();
                        self.ui_3d_card(ui, &score_str, egui::vec2(80.0, 60.0), 30.0, change);
                    }
                    ui.end_row();
                });
            } else {
                self.render_classic_grid(ui, &players, &display_statuses);
            }
        });

        // 4. コントロールパネル（別ウィンドウ）
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("control_panel"),
            egui::ViewportBuilder::default()
                .with_title("Quiz Control Panel")
                .with_inner_size([500.0, 600.0]),
            |ctx, class| {
                assert!(
                    class == egui::ViewportClass::Immediate,
                    "This platform doesn't support secondary viewports"
                );

                egui::CentralPanel::default().show(ctx, |ui| {
                    let mut data = self.state.lock().unwrap();
                    ui.heading("Controller");
                    ui.separator();

                    // 問題進行
                    ui.horizontal(|ui| {
                        if ui.button("Next Question").clicked() {
                            data.display_statuses = data.working_statuses.clone();
                            data.current_question += 1;
                        }
                    });

                    ui.separator();

                    // 簡易回答操作
                    let player_info: Vec<(PlayerId, String)> = data.players.iter()
                        .map(|p| (p.id, p.name.clone()))
                        .collect();

                    for (pid, name) in &player_info {
                        ui.horizontal(|ui| {
                            ui.label(format!("{}: {}", pid, name));
                            if ui.button(egui::RichText::new("Correct").color(egui::Color32::GREEN)).clicked() {
                                if let Some(status) = data.working_statuses.get_mut(pid) {
                                    status.score += 1;
                                    status.correct_count += 1;
                                }
                            }
                            if ui.button(egui::RichText::new("Wrong").color(egui::Color32::RED)).clicked() {
                                if let Some(status) = data.working_statuses.get_mut(pid) {
                                    status.wrong_count += 1;
                                }
                            }
                        });
                    }

                    ui.separator();
                    ui.label("Player Status Edit (Drag to adjust)");
                    egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                        egui::Grid::new("edit_grid").striped(true).show(ui, |ui| {
                            for (pid, name) in &player_info {
                                let s = data.working_statuses.get_mut(pid).unwrap();
                                ui.label(name);
                                ui.add(egui::DragValue::new(&mut s.score).prefix("Pt:"));
                                ui.add(egui::DragValue::new(&mut s.correct_count).prefix("○:"));
                                ui.add(egui::DragValue::new(&mut s.wrong_count).prefix("×:"));
                                ui.end_row();
                            }
                        });
                    });
                });
            },
        );

        ctx.request_repaint();
    }
}
