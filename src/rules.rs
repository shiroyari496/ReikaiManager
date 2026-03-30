use crate::data::{PlayerId, PlayerStatus, Event, QuestionStatus};
use std::collections::HashMap;

/// クイズルールの trait
pub trait QuizRule {
    fn apply(
        &self,
        player_statuses: &mut HashMap<PlayerId, PlayerStatus>,
        player_events: &mut HashMap<PlayerId, Vec<Event>>,
        question_status: &mut QuestionStatus,
    );
}

/// ルール無し
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
            status.correct_count += correct_count as u32;
            status.wrong_count += wrong_count as u32;
            status.score += correct_count as i32;
        }
    }
}

/// N◯M×ルール
pub struct NCorrectMWrong {
    pub n: u32,
    pub m: u32,
}

impl NCorrectMWrong {
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
            status.correct_count += correct_count as u32;
            status.wrong_count += wrong_count as u32;
            status.score += correct_count as i32;
            if status.score >= self.n as i32 {
                status.is_winner = true;
            }
            if status.wrong_count >= self.m as u32 {
                status.is_eliminated = true;
            }
        }
    }
}

/// NFreezeルール
pub struct Freeze {
    pub n: u32,
}

impl Freeze {
    pub fn new(n: u32) -> Self {
        Self { n }
    }
}

impl QuizRule for Freeze {
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
            status.correct_count += correct_count as u32;
            status.wrong_count += wrong_count as u32;
            status.score += correct_count as i32;
            if wrong_count > 0 {
                status.freeze_count = status.wrong_count + 1;
            }
            if status.score >= self.n as i32 {
                status.is_winner = true;
            }
        }
    }
}

/// NbyMルール
pub struct NByM {
    pub n: u32,
    pub m: u32,
}

impl NByM {
    pub fn new(n: u32, m: u32) -> Self {
        Self { n, m }
    }
}

impl QuizRule for NByM {
    fn apply(
        &self,
        player_statuses: &mut HashMap<PlayerId, PlayerStatus>,
        player_events: &mut HashMap<PlayerId, Vec<Event>>,
        _question_status: &mut QuestionStatus,
    ) {
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
            status.correct_count += correct_delta as u32;
            status.wrong_count += wrong_delta as u32;
            status.score = status.correct_count as i32 * (self.m - status.wrong_count) as i32;
            if status.score >= self.n as i32 * self.m as i32 {
                status.is_winner = true;
            }
            if status.wrong_count >= self.m as u32 {
                status.is_eliminated = true;
            }
        }
    }
}

/// NUpDownルール
pub struct UpDown {
    pub n: u32,
    pub m: u32,
}

impl UpDown {
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
                        correct_count = 0;
                    }
                    _ => {}
                }
            }
            let status = player_statuses
                .entry(*player_id)
                .or_insert_with(PlayerStatus::new);
            status.correct_count += correct_count as u32;
            status.wrong_count += wrong_count as u32;
            status.score += correct_count as i32;
            if wrong_count > 0 { status.score = 0; }
            if status.score >= self.n as i32 {
                status.is_winner = true;
            }
            if status.wrong_count >= self.m as u32 {
                status.is_eliminated = true;
            }
        }
    }
}

/// ルール選択に基づいて適切なルールを適用する
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
        crate::data::RuleOption::Freeze => {
            Freeze::new(n).apply(player_statuses, player_events, question_status);
        }
        crate::data::RuleOption::NByM => {
            NByM::new(n, m).apply(player_statuses, player_events, question_status);
        }
    }
}
