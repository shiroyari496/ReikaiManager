use std::collections::{HashMap, HashSet};
use std::fs::OpenOptions;
use std::io::{self, Write};
use eframe::egui::scroll_area::State;
use serde::Deserialize;
use eframe::egui;
use std::sync::{Arc, Mutex};
use std::thread;

struct DisplayData {
    players: Vec<Player>,
    statuses: HashMap<PlayerId, PlayerStatus>,
    problems: Vec<Question>,
}

// GUIと共有するためのデータ
struct SharedQuizState {
    players: Vec<Player>,
    statuses: HashMap<PlayerId, PlayerStatus>,
    current_question: u32,
}

// --- プレイヤーの情報 ---
type PlayerId = usize;

#[derive(Clone)]
struct Player {
    id: PlayerId,
    name: String,
    affiliation: Option<String>,
    grade: Option<String>,
}

// --- 問題の情報(仮) ---
#[derive(Clone)]
struct Question {
    id: usize,
    text: String,
    answer: String,
}

// --- CSVからプレイヤーの情報を読み込む ---
#[derive(Deserialize)]
struct PlayerRow {
    id: PlayerId,
    name: String,
    affiliation: Option<String>,
    grade: Option<String>,
}

fn load_players(path: &str) -> Vec<Player> {
    let mut rdr = csv::Reader::from_path(path).unwrap();

    // id, name, affiliation, name
    rdr.deserialize()
        .map(|result| {
            let row: PlayerRow = result.unwrap();
            Player {
                id: row.id,
                name: row.name,
                affiliation: row.affiliation,
                grade: row.grade,
            }
        })
        .collect()
}

// --- CSVから問題の情報を読み込む(仮) ---
#[derive(Deserialize)]
struct QuestionRow {
    id: usize,
    text: String,
    answer: String,
}

fn load_questions(path: &str) -> Vec<Question> {
    let mut rdr = csv::Reader::from_path(path).unwrap();

    rdr.deserialize()
        .map(|result| {
            let row: QuestionRow = result.unwrap();
            Question {
                id: row.id,
                text: row.text,
                answer: row.answer,
            }
        })
        .collect()
}

// --- ログの書き込み(仮) ---
fn write_log_head(path: &str, players: Vec<Player>) -> Result<(), Box<dyn std::error::Error>> {
    let mut wtr = csv::Writer::from_path(path)?;

    let mut row = vec!["id".to_string()];
    row.extend(players.iter().map(|p| p.id.to_string()));
    wtr.write_record(&row)?;

    let mut row = vec!["name".to_string()];
    row.extend(players.iter().map(|p| p.name.clone()));
    wtr.write_record(&row)?;

    let mut row = vec!["affiliation".to_string()];
    row.extend(players.iter().map(|p| p.affiliation.clone().unwrap_or_default()));
    wtr.write_record(&row)?;

    let mut row = vec!["grade".to_string()];
    row.extend(players.iter().map(|p| p.grade.clone().unwrap_or_default()));
    wtr.write_record(&row)?;

    wtr.flush()?;
    Ok(())
}

fn write_log_line(path: &str, problem_id: usize, players: Vec<Player> ,player_events: HashMap<PlayerId, Vec<Event>>) -> Result<(), Box<dyn std::error::Error>> {
    let file = OpenOptions::new()
        .write(true)
        .append(true)
        .open("data/log.csv")?;
    let mut wtr = csv::WriterBuilder::new()
        .has_headers(false) 
        .from_writer(file);

    let mut row = vec![problem_id.to_string()];
    for _ in 0..players.len() {
        row.push("".to_string());
    }

    for (id, events) in player_events {
        for event in events {
            if event == Event::Correct  {
                row[players.iter().position(|p| p.id == id).ok_or("")?+1].push('o');
            } else if event == Event::Wrong {
                row[players.iter().position(|p| p.id == id).ok_or("")?+1].push('x');
            }
        }
    }

    wtr.write_record(&row)?;
    wtr.flush()?;
    Ok(())
}

// --- イベント(ラウンド中に入力する) ---
#[derive(PartialEq, Eq)]
enum Event {
    Buzz(u32), // 解答権を獲得した順番
    Correct,
    Wrong,
}

// --- ラウンド中の各プレイヤーの状態 ---
struct PlayerStatus {
    score: i32,
    correct_count: u32,
    wrong_count: u32,
    frozen_until: Option<u32>, // 番号まで解答凍結
    is_winner: bool, // 勝ち抜け
    is_eliminated: bool, // 脱落
    // rank: Option<u32>, // ラウンド終了時順位
    // is_advance: bool, // 次ラウンド進出
}

impl PlayerStatus {
    fn new() -> Self {
        Self {
            score: 0,
            correct_count: 0,
            wrong_count: 0,
            frozen_until: None,
            is_winner: false,
            is_eliminated: false,
        }
    }
}

// -- 各問題の状態 --
struct QuestionStatus {
    finished: bool,
    locked: HashSet<PlayerId>,
}

impl QuestionStatus {
    fn new() -> Self {
        Self {
            finished: false,
            locked: HashSet::new(),
        }
    }
}

// --- ルール ---
trait QuizRule {
    fn apply(
        &self,
        player_statuses: &mut HashMap<PlayerId, PlayerStatus>,
        player_events: &mut HashMap<PlayerId, Vec<Event>>,
        question_status: &mut QuestionStatus,
    );
}

// ルール無し
struct FreeBatting;

impl QuizRule for FreeBatting {
    fn apply(
        &self,
        player_statuses: &mut HashMap<PlayerId, PlayerStatus>,
        player_events: &mut HashMap<PlayerId, Vec<Event>>,
        question_status: &mut QuestionStatus,
    ) {
        for (player_id, events) in player_events.iter() {
            let mut correct_count = 0;
            let mut wrong_count = 0;
            for event in events {
                match event {
                    Event::Buzz(_) => {},
                    Event::Correct => {
                        question_status.finished = true;
                        correct_count += 1
                    },
                    Event::Wrong => {
                        wrong_count += 1
                    },
                }
            }
            let status = player_statuses.entry(*player_id).or_insert_with(PlayerStatus::new);
            status.score += correct_count;
            status.correct_count += correct_count as u32;
            status.wrong_count += wrong_count as u32;
        }
    }
}

// N◯M×ルール
struct NCorrectMWrong {
    n: i32,
    m: i32,
}

impl NCorrectMWrong {
    fn new(n: i32, m: i32) -> Self {
        Self { n, m }
    }
}

impl QuizRule for NCorrectMWrong {
    fn apply(
        &self,
        player_statuses: &mut HashMap<PlayerId, PlayerStatus>,
        player_events: &mut HashMap<PlayerId, Vec<Event>>,
        question_status: &mut QuestionStatus,
    ) {
        for (player_id, events) in player_events.iter() {
            let mut correct_count = 0;
            let mut wrong_count = 0;
            for event in events {
                match event {
                    Event::Buzz(_) => {},
                    Event::Correct => {
                        question_status.finished = true;
                        correct_count += 1
                    },
                    Event::Wrong => {
                        wrong_count += 1
                    },
                }
            }
            let status = player_statuses.entry(*player_id).or_insert_with(PlayerStatus::new);
            status.score += correct_count;
            status.correct_count += correct_count as u32;
            status.wrong_count += wrong_count as u32;
            if status.correct_count >= self.n as u32 {
                status.is_winner = true;
            }
            if status.wrong_count >= self.m as u32 {
                status.is_eliminated = true;
            }
        }
    }
}

// N◯M休ルール

// UpDownルール

// NbyNルール

// PlusMinusルール

// Freezeルール

// AttackSurvivalルール

// --- 解答形式 ---
trait Format {
}

// --- メイン処理 ---
fn read_line() -> String {
    let mut s = String::new();
    io::stdin().read_line(&mut s).unwrap();
    s.trim().to_string()
}

fn main() -> eframe::Result<()> {
    let players = load_players("data/players.csv");
    let questions = load_questions("data/questions.csv");
    // println!("Players:");
    // for p in &players {
    //     println!("{}: {} ({} {})", p.id, p.name, p.affiliation.as_deref().unwrap_or(""), p.grade.as_deref().unwrap_or(""));
    // }
    // println!("Questions:");
    // for q in &questions {
    //     println!("{}: {} ({})", q.id, q.text, q.answer);
    // }

    // 共有状態の作成
    let mut initial_statuses = HashMap::new();
    for p in &players {
        initial_statuses.insert(p.id, PlayerStatus {
            score: 0,
            correct_count: 0,
            wrong_count: 0,
            frozen_until: None,
            is_winner: false,
            is_eliminated: false,
        });
    }

    let shared_state = Arc::new(Mutex::new(SharedQuizState {
        players: players.clone(),
        statuses: initial_statuses,
        current_question: 1,
    }));
    // 2. 【サブスレッド】ターミナル操作ロジック
    let state_for_thread = Arc::clone(&shared_state);
    thread::spawn(move || {
        // ここに元の main() のループ処理を移植
        run_terminal_loop(state_for_thread, players, questions);
    });

    // 3. 【メインスレッド】GUI起動 (ここでブロックされる)
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

fn setup_custom_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        "my_font".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/Noto_Sans_JP/NotoSansJP-VariableFont_wght.ttf")), // パスは環境に合わせてください
    );

    // フォントの優先順位を設定（Proportional: 文章用, Monospace: 等幅）
    fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap()
        .insert(0, "my_font".to_owned());
    fonts.families.get_mut(&egui::FontFamily::Monospace).unwrap()
        .insert(0, "my_font".to_owned());

    ctx.set_fonts(fonts);
}

fn run_terminal_loop(
    state: Arc<Mutex<SharedQuizState>>,
    players: Vec<Player>,
    questions: Vec<Question>,
) {
    let rule = NCorrectMWrong{ n: 7, m: 3 };
    let question_num = questions.len();

    let mut player_statuses: HashMap<PlayerId, PlayerStatus> = HashMap::new();

    println!("Players:");
    for p in &players {
        println!(
            "{}:\t{}\t({}\t{})",
            p.id, p.name, p.affiliation.as_deref().unwrap_or("-"), p.grade.as_deref().unwrap_or("-")
        );
    }
    write_log_head("data/log.csv", players.clone()).expect("ログの書き込みに失敗");

    for q in 1..=question_num {
        // 表示スレッドへ現在の問題番号を通知
        {
            state.lock().unwrap().current_question = q as u32;
        }
        // 勝ち抜け、脱落、凍結されていないプレイヤーがいなければ終了
        let active_players: Vec<&Player> = players.iter().filter(|p| {
            let status = player_statuses.get(&p.id);
            !status.map(|s| s.is_winner).unwrap_or(false) &&
            !status.map(|s| s.is_eliminated).unwrap_or(false) &&
            !status.map(|s| s.frozen_until).unwrap_or(None).map_or(false, |f| f > q as u32)
        }).collect();
        if active_players.is_empty() {
            println!("No active players remaining. Ending quiz.");
            break;
        }

        let mut question_status = QuestionStatus::new();
        let mut player_events: HashMap<PlayerId, Vec<Event>> = HashMap::new();

        println!("\n=== Question {} ===", q);
        println!("Question: {}", questions[q-1].text);
        println!("");

        let mut question_status = QuestionStatus::new();
        let mut player_events: HashMap<PlayerId, Vec<Event>> = HashMap::new();

        loop {
            print!("event> ");
            io::stdout().flush().unwrap();

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
            if parts.len() != 2 {
                println!("format: buzz A / correct A / wrong A / pass / next");
                continue;
            }

            let action = match parts[0] {
                "buzz" => "buzz",
                "correct" => "correct",
                "wrong" => "wrong",
                _ => {
                    println!("unknown action");
                    println!("format: buzz A / correct A / wrong A / pass / next");
                    continue;
                }
            };

            let id = match parts[1].parse::<PlayerId>() {
                Ok(id) => id,
                Err(_) => {
                    println!("invalid player id");
                    println!("Players:");
                    for p in &players {
                        println!("{}: {}", p.id, p.name);
                    }
                    continue;
                }
            };

            let player = match players.iter().find(|p| p.id == id) {
                Some(p) => p.id,
                None => {
                    println!("unknown player");
                    println!("Players:");
                    for p in &players {
                        println!("{}: {}", p.id, p.name);
                    }
                    continue;
                }
            };

            if question_status.locked.contains(&player) {
                println!("{} already locked out", players.iter().find(|p| p.id == player).unwrap().name);
                continue;
            }

            if player_statuses.get(&player).map(|s| s.frozen_until).unwrap_or(None).map_or(false, |f| f > q as u32) {
                println!("{} is frozen until question {}", players.iter().find(|p| p.id == player).unwrap().name, player_statuses.get(&player).unwrap().frozen_until.unwrap());
                continue;
            }

            if player_statuses.get(&player).map(|s| s.is_winner).unwrap_or(false) {
                println!("{} already winner", players.iter().find(|p| p.id == player).unwrap().name);
                continue;
            }

            if player_statuses.get(&player).map(|s| s.is_eliminated).unwrap_or(false) {
                println!("{} already eliminated", players.iter().find(|p| p.id == player).unwrap().name);
                continue;
            }

            let event = match action {
                "buzz" => Event::Buzz(question_status.locked.len() as u32 + 1),
                "correct" => {
                    question_status.finished = true;
                    Event::Correct
                },
                "wrong" => {
                    question_status.locked.insert(player);
                    Event::Wrong
                },
                _ => unreachable!(),
            };

            player_events.entry(player).or_insert_with(Vec::new).push(event);
        }
        rule.apply(&mut player_statuses, &mut player_events, &mut question_status);

        println!("\nScores:");
        for p in &players {
            println!(
                "{}:\t{}\t({}\t{})\t- score: {}\t(correct: {},\twrong: {})",
                p.id, p.name, p.affiliation.as_deref().unwrap_or("-"), p.grade.as_deref().unwrap_or("-"),
                player_statuses.get(&p.id).map(|s| s.score).unwrap_or(0),
                player_statuses.get(&p.id).map(|s| s.correct_count).unwrap_or(0),
                player_statuses.get(&p.id).map(|s| s.wrong_count).unwrap_or(0)
            );
        }
        {
            let mut data = state.lock().unwrap();
            // ここで rule.apply を呼ぶか、個別に status を更新する
            // 例: 暫定的に status を更新して GUI に即時反映させる
            rule.apply(&mut data.statuses, &mut player_events, &mut question_status);
        }
        write_log_line("data/log.csv", q, players.clone(), player_events).expect("ログの書き込みに失敗");
    }
}

// --- GUI アプリケーション構造体 ---
struct ScoreboardApp {
    state: Arc<Mutex<SharedQuizState>>,
}

impl eframe::App for ScoreboardApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let data = self.state.lock().unwrap();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(format!("Question #{}", data.current_question));
            ui.add_space(10.0);

            egui::ScrollArea::horizontal().show(ui, |ui| {
                egui::Grid::new("score_grid").striped(true).spacing([15.0, 8.0]).show(ui, |ui| {
                    // CSVヘッダ相当
                    ui.label("ID");
                    for p in &data.players { ui.label(p.id.to_string()); }
                    ui.end_row();

                    ui.label("Name");
                    for p in &data.players { ui.label(&p.name); }
                    ui.end_row();

                    ui.label("Affiliation");
                    for p in &data.players { ui.label(p.affiliation.as_deref().unwrap_or("-")); }
                    ui.end_row();

                    ui.label("Grade");
                    for p in &data.players { ui.label(p.grade.as_deref().unwrap_or("-")); }
                    ui.end_row();

                    ui.end_row(); ui.separator(); for _ in &data.players { ui.separator(); } ui.end_row();

                    // 現在の得点状況
                    ui.label(egui::RichText::new("SCORE").strong().color(egui::Color32::LIGHT_BLUE));
                    for p in &data.players {
                        let s = &data.statuses[&p.id];
                        ui.label(egui::RichText::new(s.score.to_string()).size(20.0).strong());
                    }
                    ui.end_row();

                    ui.label("Correct (○)");
                    for p in &data.players {
                        ui.label(egui::RichText::new(data.statuses[&p.id].correct_count.to_string()).color(egui::Color32::GREEN));
                    }
                    ui.end_row();

                    ui.label("Wrong (×)");
                    for p in &data.players {
                        ui.label(egui::RichText::new(data.statuses[&p.id].wrong_count.to_string()).color(egui::Color32::RED));
                    }
                    ui.end_row();
                });
            });
        });
        ctx.request_repaint(); // 常に最新の状態を描画
    }
}
