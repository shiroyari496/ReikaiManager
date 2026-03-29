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
    pub freeze_count: u32,
    #[allow(dead_code)]
    pub frozen_until: Option<u32>,
    #[allow(dead_code)]
    pub is_winner: bool,
    #[allow(dead_code)]
    pub is_eliminated: bool,
}

impl PlayerStatus {
    pub fn new() -> Self {
        Self {
            score: 0,
            correct_count: 0,
            wrong_count: 0,
            freeze_count: 0,
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
#[allow(dead_code)]
pub struct QuestionStatus {
    pub finished: bool,
    pub locked: HashSet<PlayerId>,
}

impl QuestionStatus {
    #[allow(dead_code)]
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
#[allow(dead_code)]
pub enum Event {
    Buzz(u32),  // 解答権を獲得した順番
    Correct,
    Wrong,
    Set(u32),
}

// --- クイズルール選択 ---
#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum RuleOption {
    FreeBatting,
    NCorrectMWrong,
    UpDown,
    Freeze,
    NByM,
}

impl RuleOption {
    pub fn label(&self) -> &str {
        match self {
            Self::FreeBatting => "Free Batting",
            Self::NCorrectMWrong => "N Correct M Wrong",
            Self::UpDown => "UpDown",
            Self::Freeze => "N Freeze",
            Self::NByM => "N by M",
        }
    }

    pub fn all_options() -> &'static [RuleOption] {
        &[Self::FreeBatting, Self::NCorrectMWrong, Self::UpDown, Self::Freeze, Self::NByM]
    }
}

impl Default for RuleOption {
    fn default() -> Self {
        Self::FreeBatting
    }
}

// --- GUIと共有するためのデータ ---
#[derive(Clone, Debug)]
pub struct SharedQuizState {
    pub players: Vec<Player>,
    pub display_statuses: HashMap<PlayerId, PlayerStatus>,
    pub working_statuses: HashMap<PlayerId, PlayerStatus>,
    pub questions: Vec<Question>,
    pub current_question: u32,
    pub rule_option: RuleOption,
    pub n_correct: u32,
    pub m_wrong: u32,
}

impl SharedQuizState {
    #[allow(dead_code)]
    pub fn new(players: Vec<Player>, questions: Vec<Question>) -> Self {
        let mut display_statuses = HashMap::new();
        for p in &players {
            display_statuses.insert(p.id, PlayerStatus::new());
        }
        let working_statuses = display_statuses.clone();
        Self {
            players,
            display_statuses,
            working_statuses,
            questions,
            current_question: 0,
            rule_option: RuleOption::default(),
            n_correct: 7,
            m_wrong: 3,
        }
    }

    pub fn empty() -> Self {
        Self {
            players: Vec::new(),
            display_statuses: HashMap::new(),
            working_statuses: HashMap::new(),
            questions: Vec::new(),
            current_question: 0,
            rule_option: RuleOption::default(),
            n_correct: 7,
            m_wrong: 3,
        }
    }
}
