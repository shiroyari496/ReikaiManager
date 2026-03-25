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

/// ルール無し（全員が解答可能）
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
    pub n: i32,
    pub m: i32,
}

impl NCorrectMWrong {
    #[allow(dead_code)]
    pub fn new(n: i32, m: i32) -> Self {
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

// UpDownルール、NbyNルール、PlusMinusルール、Freezeルール、
// AttackSurvivalルールなどは実装時に追加してください
