mod data;
mod loader;
mod rules;
mod terminal;

use eframe::egui;
use std::sync::{Arc, Mutex};
use std::thread;
use std::collections::HashMap;

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
    eframe::run_native(
        "Quiz Scoreboard Display",
        native_options,
        Box::new(|cc| {
            setup_custom_fonts(&cc.egui_ctx);
            Box::new(ScoreboardApp { state: shared_state })
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
}

impl eframe::App for ScoreboardApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut data = self.state.lock().unwrap();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(egui::RichText::new(format!("Question #{}", data.current_question)).size(40.0));
            ui.add_space(20.0);

            egui::ScrollArea::horizontal().show(ui, |ui| {
                egui::Grid::new("score_grid")
                    .striped(true)
                    .spacing([30.0, 20.0])
                    .show(ui, |ui| {
                        let header_size = 24.0;
                        let body_size = 30.0;
                        let score_size = 60.0;

                        // // --- ID 行 ---
                        // ui.label(egui::RichText::new("ID").size(header_size / 2.0));
                        // for p in &data.players {
                        //     ui.label(egui::RichText::new(p.id.to_string()).size(header_size));
                        // }
                        // ui.end_row();

                        // --- Name 行 ---
                        ui.label(egui::RichText::new("Name").size(header_size));
                        for p in &data.players {
                            ui.label(egui::RichText::new(&p.name).size(body_size).strong());
                        }
                        ui.end_row();

                        ui.label("Affiliation");
                        for p in &data.players {
                            ui.label(p.affiliation.as_deref().unwrap_or("-"));
                        }
                        ui.end_row();

                        ui.label("Grade");
                        for p in &data.players {
                            ui.label(p.grade.as_deref().unwrap_or("-"));
                        }
                        ui.end_row();

                        ui.end_row();
                        ui.separator();
                        for _ in &data.players {
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
                        for p in &data.players {
                            let s = &data.display_statuses[&p.id];
                            ui.label(egui::RichText::new(s.score.to_string()).size(score_size).strong());
                        }
                        ui.end_row();

                        ui.label("Correct (○)");
                        for p in &data.players {
                            ui.label(
                                egui::RichText::new(data.display_statuses[&p.id].correct_count.to_string())
                                    .color(egui::Color32::GREEN),
                            );
                        }
                        ui.end_row();

                        ui.label("Wrong (×)");
                        for p in &data.players {
                            ui.label(
                                egui::RichText::new(data.display_statuses[&p.id].wrong_count.to_string())
                                    .color(egui::Color32::RED),
                            );
                        }
                        ui.end_row();
                    });
            });
        });

        // --- コントロール ---
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
                    ui.heading("Controller");
                    ui.separator();

                    // 1. 問題進行
                    ui.horizontal(|ui| {
                        if ui.button("Next Question").clicked() {
                            // 1. 編集中の状態を表示用にコピー
                            data.display_statuses = data.working_statuses.clone();
                            // 2. 問題を進める
                            data.current_question += 1;
                            // data.buzz_queue.clear();
                        }
                    });

                    ui.separator();

                    // 2. 回答権キュー（押し順）
                    // ui.label(egui::RichText::new("Buzz Queue (Priority)").strong());
                    // if data.buzz_queue.is_empty() {
                    //     ui.label("No one buzzed yet.");
                    // } else
                    {
                        let player_list: Vec<(PlayerId, String)> = data.players.iter()
                            .map(|p| (p.id, p.name.clone()))
                            .collect();
                        // let mut to_remove = None;
                        // for (i, &pid) in data.buzz_queue.iter().enumerate()
                        for (pid, name) in player_list {
                            ui.horizontal(|ui| {
                                ui.label(format!("{}: Player {}", pid, name));
                                // トグル式にしたい
                                if ui.button(egui::RichText::new("Correct").color(egui::Color32::GREEN)).clicked() {
                                    if let Some(status) = data.working_statuses.get_mut(&pid) {
                                        status.score += 1;
                                        status.correct_count += 1;
                                    }
                                }
                                // トグル式にしたい
                                if ui.button(egui::RichText::new("Wrong").color(egui::Color32::RED)).clicked() {
                                    if let Some(status) = data.working_statuses.get_mut(&pid) {
                                        // 必要なら減点処理などを追加
                                        status.wrong_count += 1;
                                    }
                                }
                                // if ui.button("Cancel").clicked() { to_remove = Some(pid); }
                            });
                        }
                        // if let Some(idx) = to_remove { data.buzz_queue.remove(idx); }
                    }

                    ui.separator();

                    // 3. プレイヤーごとの詳細修正・プレビュー
                    ui.label("Player Status Edit");
                    egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                        egui::Grid::new("edit_grid").striped(true).show(ui, |ui| {
                            let player_info: Vec<(PlayerId, String)> = data.players.iter()
                                .map(|p| (p.id, p.name.clone()))
                                .collect();
                            for (pid, name) in &player_info {
                                let mut s = data.working_statuses.get_mut(&pid).unwrap();
                                ui.label(name);
                                
                                // スコアや回数の直接修正
                                ui.add(egui::DragValue::new(&mut s.score).prefix("Pt:"));
                                ui.add(egui::DragValue::new(&mut s.correct_count).prefix("○:"));
                                ui.add(egui::DragValue::new(&mut s.wrong_count).prefix("×:"));
                                
                                // 手動でのBuzz登録
                                // if ui.button("Buzz").clicked() {
                                //     // if !data.buzz_queue.contains(&p.id) {
                                //     //     data.buzz_queue.push(p.id);
                                //     // }
                                // }
                                ui.end_row();
                            }
                        });
                    });

                    ui.separator();

                    // 4. 履歴プレビュー
                    ui.collapsing("Action Logs", |ui| {
                        // for log in data.logs.iter().rev().take(10) {
                        //     ui.small(log);
                        // }
                    });
                });
            },
        );

        ctx.request_repaint();
    }
}
