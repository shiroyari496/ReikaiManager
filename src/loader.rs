use crate::data::{Player, PlayerRow, Question, QuestionRow, PlayerId, PlayerStatus, Event};
use std::collections::HashMap;
use std::fs::OpenOptions;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// CSVファイルからプレイヤー情報を読み込む
pub fn load_players(path: &str) -> Result<Vec<Player>> {
    let mut rdr = csv::Reader::from_path(path)?;

    let mut players = Vec::new();
    for result in rdr.deserialize() {
        let row: PlayerRow = result?;
        players.push(Player::from(row));
    }

    if players.is_empty() {
        return Err("No players loaded from CSV".into());
    }

    Ok(players)
}

/// CSVファイルから問題情報を読み込む
pub fn load_questions(path: &str) -> Result<Vec<Question>> {
    let mut rdr = csv::Reader::from_path(path)?;

    let mut questions = Vec::new();
    for result in rdr.deserialize() {
        let row: QuestionRow = result?;
        questions.push(Question::from(row));
    }

    if questions.is_empty() {
        return Err("No questions loaded from CSV".into());
    }

    Ok(questions)
}

/// ログファイルのヘッダーを書き込む
#[allow(dead_code)]
pub fn write_log_head(path: &str, players: &[Player]) -> Result<()> {
    let mut wtr = csv::Writer::from_path(path)?;

    let mut row = vec!["id".to_string()];
    row.extend(players.iter().map(|p| p.id.to_string()));
    wtr.write_record(&row)?;

    let mut row = vec!["name".to_string()];
    row.extend(players.iter().map(|p| p.name.clone()));
    wtr.write_record(&row)?;

    let mut row = vec!["affiliation".to_string()];
    row.extend(
        players
            .iter()
            .map(|p| p.affiliation.clone().unwrap_or_default()),
    );
    wtr.write_record(&row)?;

    let mut row = vec!["grade".to_string()];
    row.extend(
        players
            .iter()
            .map(|p| p.grade.clone().unwrap_or_default()),
    );
    wtr.write_record(&row)?;

    wtr.flush()?;
    Ok(())
}

/// ログファイルに1行書き込む
#[allow(dead_code)]
pub fn write_log_line(
    path: &str,
    problem_id: usize,
    players: &[Player],
    player_events: &HashMap<PlayerId, Vec<Event>>,
) -> Result<()> {
    let file = OpenOptions::new()
        .write(true)
        .append(true)
        .open(path)?;
    let mut wtr = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(file);

    let mut row = vec![problem_id.to_string()];
    for _ in 0..players.len() {
        row.push("".to_string());
    }

    for (id, events) in player_events {
        let player_idx = players
            .iter()
            .position(|p| p.id == *id)
            .ok_or(format!("Player with id {} not found", id))?;

        for event in events {
            match event {
                Event::Correct => row[player_idx + 1].push('o'),
                Event::Wrong => row[player_idx + 1].push('x'),
                _ => {}
            }
        }
    }

    wtr.write_record(&row)?;
    wtr.flush()?;
    Ok(())
}

pub fn write_next_round_players(
    path: &str,
    players: &[Player],
    statuses: &HashMap<PlayerId, PlayerStatus>,
    advance_count: usize,
) -> Result<()> {
    let mut ranked: Vec<(PlayerId, u32)> = statuses
        .iter()
        .filter_map(|(&pid, status)| status.finish_rank.map(|r| (pid, r)))
        .collect();

    ranked.sort_by_key(|(_, r)| *r);

    let mut selected = Vec::new();
    for (pid, _rank) in &ranked {
        if selected.len() >= advance_count {
            break;
        }
        if let Some(player) = players.iter().find(|p| p.id == *pid) {
            selected.push(player.clone());
        }
    }

    if selected.len() < advance_count {
        for player in players {
            if selected.iter().any(|p| p.id == player.id) {
                continue;
            }
            selected.push(player.clone());
            if selected.len() >= advance_count {
                break;
            }
        }
    }

    let mut wtr = csv::Writer::from_path(path)?;
    wtr.write_record(&["id", "name", "affiliation", "grade"])?;
    for p in selected {
        wtr.write_record(&[
            p.id.to_string(),
            p.name.clone(),
            p.affiliation.clone().unwrap_or_default(),
            p.grade.clone().unwrap_or_default(),
        ])?;
    }
    wtr.flush()?;
    Ok(())
}
