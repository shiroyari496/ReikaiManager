use std::collections::{HashMap, HashSet};
use serde::Deserialize;

// --- プレイヤーのID型 ---
pub type PlayerId = usize;

// --- プレイヤー情報 ---
#[derive(Clone, Debug)]
pub struct Player {
    pub id: PlayerId,
    pub name: String,
    pub affiliation: Option<String>,
    pub grade: Option<String>,
}

// --- CSV読み込み用 ---
#[derive(Deserialize)]
pub struct PlayerRow {
    pub id: PlayerId,
    pub name: String,
    pub affiliation: Option<String>,
    pub grade: Option<String>,
}

impl From<PlayerRow> for Player {
    fn from(row: PlayerRow) -> Self {
        Player {
            id: row.id,
            name: row.name,
            affiliation: row.affiliation,
            grade: row.grade,
        }
    }
}

// --- 問題情報 ---
#[derive(Clone, Debug)]
pub struct Question {
    #[allow(dead_code)]
    pub id: usize,
    pub text: String,
    #[allow(dead_code)]
    pub answer: String,
}

// --- CSV読み込み用 ---
#[derive(Deserialize)]
pub struct QuestionRow {
    pub id: usize,
    pub text: String,
    pub answer: String,
}

impl From<QuestionRow> for Question {
    fn from(row: QuestionRow) -> Self {
        Question {
            id: row.id,
            text: row.text,
            answer: row.answer,
        }
    }
}

// --- ラウンド中の各プレイヤーの状態 ---
#[derive(Clone, Debug)]
pub struct PlayerStatus {
    pub score: i32,
    pub correct_count: u32,
    pub wrong_count: u32,
    pub frozen_until: Option<u32>,
    pub is_winner: bool,
    pub is_eliminated: bool,
}

impl PlayerStatus {
    pub fn new() -> Self {
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

impl Default for PlayerStatus {
    fn default() -> Self {
        Self::new()
    }
}

// --- 各問題の状態 ---
#[derive(Clone, Debug)]
pub struct QuestionStatus {
    pub finished: bool,
    pub locked: HashSet<PlayerId>,
}

impl QuestionStatus {
    pub fn new() -> Self {
        Self {
            finished: false,
            locked: HashSet::new(),
        }
    }
}

impl Default for QuestionStatus {
    fn default() -> Self {
        Self::new()
    }
}

// --- イベント（ラウンド中に入力される） ---
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Buzz(u32),  // 解答権を獲得した順番
    Correct,
    Wrong,
    Set(u32),
}

// --- GUIと共有するためのデータ ---
#[derive(Clone, Debug)]
pub struct SharedQuizState {
    pub players: Vec<Player>,
    pub statuses: HashMap<PlayerId, PlayerStatus>,
    pub current_question: u32,
}

impl SharedQuizState {
    pub fn new(players: Vec<Player>) -> Self {
        let mut statuses = HashMap::new();
        for p in &players {
            statuses.insert(p.id, PlayerStatus::new());
        }
        Self {
            players,
            statuses,
            current_question: 1,
        }
    }
}
