mod data;
mod loader;
mod rules;
mod terminal;

use eframe::egui;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::time::Instant;

use crate::data::{Player, Question, PlayerStatus, QuestionStatus, Event, SharedQuizState, PlayerId, RuleOption};
use crate::loader::{load_players, load_questions, write_log_head, write_log_line};
use crate::terminal::{read_line, show_prompt, display_players, display_question, display_scores, 
                     handle_set_command, handle_answer_command};

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

#[allow(dead_code)]
fn run_terminal_loop(
    state: Arc<Mutex<SharedQuizState>>,
    players: Vec<Player>,
    questions: Vec<Question>,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::rules::apply_selected_rule;
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

            apply_selected_rule(&rule_option, n, m, &mut player_statuses, &mut player_events, &mut question_status);
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
    players_csv: String,
    questions_csv: String,
    log_csv: String,
    rule_option: RuleOption,
    n_correct: u32,
    m_wrong: u32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            players_csv: "data/players.csv".into(),
            questions_csv: "data/questions.csv".into(),
            log_csv: "data/log.csv".into(),
            rule_option: RuleOption::default(),
            n_correct: 7,
            m_wrong: 3,
        }
    }
}

struct ScoreboardApp {
    state: Arc<Mutex<SharedQuizState>>,
    is_3d_mode: bool,
    is_config_mode: bool,
    config: AppConfig,
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
            is_3d_mode: true,
            is_config_mode: true,
            config: AppConfig::default(),
            config_message: "設定を入力して [Load Data] を押してください".into(),
            last_scores: HashMap::new(),
            last_change_times: HashMap::new(),
        }
    }

    fn apply_config_to_state(&mut self, players: Vec<Player>, questions: Vec<Question>) {
        let mut display_statuses = HashMap::new();
        for p in &players {
            display_statuses.insert(p.id, PlayerStatus::new());
        }

        let mut data = self.state.lock().unwrap();
        data.players = players;
        data.questions = questions;
        data.display_statuses = display_statuses.clone();
        data.working_statuses = display_statuses;
        data.current_question = 0;
        data.rule_option = self.config.rule_option;
        data.n_correct = self.config.n_correct;
        data.m_wrong = self.config.m_wrong;
    }

    fn render_config_ui(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Quiz Startup Configuration");
            ui.add_space(8.0);

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

            ui.add_space(8.0);
            ui.label("Rule option");
            for &rule in RuleOption::all_options() {
                if ui
                    .selectable_label(self.config.rule_option == rule, rule.label())
                    .clicked()
                {
                    self.config.rule_option = rule;
                }
            }

            if self.config.rule_option == RuleOption::NCorrectMWrong || self.config.rule_option == RuleOption::UpDown {
                ui.horizontal(|ui| {
                    ui.label("N Correct");
                    ui.add(egui::Slider::new(&mut self.config.n_correct, 1..=20).text(""));
                });
                ui.horizontal(|ui| {
                    ui.label("M Wrong");
                    ui.add(egui::Slider::new(&mut self.config.m_wrong, 1..=20).text(""));
                });
            }

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("Load Data").clicked() {
                    let players = load_players(&self.config.players_csv);
                    let questions = load_questions(&self.config.questions_csv);

                    match (players, questions) {
                        (Ok(p), Ok(q)) => {
                            if let Err(e) = write_log_head(&self.config.log_csv, &p) {
                                self.config_message = format!("CSV読み込み成功、ただしログヘッダー書き込み失敗: {}", e);
                            } else {
                                self.config_message = "CSV読み込み成功。ゲームを開始できます。".into();
                            }
                            self.apply_config_to_state(p, q);
                            self.is_config_mode = false;
                        }
                        (Err(e), _) => {
                            self.config_message = format!("プレイヤー読み込みエラー: {}", e);
                        }
                        (_, Err(e)) => {
                            self.config_message = format!("問題読み込みエラー: {}", e);
                        }
                    }
                }

                if ui.button("Reset to defaults").clicked() {
                    self.config = AppConfig::default();
                    self.config_message = "デフォルト設定にリセットしました。".into();
                }
            });

            ui.add_space(8.0);
            ui.label(egui::RichText::new(&self.config_message).color(egui::Color32::YELLOW));
            ui.add_space(8.0);

            ui.separator();
            ui.label("設定完了後、画面上部のコントロールを使ってゲームを進めてください。");
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

                    // --- 1R順位 行 ---
                    ui.label(egui::RichText::new("1R Rank").size(header_size));
                    for p in players {
                        let ordinal = match p.id % 10 {
                            1 if p.id % 100 != 11 => "st",
                            2 if p.id % 100 != 12 => "nd",
                            3 if p.id % 100 != 13 => "rd",
                            _ => "th",
                        };
                        let rank_str = format!("{}{}", p.id.to_string(), ordinal);
                        ui.label(egui::RichText::new(rank_str).size(body_size).strong());
                    }
                    ui.end_row();

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

        // 背景色
        let bg_color = egui::Color32::from_rgb(20, 20, 30);

        // 共有データの取得
        let (current_question, questions, players, display_statuses) = {
            let data = self.state.lock().unwrap();
            (data.current_question as usize, data.questions.clone(), data.players.clone(), data.display_statuses.clone())
        };

        // メイン画面用の問題文 (indexが1以上なら「前回」を表示)
        let display_q_text = if current_question > 0 && current_question <= questions.len() {
            &questions[current_question - 1].text
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
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(bg_color))
            .show(ctx, |ui| {
                // --- コントロールパネル（ルール選択など）削除予定 ---
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        // 3D モード トグル
                        ui.checkbox(&mut self.is_3d_mode, "3D Mode");
                        ui.separator();

                        // ルール選択
                        ui.label("Rule:");
                        let mut current_rule = {
                            let state = self.state.lock().unwrap();
                            state.rule_option
                        };

                        let mut rule_changed = false;
                        for &rule in RuleOption::all_options() {
                            if ui.selectable_label(current_rule == rule, rule.label()).clicked() {
                                current_rule = rule;
                                rule_changed = true;
                            }
                        }

                        if rule_changed {
                            let mut state = self.state.lock().unwrap();
                            state.rule_option = current_rule;
                        }

                        // N Correct M Wrong / NFreeze / NbyM / UpDown パラメータ編集
                        if current_rule == RuleOption::NCorrectMWrong
                            || current_rule == RuleOption::UpDown
                            || current_rule == RuleOption::NFreeze
                            || current_rule == RuleOption::NbyM
                        {
                            ui.separator();
                            let mut state = self.state.lock().unwrap();
                            ui.label("N Correct:");
                            ui.add(egui::Slider::new(&mut state.n_correct, 1..=10).text(""));
                            ui.label("M Wrong:");
                            ui.add(egui::Slider::new(&mut state.m_wrong, 1..=10).text(""));
                        }
                    });
                });

                ui.add_space(10.0);

                if self.is_3d_mode {
                    // --- 問題文パネル ---
                    ui.vertical_centered(|ui| {
                        let panel_width = ui.available_width() - 40.0;
                        let q_label = format!("Q{}: {}", current_question, display_q_text);
                        self.ui_3d_card(ui, &q_label, 22.0, egui::vec2(panel_width, 60.0), egui::Color32::from_rgb(40, 40, 50), 16.0, egui::Color32::from_rgb(0, 240, 240), None);
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
                                    self.ui_3d_player_info_card(ui, &p.name, p.affiliation.as_deref(), p.grade.as_deref(), 40.0, 20.0, egui::vec2(tile_width, 350.0), name_card_color, 0.0, egui::Color32::from_rgb(240, 240, 0), None);
                                }
                                ui.end_row();

                                // --- スコア行（アニメーション付き） ---
                                for p in &players {
                                    let score_str = display_statuses[&p.id].score.to_string();
                                    let change = self.last_change_times.get(&p.id).cloned();
                                    self.ui_3d_card(ui, &score_str, 45.0, egui::vec2(tile_width, 60.0), egui::Color32::from_rgb(40, 80, 120), 8.0, egui::Color32::from_rgb(240, 240, 0), change);
                                }
                                ui.end_row();

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
                } else {
                    self.render_classic_grid(ui, &players, &display_statuses);
                }
            });

        // 4. コントロールパネル
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
                    let mut data = self.state.lock().unwrap();
                    ui.heading("Quiz Monitor");
                    ui.group(|ui| {
                        ui.set_width(ui.available_width());
                        // ディスプレイに映っているもの（前回分）
                        ui.label(egui::RichText::new("ON SCREEN (Previous):").color(egui::Color32::GOLD));
                        let prev_text = if data.current_question > 0 { &data.questions[data.current_question as usize - 1].text } else { "-" };
                        ui.label(prev_text);

                        ui.separator();

                        // これから出すもの（今回分）
                        ui.label(egui::RichText::new("NEXT UP (Current):").color(egui::Color32::LIGHT_BLUE));
                        let curr_idx = data.current_question as usize;
                        if curr_idx < data.questions.len() {
                            ui.label(egui::RichText::new(&data.questions[curr_idx].text).size(18.0).strong());
                        } else {
                            ui.label("No more questions.");
                        }
                    });

                    ui.add_space(10.0);
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

                    // 解答操作
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

                    // 手動得点操作
                    ui.separator();
                    ui.label("Player Status Edit");
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
