use crate::data::{Player, PlayerStatus, PlayerId, Event, QuestionStatus};
use std::collections::HashMap;
use std::io::{self, Write};

/// 標準入力から1行読み込む
pub fn read_line() -> String {
    let mut s = String::new();
    io::stdin().read_line(&mut s).unwrap();
    s.trim().to_string()
}

/// ターミナルでコマンドプロンプトを表示
pub fn show_prompt() {
    print!("event> ");
    io::stdout().flush().unwrap();
}

/// プレイヤー情報を表示
pub fn display_players(players: &[Player]) {
    println!("Players:");
    for p in players {
        println!("{}: {}", p.id, p.name);
    }
}

/// 問題を表示
pub fn display_question(question_id: usize, question_text: &str) {
    println!("\n=== Question {} ===", question_id);
    println!("Question: {}", question_text);
    println!("");
}

/// スコアボードを表示
pub fn display_scores(players: &[Player], player_statuses: &HashMap<PlayerId, PlayerStatus>) {
    println!("\nScores:");
    for p in players {
        let status = player_statuses.get(&p.id).cloned().unwrap_or_default();
        println!(
            "{}:\t{}\t({}\t{})\t- score: {}\t(correct: {},\twrong: {})",
            p.id,
            p.name,
            p.affiliation.as_deref().unwrap_or("-"),
            p.grade.as_deref().unwrap_or("-"),
            status.score,
            status.correct_count,
            status.wrong_count
        );
    }
}

/// `set` コマンドを解析してスコアを設定
pub fn handle_set_command(
    parts: &[&str],
    players: &[Player],
    player_statuses: &mut HashMap<PlayerId, PlayerStatus>,
    player_events: &mut HashMap<PlayerId, Vec<Event>>,
) -> Result<(), String> {
    if parts.len() != 3 {
        return Err("format: set <player_id> <score>".to_string());
    }

    let id: PlayerId = parts[1]
        .parse()
        .map_err(|_| "invalid player id".to_string())?;

    let new_score: i32 = parts[2]
        .parse()
        .map_err(|_| "invalid score value".to_string())?;

    let player = players
        .iter()
        .find(|p| p.id == id)
        .ok_or("unknown player")?;

    let status = player_statuses
        .entry(player.id)
        .or_insert_with(PlayerStatus::new);
    status.score = new_score;
    player_events
        .entry(player.id)
        .or_insert_with(Vec::new)
        .push(Event::Set(new_score as u32));

    println!("Player {} score set to {}", player.name, new_score);
    Ok(())
}

/// buzz/correct/wrong コマンドを解析
pub fn handle_answer_command(
    parts: &[&str],
    players: &[Player],
    player_statuses: &mut HashMap<PlayerId, PlayerStatus>,
    player_events: &mut HashMap<PlayerId, Vec<Event>>,
    question_status: &mut QuestionStatus,
) -> Result<(), String> {
    if parts.len() != 2 {
        return Err("format: buzz <player_id> / correct <player_id> / wrong <player_id>".to_string());
    }

    let action = parts[0];
    let id: PlayerId = parts[1]
        .parse()
        .map_err(|_| "invalid player id".to_string())?;

    let player = players
        .iter()
        .find(|p| p.id == id)
        .ok_or("unknown player")?;

    let status = player_statuses.get(&player.id);

    // 各種制約をチェック
    if question_status.locked.contains(&player.id) {
        return Err(format!("{} already locked out", player.name));
    }

    if status.map(|s| s.frozen_until).flatten().map_or(false, |f| f > 0) {
        let frozen_until = status.unwrap().frozen_until.unwrap();
        return Err(format!("{} is frozen until question {}", player.name, frozen_until));
    }

    if status.map(|s| s.is_winner).unwrap_or(false) {
        return Err(format!("{} already winner", player.name));
    }

    if status.map(|s| s.is_eliminated).unwrap_or(false) {
        return Err(format!("{} already eliminated", player.name));
    }

    // イベントを生成
    let event = match action {
        "buzz" => Event::Buzz(question_status.locked.len() as u32 + 1),
        "correct" => {
            question_status.finished = true;
            Event::Correct
        }
        "wrong" => {
            question_status.locked.insert(player.id);
            Event::Wrong
        }
        _ => return Err("unknown action".to_string()),
    };

    player_events
        .entry(player.id)
        .or_insert_with(Vec::new)
        .push(event);

    Ok(())
}
