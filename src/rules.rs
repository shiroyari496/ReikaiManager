use crate::data::{PlayerId, PlayerStatus, Event, QuestionStatus};
use std::collections::HashMap;

/// クイズルールの trait
#[allow(dead_code)]
pub trait QuizRule {
    fn apply(
        &self,
        player_statuses: &mut HashMap<PlayerId, PlayerStatus>,
        player_events: &mut HashMap<PlayerId, Vec<Event>>,
        question_status: &mut QuestionStatus,
    );
}

/// ルール無し（全員が解答可能）
#[allow(dead_code)]
pub struct FreeBatting;

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
                    Event::Buzz(_) => {}
                    Event::Correct => {
                        question_status.finished = true;
                        correct_count += 1
                    }
                    Event::Wrong => wrong_count += 1,
                    _ => {}
                }
            }
            let status = player_statuses
                .entry(*player_id)
                .or_insert_with(PlayerStatus::new);
            status.score += correct_count;
            status.correct_count += correct_count as u32;
            status.wrong_count += wrong_count as u32;
        }
    }
}

/// N◯M×ルール
#[allow(dead_code)]
pub struct NCorrectMWrong {
    pub n: u32,
    pub m: u32,
}

impl NCorrectMWrong {
    #[allow(dead_code)]
    pub fn new(n: u32, m: u32) -> Self {
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
                    Event::Buzz(_) => {}
                    Event::Correct => {
                        question_status.finished = true;
                        correct_count += 1
                    }
                    Event::Wrong => wrong_count += 1,
                    _ => {}
                }
            }
            let status = player_statuses
                .entry(*player_id)
                .or_insert_with(PlayerStatus::new);
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

/// NFreezeルール
#[allow(dead_code)]
pub struct NFreeze {
    pub n: u32,
    pub m: u32,
}

impl NFreeze {
    #[allow(dead_code)]
    pub fn new(n: u32, m: u32) -> Self {
        Self { n, m }
    }
}

impl QuizRule for NFreeze {
    fn apply(
        &self,
        player_statuses: &mut HashMap<PlayerId, PlayerStatus>,
        player_events: &mut HashMap<PlayerId, Vec<Event>>,
        _question_status: &mut QuestionStatus,
    ) {
        for (player_id, events) in player_events.iter() {
            let mut correct_count = 0;
            let mut wrong_count = 0;
            for event in events {
                match event {
                    Event::Buzz(_) => {}
                    Event::Correct => {
                        correct_count += 1;
                    }
                    Event::Wrong => {
                        wrong_count += 1;
                    }
                    _ => {}
                }
            }
            let status = player_statuses
                .entry(*player_id)
                .or_insert_with(PlayerStatus::new);

            status.score += correct_count as i32;
            status.correct_count += correct_count as u32;
            status.wrong_count += wrong_count as u32;

            if wrong_count > 0 {
                status.freeze_count = status.wrong_count + 1;
            }

            if status.correct_count >= self.n as u32 {
                status.is_winner = true;
            }
            if status.wrong_count >= self.m as u32 {
                status.is_eliminated = true;
            }
        }
    }
}

/// NbyMルール
#[allow(dead_code)]
pub struct NbyM {
    pub n: u32,
    pub m: u32,
}

impl NbyM {
    #[allow(dead_code)]
    pub fn new(n: u32, m: u32) -> Self {
        Self { n, m }
    }
}

impl QuizRule for NbyM {
    fn apply(
        &self,
        player_statuses: &mut HashMap<PlayerId, PlayerStatus>,
        player_events: &mut HashMap<PlayerId, Vec<Event>>,
        _question_status: &mut QuestionStatus,
    ) {
        let lim = self.n.saturating_mul(self.m);

        for (player_id, events) in player_events.iter() {
            let mut correct_delta = 0;
            let mut wrong_delta = 0;

            for event in events {
                match event {
                    Event::Buzz(_) => {}
                    Event::Correct => {
                        correct_delta += 1;
                    }
                    Event::Wrong => {
                        wrong_delta += 1;
                    }
                    _ => {}
                }
            }

            let status = player_statuses
                .entry(*player_id)
                .or_insert_with(PlayerStatus::new);

            // 初期化: correct=0, wrong=m
            if status.correct_count == 0 && status.wrong_count == 0 {
                status.wrong_count = self.m;
            }

            status.correct_count = status.correct_count.saturating_add(correct_delta as u32);
            status.wrong_count = status.wrong_count.saturating_sub(wrong_delta as u32);
            status.score += correct_delta as i32;

            if status.correct_count.saturating_mul(status.wrong_count) >= lim {
                status.is_winner = true;
            }
            if status.wrong_count == 0 {
                status.is_eliminated = true;
            }
        }
    }
}

/// NUpDownルール
#[allow(dead_code)]
pub struct UpDown {
    pub n: u32,
    pub m: u32,
}

impl UpDown {
    #[allow(dead_code)]
    pub fn new(n: u32, m:u32) -> Self {
        Self { n, m }
    }
}

impl QuizRule for UpDown {
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
                    Event::Buzz(_) => {}
                    Event::Correct => {
                        question_status.finished = true;
                        correct_count += 1
                    }
                    Event::Wrong => {
                        wrong_count += 1;
                        correct_count = 0; // 間違えたら正解数リセット
                    }
                    _ => {}
                }
            }
            let status = player_statuses
                .entry(*player_id)
                .or_insert_with(PlayerStatus::new);
            if wrong_count > 0 { status.score = 0; }
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

/// ルール選択に基づいて適切なルールを適用する
#[allow(dead_code)]
pub fn apply_selected_rule(
    rule_option: &crate::data::RuleOption,
    n: u32,
    m: u32,
    player_statuses: &mut HashMap<PlayerId, PlayerStatus>,
    player_events: &mut HashMap<PlayerId, Vec<Event>>,
    question_status: &mut QuestionStatus,
) {
    match rule_option {
        crate::data::RuleOption::FreeBatting => {
            FreeBatting.apply(player_statuses, player_events, question_status);
        }
        crate::data::RuleOption::NCorrectMWrong => {
            NCorrectMWrong::new(n, m).apply(player_statuses, player_events, question_status);
        }
        crate::data::RuleOption::UpDown => {
            UpDown::new(n, m).apply(player_statuses, player_events, question_status);
        }
        crate::data::RuleOption::NFreeze => {
            NFreeze::new(n, m).apply(player_statuses, player_events, question_status);
        }
        crate::data::RuleOption::NbyM => {
            NbyM::new(n, m).apply(player_statuses, player_events, question_status);
        }
    }
}
