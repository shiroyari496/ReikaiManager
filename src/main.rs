mod data;
mod loader;
mod rules;
mod terminal;

use eframe::egui;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::time::Instant;

use crate::data::{Player, Question, PlayerStatus, QuestionStatus, Event, SharedQuizState, PlayerId, RuleOption};
use crate::loader::{load_players, load_questions, write_log_head, write_log_line, write_next_round_players};
use crate::terminal::{read_line, show_prompt, display_players, display_question, display_scores, 
                     handle_set_command, handle_answer_command};
use crate::rules::apply_selected_rule;

fn main() -> eframe::Result<()> {
    // GUI起動前に設定画面でパス・ルールを選択してから読み込む
    let shared_state = Arc::new(Mutex::new(SharedQuizState::empty()));

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1000.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Quiz Scoreboard Display",
        native_options,
        Box::new(move |cc| {
            // ScoreboardApp::new に cc と shared_state を渡す
            Box::new(ScoreboardApp::new(cc, shared_state))
        }),
    )
}

// ターミナルでの操作
#[allow(dead_code)]
fn run_terminal_loop(
    state: Arc<Mutex<SharedQuizState>>,
    players: Vec<Player>,
    questions: Vec<Question>,
) -> Result<(), Box<dyn std::error::Error>> {
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

        // Freeze カウントを進める (質問が進むごとに-1)
        for status in player_statuses.values_mut() {
            if status.freeze_count > 0 {
                status.freeze_count -= 1;
            }
        }

        // 活動中のプレイヤーをフィルタリング
        let active_players: Vec<&Player> = players
            .iter()
            .filter(|p| {
                let status = player_statuses.get(&p.id);
                !status.map(|s| s.is_winner).unwrap_or(false)
                    && !status.map(|s| s.is_eliminated).unwrap_or(false)
                    && status.map(|s| s.freeze_count).unwrap_or(0) == 0
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
        {
            let shared_state = state.lock().unwrap();
            let rule_option = shared_state.rule_option;
            let n = shared_state.n_correct;
            let m = shared_state.m_wrong;
            drop(shared_state);  // ロックを明示的に解放

            apply_selected_rule(&rule_option, n, m, &mut player_statuses, &mut player_events, &mut question_status, q as u32);
        }

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
struct AppConfig {
    round_name: String,
    players_csv: String,
    questions_csv: String,
    log_csv: String,
    next_round_player_csv: String,
    next_round_advance_count: u32,
    rule_option: RuleOption,
    n_correct: u32,
    m_wrong: u32,
    initial_player_states: HashMap<PlayerId, (i32, u32, u32)>, // (score, correct, wrong)
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            round_name: "Free Batting".into(),
            players_csv: "data/players.csv".into(),
            questions_csv: "data/questions.csv".into(),
            log_csv: "data/log.csv".into(),
            next_round_player_csv: "data/next_round_players.csv".into(),
            next_round_advance_count: 5,
            rule_option: RuleOption::default(),
            n_correct: 7,
            m_wrong: 3,
            initial_player_states: HashMap::new(),
        }
    }
}

struct ScoreboardApp {
    state: Arc<Mutex<SharedQuizState>>,
    is_config_mode: bool,
    show_player_setup: bool,
    config: AppConfig,
    loaded_players: Vec<Player>,
    config_message: String,
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
            is_config_mode: true,
            show_player_setup: false,
            config: AppConfig::default(),
            loaded_players: Vec::new(),
            config_message: "設定を入力して [Load Data] を押してください。".into(),
            last_scores: HashMap::new(),
            last_change_times: HashMap::new(),
        }
    }

    fn apply_config_to_state(&mut self, players: Vec<Player>, questions: Vec<Question>) {
        let mut display_statuses = HashMap::new();
        for p in &players {
            let mut status = PlayerStatus::new();
            // 初期値設定を反映
            if let Some((score, correct, wrong)) = self.config.initial_player_states.get(&p.id) {
                status.score = *score;
                status.correct_count = *correct;
                status.wrong_count = *wrong;
            }
            display_statuses.insert(p.id, status);
        }

        let mut data = self.state.lock().unwrap();
        data.players = players;
        data.questions = questions;
        data.display_statuses = display_statuses.clone();
        data.working_statuses = display_statuses;
        data.player_events = HashMap::new();
        data.question_status = QuestionStatus::new();
        data.current_question = 0;
        data.rule_option = self.config.rule_option;
        data.n_correct = self.config.n_correct;
        data.m_wrong = self.config.m_wrong;
        data.next_finish_rank = 1;
        data.round_completed = false;
    }

    fn apply_pending_events(&self, data: &mut SharedQuizState) {
        data.working_statuses = data.display_statuses.clone();

        apply_selected_rule(
            &data.rule_option,
            data.n_correct,
            data.m_wrong,
            &mut data.working_statuses,
            &mut data.player_events,
            &mut data.question_status,
            data.current_question,
        );

        // 抜け順位を更新（勝利 or 脱落が初めて付与されたとき）
        for status in data.working_statuses.values_mut() {
            if status.finish_rank.is_none() && status.is_winner {
                status.finish_rank = Some(data.next_finish_rank);
                data.next_finish_rank += 1;
            }
        }
    }

    fn export_next_round_players(&mut self, data: &mut SharedQuizState) {
        if data.round_completed {
            return;
        }

        data.round_completed = true;

        match write_next_round_players(
            &self.config.next_round_player_csv,
            &data.players,
            &data.display_statuses,
            self.config.next_round_advance_count as usize,
            data.rule_option,
        ) {
            Ok(_) => {
                self.config_message = format!(
                    "終了: {}人を {} に書き出しました",
                    self.config.next_round_advance_count,
                    self.config.next_round_player_csv
                );
            }
            Err(e) => {
                self.config_message = format!("CSV出力失敗: {}", e);
            }
        }
    }

    fn render_config_ui(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.show_player_setup {
                ui.heading("Player Initial Setup");
                ui.label("各プレイヤーの初期得点・正答数・誤答数を設定します（オプション）");
                ui.separator();

                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::Grid::new("player_setup_grid").striped(true).show(ui, |ui| {
                        ui.label("Player");
                        ui.label("Score");
                        ui.label("Correct");
                        ui.label("Wrong");
                        ui.end_row();

                        for p in &self.loaded_players {
                            let entry = self.config.initial_player_states.entry(p.id).or_insert((0, 0, 0));
                            ui.label(&p.name);
                            ui.add(egui::Slider::new(&mut entry.0, -50..=200).text(""));
                            ui.add(egui::Slider::new(&mut entry.1, 0..=100).text(""));
                            ui.add(egui::Slider::new(&mut entry.2, 0..=100).text(""));
                            ui.end_row();
                        }
                    });
                });

                ui.separator();

                ui.horizontal(|ui| {
                    if ui.button("Back to Config").clicked() {
                        self.show_player_setup = false;
                    }
                    if ui.button("Begin Quiz").clicked() {
                        let players = self.loaded_players.clone();
                        let questions = match load_questions(&self.config.questions_csv) {
                            Ok(q) => q,
                            Err(e) => {
                                self.config_message = format!("問題読み込みエラー: {}", e);
                                return;
                            }
                        };
                        if let Err(e) = write_log_head(&self.config.log_csv, &players) {
                            self.config_message = format!("CSV読み込み成功、ただしログヘッダー書き込み失敗: {}", e);
                        } else {
                            self.config_message = "クイズ開始".into();
                        }
                        self.apply_config_to_state(players, questions);
                        self.is_config_mode = false;
                    }
                });
            } else {
                ui.heading("Quiz Startup Configuration");

                ui.separator();

                ui.horizontal(|ui| {
                    ui.label("Round name");
                    ui.text_edit_singleline(&mut self.config.round_name);
                });

                ui.separator();

                ui.horizontal(|ui| {
                    ui.label("Players CSV");
                    ui.text_edit_singleline(&mut self.config.players_csv);
                });
                ui.horizontal(|ui| {
                    ui.label("Questions CSV");
                    ui.text_edit_singleline(&mut self.config.questions_csv);
                });
                ui.horizontal(|ui| {
                    ui.label("Log CSV");
                    ui.text_edit_singleline(&mut self.config.log_csv);
                });
                ui.horizontal(|ui| {
                    ui.label("Next Round Players CSV");
                    ui.text_edit_singleline(&mut self.config.next_round_player_csv);
                });
                ui.horizontal(|ui| {
                    ui.label("Advance Count");
                    ui.add(egui::DragValue::new(&mut self.config.next_round_advance_count).clamp_range(1..=100));
                });

                ui.separator();

                ui.label("Rule option");
                ui.horizontal( |ui| {
                    for &rule in RuleOption::all_options() {
                        if ui
                            .selectable_label(self.config.rule_option == rule, rule.label())
                            .clicked()
                        {
                            self.config.rule_option = rule;
                        }
                    }
                });

                if
                    self.config.rule_option == RuleOption::NCorrectMWrong ||
                    self.config.rule_option == RuleOption::UpDown ||
                    self.config.rule_option == RuleOption::NByM ||
                    self.config.rule_option == RuleOption::RenDatsuNCorrectMWrong
                {
                    ui.horizontal(|ui| {
                        ui.label("N Correct");
                        ui.add(egui::Slider::new(&mut self.config.n_correct, 1..=20).text(""));
                    });
                    ui.horizontal(|ui| {
                        ui.label("M Wrong");
                        ui.add(egui::Slider::new(&mut self.config.m_wrong, 1..=20).text(""));
                    });
                }
                if self.config.rule_option == RuleOption::Freeze {
                    ui.horizontal(|ui| {
                        ui.label("N Correct");
                        ui.add(egui::Slider::new(&mut self.config.n_correct, 1..=20).text(""));
                    });
                }

                ui.separator();

                if ui.button("Load Data").clicked() {
                    match load_players(&self.config.players_csv) {
                        Ok(p) => {
                            self.loaded_players = p;
                            self.config.initial_player_states.clear();
                            self.show_player_setup = true;
                            self.config_message = "プレイヤー読み込み成功。初期値を設定してください。".into();
                        }
                        Err(e) => {
                            self.config_message = format!("プレイヤー読み込みエラー: {}", e);
                        }
                    }
                }

                if ui.button("Load Data and Begin (No Setup)").clicked() {
                    let players = match load_players(&self.config.players_csv) {
                        Ok(p) => p,
                        Err(e) => {
                            self.config_message = format!("プレイヤー読み込みエラー: {}", e);
                            return;
                        }
                    };
                    let questions = match load_questions(&self.config.questions_csv) {
                        Ok(q) => q,
                        Err(e) => {
                            self.config_message = format!("問題読み込みエラー: {}", e);
                            return;
                        }
                    };
                    if let Err(e) = write_log_head(&self.config.log_csv, &players) {
                        self.config_message = format!("CSV読み込み成功、ただしログヘッダー書き込み失敗: {}", e);
                    } else {
                        self.config_message = "CSV読み込み成功。ラウンドを開始します。".into();
                    }
                    self.apply_config_to_state(players, questions);
                    self.is_config_mode = false;
                }

                if ui.button("Reset to defaults").clicked() {
                    self.config = AppConfig::default();
                    self.config_message = "デフォルト設定にリセットしました。".into();
                }

                ui.separator();

                ui.label(egui::RichText::new(&self.config_message));
            }
        });
    }

    /// 厚みと縁取りのある3Dカード描画
    fn ui_3d_card(
        &self,
        ui: &mut egui::Ui,
        text: &str,
        font_size: f32,
        size: egui::Vec2,
        color: egui::Color32,
        marker_size: f32,
        marker_color: egui::Color32,
        change_time: Option<std::time::Instant>,
    ) {
        let t = change_time.map_or(0.0, |inst| {
            let elapsed = inst.elapsed().as_secs_f32();
            // 0.8秒で180度回転する
            (elapsed.min(0.8) / 0.8 * std::f32::consts::FRAC_PI_2).sin()
        });

        let angle = t * std::f32::consts::PI;
        let cos_a = angle.cos();
        let sin_a = angle.sin();

        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let painter = ui.painter();
        let center = rect.center();

        // パースの強さ（0.0～0.3程度が自然）
        let perspective_factor = 0.2 * sin_a.abs();
 
        let width_scale = cos_a.abs().max(0.01);
        let hw = size.x / 2.0 * width_scale;
        let hh = size.y / 2.0;

        // 左右のパース計算
        let side_sign = if cos_a >= 0.0 { 1.0 } else { -1.0 };
        let left_h_scale  = 1.0 + (perspective_factor * if cos_a >= 0.0 { -1.0 } else { 1.0 });
        let right_h_scale = 1.0 + (perspective_factor * if cos_a >= 0.0 { 1.0 } else { -1.0 });

        let p1 = egui::pos2(center.x - hw, center.y - hh * left_h_scale); // 左上
        let p2 = egui::pos2(center.x + hw, center.y - hh * right_h_scale); // 右上
        let p3 = egui::pos2(center.x + hw, center.y + hh * right_h_scale); // 右下
        let p4 = egui::pos2(center.x - hw, center.y + hh * left_h_scale); // 左下

        // --- 1. 厚みの描画 ---
        if cos_a.abs() < 0.98 {
            let thickness_val = 10.0 * sin_a.abs() * side_sign;
            let side_color = color.linear_multiply(0.4); // 側面は暗く

            // カードの端(p2-p3 または p1-p4)から厚み分ずらしたポリゴンを描画
            let (top, bot) = if cos_a >= 0.0 { (p2, p3) } else { (p1, p4) };
            painter.add(egui::Shape::convex_polygon(
                vec![
                    top,
                    egui::pos2(top.x + thickness_val, top.y),
                    egui::pos2(bot.x + thickness_val, bot.y),
                    bot,
                ],
                side_color,
                egui::Stroke::NONE,
            ));
        }

        // --- 2. メインの板の描画 ---
        let light_factor = 0.7 + 0.3 * cos_a.abs();
        let current_color = color.linear_multiply(light_factor);

        painter.add(egui::Shape::convex_polygon(
            vec![p1, p2, p3, p4],
            current_color,
            egui::Stroke::new(1.0, egui::Color32::WHITE),
        ));

        // --- 3. 四隅マーカーの描画 ---
        let marker_horiz = (marker_size / 2.0) * cos_a.abs().max(0.05); // 角度に合わせて横方向に圧縮
        let marker_vert = marker_size / 2.0;

        for &c in &[p1, p2, p3, p4] {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(c.x - marker_horiz, c.y - marker_vert),
                    egui::pos2(c.x + marker_horiz, c.y - marker_vert),
                    egui::pos2(c.x + marker_horiz, c.y + marker_vert),
                    egui::pos2(c.x - marker_horiz, c.y + marker_vert),
                ],
                marker_color,
                egui::Stroke::new(1.0, egui::Color32::WHITE),
            ));
        }

        // 4. テキスト描画
        if cos_a.abs() > 0.3 {
            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::proportional(font_size * width_scale),
                egui::Color32::WHITE,
            );
        }
    }

    /// 左側に名前と右側に所属・学年を配置した厚みと縁取りのある3Dカード描画
    fn ui_3d_player_info_card(
        &self,
        ui: &mut egui::Ui,
        name: &str,
        affiliation: Option<&str>,
        grade: Option<&str>,
        name_font_size: f32,
        small_font_size: f32,
        size: egui::Vec2,
        color: egui::Color32,
        marker_size: f32,
        marker_color: egui::Color32,
        change_time: Option<std::time::Instant>
    ) {
        let t = change_time.map_or(0.0, |inst| {
            let elapsed = inst.elapsed().as_secs_f32();
            // 0.8秒で180度回転する
            (elapsed.min(0.8) / 0.8 * std::f32::consts::FRAC_PI_2).sin()
        });

        let angle = t * std::f32::consts::PI;
        let cos_a = angle.cos();
        let sin_a = angle.sin();

        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let painter = ui.painter();
        let center = rect.center();

        // パースの強さ（0.0～0.3程度が自然）
        let perspective_factor = 0.2 * sin_a.abs();

        let width_scale = cos_a.abs().max(0.01);
        let hw = size.x / 2.0 * width_scale;
        let hh = size.y / 2.0;

        // 左右のパース計算
        let side_sign = if cos_a >= 0.0 { 1.0 } else { -1.0 };
        let left_h_scale  = 1.0 + (perspective_factor * if cos_a >= 0.0 { -1.0 } else { 1.0 });
        let right_h_scale = 1.0 + (perspective_factor * if cos_a >= 0.0 { 1.0 } else { -1.0 });

        let p1 = egui::pos2(center.x - hw, center.y - hh * left_h_scale); // 左上
        let p2 = egui::pos2(center.x + hw, center.y - hh * right_h_scale); // 右上
        let p3 = egui::pos2(center.x + hw, center.y + hh * right_h_scale); // 右下
        let p4 = egui::pos2(center.x - hw, center.y + hh * left_h_scale); // 左下

         // --- 1. 厚みの描画 ---
        if cos_a.abs() < 0.98 {
            let thickness_val = 10.0 * sin_a.abs() * side_sign;
            let side_color = color.linear_multiply(0.4); // 側面は暗く

            // カードの端(p2-p3 または p1-p4)から厚み分ずらしたポリゴンを描画
            let (top, bot) = if cos_a >= 0.0 { (p2, p3) } else { (p1, p4) };
            painter.add(egui::Shape::convex_polygon(
                vec![
                    top,
                    egui::pos2(top.x + thickness_val, top.y),
                    egui::pos2(bot.x + thickness_val, bot.y),
                    bot,
                ],
                side_color,
                egui::Stroke::NONE,
            ));
        }

        // --- 2. メインの板の描画 ---
        let light_factor = 0.7 + 0.3 * cos_a.abs();
        let current_color = color.linear_multiply(light_factor);

        painter.add(egui::Shape::convex_polygon(
            vec![p1, p2, p3, p4],
            current_color,
            egui::Stroke::new(1.0, egui::Color32::WHITE),
        ));

        // --- 3. 四隅マーカーの描画 ---
        let marker_horiz = (marker_size / 2.0) * cos_a.abs().max(0.05); // 角度に合わせて横方向に圧縮
        let marker_vert = marker_size / 2.0;

        for &c in &[p1, p2, p3, p4] {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(c.x - marker_horiz, c.y - marker_vert),
                    egui::pos2(c.x + marker_horiz, c.y - marker_vert),
                    egui::pos2(c.x + marker_horiz, c.y + marker_vert),
                    egui::pos2(c.x - marker_horiz, c.y + marker_vert),
                ],
                marker_color,
                egui::Stroke::new(1.0, egui::Color32::WHITE),
            ));
        }

        // 4. テキスト描画
        if cos_a.abs() > 0.3 {
            let visibility = cos_a.abs();
            let alpha = (visibility * 255.0) as u8;
            let text_color = egui::Color32::from_rgba_unmultiplied(255, 255, 255, alpha);

            let char_height_name = name_font_size * visibility;
            let char_height_small = small_font_size * visibility;

            // 左右のフォントサイズの合計に対する比率を計算
            let total_font_weight = name_font_size + small_font_size;
            let left_weight = name_font_size / total_font_weight;  // 名前側の占有率
            let right_weight = small_font_size / total_font_weight; // 情報側の占有率

            // カードの全幅 (2.0 * hw) を重みに基づいて分割
            // 各エリアの中央座標を計算する
            // 名前側 (左): 端から「名前エリアの半分」の位置
            // 情報側 (右): 端から「名前エリア + 情報エリアの半分」の位置
            let name_x_offset = (2.0 * hw) * (left_weight / 2.0);
            let info_x_offset = (2.0 * hw) * (left_weight + right_weight / 2.0);

            // center.x - hw (左端) を起点に座標を決定
            // side_sign で回転時の反転を考慮
            let left_edge = center.x - hw * side_sign;
            let name_x = left_edge + name_x_offset * side_sign;
            let info_x = left_edge + info_x_offset * side_sign;

            // 左側: 名前
            let name_chars: Vec<char> = name.chars().collect();
            let total_name_height = name_chars.len() as f32 * char_height_name;
            let mut y_name = center.y - total_name_height / 2.0;
            for ch in &name_chars {
                let pos = egui::pos2(name_x, y_name + char_height_name / 2.0);
                painter.text(
                    pos,
                    egui::Align2::CENTER_CENTER,
                    ch.to_string(),
                    egui::FontId::proportional(char_height_name),
                    text_color,
                );
                y_name += char_height_name;
            }

            // 右側: 所属・学年

            // カードの高さ hh に基づいて上下に振り分け
            let section_gap = 4.0 * visibility;

            // 右側上部: 所属
            if let Some(aff_str) = affiliation {
                let aff_chars: Vec<char> = aff_str.chars().collect();
                let total_aff_height = aff_chars.len() as f32 * char_height_small;
                let mut y_aff = center.y - section_gap - total_aff_height;
                for ch in &aff_chars {
                    let pos = egui::pos2(info_x, y_aff + char_height_small / 2.0);
                    painter.text(
                        pos,
                        egui::Align2::CENTER_CENTER,
                        ch.to_string(),
                        egui::FontId::proportional(char_height_small),
                        text_color,
                    );
                    y_aff += char_height_small;
                }
            }

            // 右側下部: 学年
            if let Some(grade_str) = grade {
                let grade_chars: Vec<char> = grade_str.chars().collect();
                // let total_grade_height = grade_chars.len() as f32 * char_height_small;
                let mut y_grade = center.y + hh * 0.1;
                for ch in &grade_chars {
                    let pos = egui::pos2(info_x, y_grade + char_height_name / 2.0);
                    painter.text(
                        pos,
                        egui::Align2::CENTER_CENTER,
                        ch.to_string(),
                        egui::FontId::proportional(char_height_small),
                        text_color,
                    );
                    y_grade += char_height_small;
                }
            }
        }
    }

    /// SpecialBy専用のプレイヤー情報カード描画（横書き）
    /// 枠内の上段左側が所属、上段右側が学年、下側が名前
    fn ui_3d_player_info_card_special_by(
        &self,
        ui: &mut egui::Ui,
        name: &str,
        affiliation: Option<&str>,
        grade: Option<&str>,
        name_font_size: f32,
        small_font_size: f32,
        size: egui::Vec2,
        color: egui::Color32,
        marker_size: f32,
        marker_color: egui::Color32,
        change_time: Option<std::time::Instant>
    ) {
        let t = change_time.map_or(0.0, |inst| {
            let elapsed = inst.elapsed().as_secs_f32();
            // 0.8秒で180度回転する
            (elapsed.min(0.8) / 0.8 * std::f32::consts::FRAC_PI_2).sin()
        });

        let angle = t * std::f32::consts::PI;
        let cos_a = angle.cos();
        let sin_a = angle.sin();

        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let painter = ui.painter();
        let center = rect.center();

        // パースの強さ（0.0～0.3程度が自然）
        let perspective_factor = 0.2 * sin_a.abs();

        let width_scale = cos_a.abs().max(0.01);
        let hw = size.x / 2.0 * width_scale;
        let hh = size.y / 2.0;

        // 左右のパース計算
        let side_sign = if cos_a >= 0.0 { 1.0 } else { -1.0 };
        let left_h_scale  = 1.0 + (perspective_factor * if cos_a >= 0.0 { -1.0 } else { 1.0 });
        let right_h_scale = 1.0 + (perspective_factor * if cos_a >= 0.0 { 1.0 } else { -1.0 });

        let p1 = egui::pos2(center.x - hw, center.y - hh * left_h_scale); // 左上
        let p2 = egui::pos2(center.x + hw, center.y - hh * right_h_scale); // 右上
        let p3 = egui::pos2(center.x + hw, center.y + hh * right_h_scale); // 右下
        let p4 = egui::pos2(center.x - hw, center.y + hh * left_h_scale); // 左下

         // --- 1. 厚みの描画 ---
        if cos_a.abs() < 0.98 {
            let thickness_val = 10.0 * sin_a.abs() * side_sign;
            let side_color = color.linear_multiply(0.4); // 側面は暗く

            // カードの端(p2-p3 または p1-p4)から厚み分ずらしたポリゴンを描画
            let (top, bot) = if cos_a >= 0.0 { (p2, p3) } else { (p1, p4) };
            painter.add(egui::Shape::convex_polygon(
                vec![
                    top,
                    egui::pos2(top.x + thickness_val, top.y),
                    egui::pos2(bot.x + thickness_val, bot.y),
                    bot,
                ],
                side_color,
                egui::Stroke::NONE,
            ));
        }

        // --- 2. メインの板の描画 ---
        let light_factor = 0.7 + 0.3 * cos_a.abs();
        let current_color = color.linear_multiply(light_factor);

        painter.add(egui::Shape::convex_polygon(
            vec![p1, p2, p3, p4],
            current_color,
            egui::Stroke::new(1.0, egui::Color32::WHITE),
        ));

        // --- 3. 四隅マーカーの描画 ---
        let marker_horiz = (marker_size / 2.0) * cos_a.abs().max(0.05); // 角度に合わせて横方向に圧縮
        let marker_vert = marker_size / 2.0;

        for &c in &[p1, p2, p3, p4] {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(c.x - marker_horiz, c.y - marker_vert),
                    egui::pos2(c.x + marker_horiz, c.y - marker_vert),
                    egui::pos2(c.x + marker_horiz, c.y + marker_vert),
                    egui::pos2(c.x - marker_horiz, c.y + marker_vert),
                ],
                marker_color,
                egui::Stroke::new(1.0, egui::Color32::WHITE),
            ));
        }

        // 4. テキスト描画（横書き）
        if cos_a.abs() > 0.3 {
            let visibility = cos_a.abs();
            let alpha = (visibility * 255.0) as u8;
            let text_color = egui::Color32::from_rgba_unmultiplied(255, 255, 255, alpha);

            let char_height_name = name_font_size * visibility;
            let char_height_small = small_font_size * visibility;

            // 上段: 所属（左）と学年（右）
            let top_y = center.y - hh * 0.3;
            let bottom_y = center.y + hh * 0.2;

            // 所属（上段左）
            if let Some(aff_str) = affiliation {
                let pos = egui::pos2(center.x - hw * 0.4, top_y);
                painter.text(
                    pos,
                    egui::Align2::CENTER_CENTER,
                    aff_str,
                    egui::FontId::proportional(char_height_small),
                    text_color,
                );
            }

            // 学年（上段右）
            if let Some(grade_str) = grade {
                let pos = egui::pos2(center.x + hw * 0.4, top_y);
                painter.text(
                    pos,
                    egui::Align2::CENTER_CENTER,
                    grade_str,
                    egui::FontId::proportional(char_height_small),
                    text_color,
                );
            }

            // 下段: 名前（中央）
            let pos = egui::pos2(center.x, bottom_y);
            painter.text(
                pos,
                egui::Align2::CENTER_CENTER,
                name,
                egui::FontId::proportional(char_height_name),
                text_color,
            );
        }
    }
}

fn wrap_text(text: &str, width: usize) -> String {
    text.chars()
        .enumerate()
        .flat_map(|(i, c)| {
            if i != 0 && i % width == 0 {
                vec!['\n', ' ', ' ', ' ', ' ', ' ', c]
            } else {
                vec![c]
            }
        })
        .collect()
}

impl eframe::App for ScoreboardApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // F11 キーで全画面の切り替え
        if ctx.input(|i| i.key_pressed(egui::Key::F11)) {
            let is_fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
            // メインウィンドウ（表示側）の表示モードを切り替え
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!is_fullscreen));
        }

        if self.is_config_mode {
            self.render_config_ui(ctx);
            ctx.request_repaint();
            return;
        }

        // 共有データの取得
        let (current_question, questions, players, display_statuses, rule_option) = {
            let data = self.state.lock().unwrap();
            (data.current_question as usize, data.questions.clone(), data.players.clone(), data.display_statuses.clone(), data.rule_option)
        };

        // メイン画面用の問題文 (indexが1以上なら「前回」を表示)
        let display_q_text = if current_question > 0 && current_question <= questions.len() {
            wrap_text(&questions[current_question - 1].text, 50)
        } else {
            "Waiting for Quiz to start...".to_string()
        };
        let display_a_text = if current_question > 0 && current_question <= questions.len() {
            &questions[current_question - 1].answer
        } else {
            "Waiting for Quiz to start..."
        };

        // スコア変更の検知とアニメーション更新
        for p in &players {
            let current_score = display_statuses[&p.id].score;
            let last_score = self.last_scores.entry(p.id).or_insert(current_score);
            if *last_score != current_score {
                self.last_change_times.insert(p.id, std::time::Instant::now());
                *last_score = current_score;
            }
        }

        // メインUIの描画
        // 背景色
        let bg_color = egui::Color32::from_rgb(20, 20, 30);

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(bg_color))
            .show(ctx, |ui| {
                ui.add_space(10.0);

                // --- ラウンド名 ---
                ui.label(egui::RichText::new(format!("      {}", &self.config.round_name)).color(egui::Color32::from_rgb(255, 255, 255)).strong().size(18.0));

                ui.add_space(10.0);

                // --- 問題文 ---
                ui.vertical_centered(|ui| {
                    let panel_width = ui.available_width() - 40.0;
                    let question_height = if rule_option == RuleOption::SpecialBy { 300.0 } else { 150.0 };
                    let q_label = format!("Q{}: {}", current_question, display_q_text);
                    self.ui_3d_card(ui, &q_label, 22.0, egui::vec2(panel_width, question_height), egui::Color32::from_rgb(40, 40, 50), 16.0, egui::Color32::from_rgb(0, 240, 240), None);
                });
                // --- 答え ---
                ui.vertical_centered(|ui| {
                    let panel_width = ui.available_width() - 60.0;
                    self.ui_3d_card(ui, &display_a_text, 15.0, egui::vec2(panel_width, 30.0), egui::Color32::from_rgb(40, 40, 50), 0.0, egui::Color32::from_rgb(0, 240, 240), None);
                });

                ui.add_space(20.0);

                egui::ScrollArea::both().show(ui, |ui| {
                    // タイル幅と間隔
                    let available_width = ui.available_width();
                    let player_count = players.len().max(1);
                    let spacing_x = 5.0;
                    let spacing_y = 5.0;
                    let tile_width = ((available_width/player_count as f32)-spacing_x).max(90.0);

                    egui::Grid::new("3d_grid_extended")
                        .spacing([spacing_x, spacing_y])
                        .show(ui, |ui| {
                            // --- 1R順位行 ---
                            for p in &players {
                                let ordinal = match p.id % 10 {
                                    1 if p.id % 100 != 11 => "st",
                                    2 if p.id % 100 != 12 => "nd",
                                    3 if p.id % 100 != 13 => "rd",
                                    _ => "th",
                                };
                                let rank_str = format!("{}{}", p.id.to_string(), ordinal);
                                self.ui_3d_card(ui, &rank_str, 18.0, egui::vec2(tile_width, 45.0), egui::Color32::from_rgb(200, 150, 80), 0.0, egui::Color32::from_rgb(240, 240, 0), None);
                            }
                            ui.end_row();

                            // --- 名前と所属・学年（統合） ---
                            for p in &players {
                                let is_frozen = display_statuses[&p.id].freeze_count > 0;
                                let name_card_color = if is_frozen {
                                    egui::Color32::from_rgb(0, 200, 255)
                                } else {
                                    egui::Color32::from_rgb(60, 60, 80)
                                };
                                if rule_option == RuleOption::SpecialBy {
                                    self.ui_3d_player_info_card_special_by(ui, &p.name, p.affiliation.as_deref(), p.grade.as_deref(), 30.0, 18.0, egui::vec2(tile_width, 170.0), name_card_color, 0.0, egui::Color32::from_rgb(240, 240, 0), None);
                                } else {
                                    self.ui_3d_player_info_card(ui, &p.name, p.affiliation.as_deref(), p.grade.as_deref(), 40.0, 20.0, egui::vec2(tile_width, 350.0), name_card_color, 0.0, egui::Color32::from_rgb(240, 240, 0), None);
                                }
                            }
                            ui.end_row();

                            // --- スコア行（アニメーション付き） ---
                            for p in &players {
                                let status = &display_statuses[&p.id];
                                let mut score_str = status.score.to_string();
                                let mut font_size = 45.0;

                                if let Some(rank) = status.finish_rank {
                                    if status.is_winner {
                                        score_str = format!("{}\n(Win! #{})", score_str, rank);
                                        font_size = 15.0;
                                    } else if status.is_eliminated {
                                        score_str = format!("{}\n(Elim #{})", score_str, rank);
                                        font_size = 15.0;
                                    } else {
                                        score_str = format!("{}\n(Rnk  #{})", score_str, rank);
                                        font_size = 15.0;
                                    }
                                } else if status.is_winner {
                                    score_str = format!("{}\n(Win!)    ", score_str);
                                    font_size = 15.0;
                                } else if status.is_eliminated {
                                    score_str = format!("{}\n(Elim)    ", score_str);
                                    font_size = 15.0;
                                }

                                let change = self.last_change_times.get(&p.id).cloned();
                                self.ui_3d_card(ui, &score_str, font_size, egui::vec2(tile_width, 60.0), egui::Color32::from_rgb(40, 80, 120), 8.0, egui::Color32::from_rgb(240, 240, 0), change);
                            }
                            ui.end_row();

                            // --- SpecialByの場合、x値とy値を横並びに配置 ---
                            if rule_option == RuleOption::SpecialBy {
                                for p in &players {
                                    // 1つのセルの中で水平に並べる
                                    ui.horizontal(|ui| {
                                        let spacing_between_cw = spacing_x / 2.0;
                                        ui.spacing_mut().item_spacing.x = spacing_between_cw;

                                        let status = &display_statuses[&p.id];
                                        let x_str = format!("x:{}", status.x);
                                        let y_str = format!("y:{}", status.y);

                                        let half_size = egui::vec2((tile_width-spacing_between_cw)/2.0, 30.0);

                                        // x値
                                        self.ui_3d_card(ui, &x_str, 25.0, half_size, egui::Color32::from_rgb(100, 60, 120), 0.0, egui::Color32::from_rgb(240, 240, 0), None);
                                        // y値
                                        self.ui_3d_card(ui, &y_str, 25.0, half_size, egui::Color32::from_rgb(120, 100, 60), 0.0, egui::Color32::from_rgb(240, 240, 0), None);
                                    });
                                }
                                ui.end_row();
                            }

                            // --- 正答数と誤答数を横並びに配置 ---
                            for p in &players {
                                // 1つのセルの中で水平に並べる
                                ui.horizontal(|ui| {
                                    let spacing_between_cw = spacing_x / 2.0;
                                    ui.spacing_mut().item_spacing.x = spacing_between_cw;

                                    let correct_val = display_statuses[&p.id].correct_count.to_string();
                                    let wrong_val = display_statuses[&p.id].wrong_count.to_string();

                                    let half_size = egui::vec2((tile_width-spacing_between_cw)/2.0, 30.0);

                                    // 正答数
                                    self.ui_3d_card(ui, &correct_val, 20.0, half_size, egui::Color32::from_rgb(40, 100, 40), 0.0, egui::Color32::from_rgb(240, 240, 0), None);
                                    // 誤答数
                                    self.ui_3d_card(ui, &wrong_val, 20.0, half_size, egui::Color32::from_rgb(120, 40, 40), 0.0, egui::Color32::from_rgb(240, 240, 0), None);
                                });
                            }
                            ui.end_row();
                        });
                });
            });

        // コントロールパネル
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("control_panel"),
            egui::ViewportBuilder::default()
                .with_title("Quiz Scoreboard Control")
                .with_inner_size([400.0, 500.0]),
            |ctx, class| {
                assert!(
                    class == egui::ViewportClass::Immediate,
                    "This platform doesn't support secondary viewports"
                );

                egui::CentralPanel::default().show(ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let state_arc = self.state.clone(); 
                        let mut data = state_arc.lock().unwrap();
                        ui.heading("Quiz Monitor");

                        ui.group(|ui| {
                            ui.set_width(ui.available_width());
                            // ディスプレイに映っているもの（前回分）
                            ui.label(egui::RichText::new("ON SCREEN (Previous):").color(egui::Color32::GOLD));
                            let prev_text = if data.current_question > 0 && (data.current_question as usize) <= data.questions.len() {
                                &data.questions[data.current_question as usize - 1].text.clone()
                            } else {
                                "-"
                            };
                            ui.label(prev_text);

                            ui.separator();

                            // これから出すもの（今回分）
                            ui.label(egui::RichText::new("NEXT UP (Current):").color(egui::Color32::LIGHT_BLUE));
                            let curr_idx = data.current_question as usize;
                            if curr_idx < data.questions.len() {
                                let curr_text = data.questions[curr_idx].text.clone(); 
                                ui.label(egui::RichText::new(curr_text).size(18.0).strong());
                            } else {
                                ui.label("No more questions.");
                            }

                            if data.round_completed {
                                ui.separator();
                                ui.label(egui::RichText::new("Round completed: next round players exported.").color(egui::Color32::GREEN));
                            }
                        });

                        ui.add_space(10.0);
                        ui.heading("Controller");
                        ui.separator();

                        // 問題進行
                        ui.horizontal(|ui| {
                            if ui.button("Next Question").clicked() {
                                if data.round_completed {
                                    // すでに最終問題完了後
                                } else if data.current_question as usize + 1 >= data.questions.len() {
                                    // 最終問題の後、完了状態にする
                                    self.export_next_round_players(&mut data);
                                } else {
                                    // ログ出力
                                    if let Err(e) = write_log_line(&self.config.log_csv, data.current_question as usize, &players, &data.player_events) {
                                        ui.label(format!("ログ書き込み失敗: {}", e));
                                    } else {
                                        ui.label(format!("ログ書き込み成功"));
                                    };

                                    // 問題確定: 現在の暫定ステータスを表示用に確定
                                    data.display_statuses = data.working_statuses.clone();

                                    // 凍結カウントダウン
                                    for status in data.display_statuses.values_mut() {
                                        if status.freeze_count > 0 {
                                            status.freeze_count -= 1;
                                        }
                                    }

                                    // 次の問題へ移行
                                    data.current_question += 1;

                                    // 新しい問題に向けてイベント・状態リセット
                                    data.player_events.clear();
                                    data.question_status = QuestionStatus::new();
                                    data.working_statuses = data.display_statuses.clone();

                                    if data.current_question as usize >= data.questions.len() {
                                        self.export_next_round_players(&mut data);
                                    }
                                }
                            }
                        });

                        ui.separator();

                        let player_info: Vec<(PlayerId, String)> = data.players.iter()
                            .map(|p| (p.id, p.name.clone()))
                            .collect();

                        // 解答操作
                        egui::Grid::new("answer_grid").striped(true).show(ui, |ui| {
                            for (pid, name) in &player_info {
                                ui.label(format!("{}: {}", pid, name));

                                let is_disabled = (data.question_status.finished && data.rule_option != RuleOption::QuickBoard)
                                    || data.working_statuses.get(pid).map_or(false, |s| s.freeze_count > 0);

                                if ui.add_enabled(!is_disabled, egui::Button::new(egui::RichText::new("Correct").color(egui::Color32::GREEN))).clicked() {
                                    data.player_events.entry(*pid).or_default().push(Event::Correct);
                                    if data.rule_option != RuleOption::QuickBoard {
                                        data.question_status.finished = true;
                                    }
                                    self.apply_pending_events(&mut data);
                                }

                                if ui.add_enabled(!is_disabled, egui::Button::new(egui::RichText::new("Wrong").color(egui::Color32::RED))).clicked() {
                                    data.player_events.entry(*pid).or_default().push(Event::Wrong);
                                    self.apply_pending_events(&mut data);
                                }

                                if ui.add_enabled(!is_disabled, egui::Button::new(egui::RichText::new("Buzz").color(egui::Color32::DARK_GRAY))).clicked() {
                                    let next_buzz = data.player_events.values().flat_map(|es| es.iter()).filter(|e| matches!(e, Event::Buzz(_))).count() as u32 + 1;
                                    data.player_events.entry(*pid).or_default().push(Event::Buzz(next_buzz));
                                    self.apply_pending_events(&mut data);
                                }

                                ui.end_row();
                            }
                        });

                        ui.separator();

                        // 手動得点操作
                        ui.label("解答操作の後、Next Questionの前に行うこと");
                        egui::Grid::new("edit_grid").striped(true).show(ui, |ui| {
                            for (pid, name) in &player_info {
                                let s = data.working_statuses.get_mut(pid).unwrap();
                                ui.label(format!("{}: {}", pid, name));
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
