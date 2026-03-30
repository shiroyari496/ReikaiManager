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

/// 連答付き N◯M×
pub struct RenDatsuNCorrectMWrong {
    pub n: u32,
    pub m: u32,
}

impl RenDatsuNCorrectMWrong {
    pub fn new(n: u32, m: u32) -> Self {
        Self { n, m }
    }
}

impl QuizRule for RenDatsuNCorrectMWrong {
    fn apply(
        &self,
        player_statuses: &mut HashMap<PlayerId, PlayerStatus>,
        player_events: &mut HashMap<PlayerId, Vec<Event>>,
        question_status: &mut QuestionStatus,
    ) {
        let any_correct = player_events.values().any(|events| events.iter().any(|e| matches!(e, Event::Correct)));

        for (player_id, events) in player_events.iter() {
            let status = player_statuses
                .entry(*player_id)
                .or_insert_with(PlayerStatus::new);

            let mut correct_delta = 0;
            let mut wrong_delta = 0;
            let mut score_delta = 0;

            // イベント内容をチェック
            let has_self_correct = events.iter().any(|e| matches!(e, Event::Correct));
            let has_self_wrong = events.iter().any(|e| matches!(e, Event::Wrong));

            // まず誤答数と正答数をカウント
            for event in events {
                match event {
                    Event::Buzz(_) => {}
                    Event::Correct => {
                        question_status.finished = true;
                        correct_delta += 1;
                        if status.has_streak_right {
                            score_delta += 2; // 1 point + 連答ボーナス
                        } else {
                            score_delta += 1;
                        }
                    }
                    Event::Wrong => {
                        wrong_delta += 1;
                    }
                    _ => {}
                }
            }

            status.correct_count += correct_delta as u32;
            status.wrong_count += wrong_delta as u32;
            status.score += score_delta;

            // 連答権の更新ロジック
            // 誤答があれば連答権は失われる
            if has_self_wrong {
                status.has_streak_right = false;
            }
            // 他人が正答した場合、自分が正答していなければ連答権は失われる
            else if any_correct && !has_self_correct {
                status.has_streak_right = false;
            }
            // 自分が正答した場合
            else if has_self_correct {
                // 連答権を持ったまま正答した場合は消失、そうでない場合は新規付与
                if status.has_streak_right {
                    status.has_streak_right = false;
                } else {
                    status.has_streak_right = true;
                }
            }
            // それ以外は状態を維持（Buzzのみなど）

            if status.score >= self.n as i32 {
                status.is_winner = true;
            }
            if status.wrong_count >= self.m as u32 {
                status.is_eliminated = true;
            }
        }

        if any_correct {
            question_status.finished = true;
        }
    }
}

/// 早押しボード
pub struct QuickBoard;

impl QuizRule for QuickBoard {
    fn apply(
        &self,
        player_statuses: &mut HashMap<PlayerId, PlayerStatus>,
        player_events: &mut HashMap<PlayerId, Vec<Event>>,
        _question_status: &mut QuestionStatus,
    ) {
        let correct_players: Vec<PlayerId> = player_events
            .iter()
            .filter(|(_, events)| events.iter().any(|e| matches!(e, Event::Correct)))
            .map(|(&pid, _)| pid)
            .collect();
        let correct_count = correct_players.len();

        for (player_id, events) in player_events.iter() {
            let status = player_statuses
                .entry(*player_id)
                .or_insert_with(PlayerStatus::new);

            let pressed = events.iter().any(|e| matches!(e, Event::Buzz(_)));
            let is_correct = events.iter().any(|e| matches!(e, Event::Correct));
            let is_wrong = events.iter().any(|e| matches!(e, Event::Wrong));

            let mut points = 0;
            if is_correct {
                points += if pressed { 3 } else { 1 };
            }
            if is_wrong && pressed {
                points -= 2;
            }

            if is_correct {
                if correct_count == 1 {
                    points += 2;
                } else if (2..=3).contains(&correct_count) {
                    points += 1;
                }
            }

            status.score += points;
            status.correct_count += is_correct as u32;
            status.wrong_count += is_wrong as u32;
            status.has_streak_right = false;
        }
    }
}

/// 変則by
pub struct SpecialBy;

impl QuizRule for SpecialBy {
    fn apply(
        &self,
        player_statuses: &mut HashMap<PlayerId, PlayerStatus>,
        player_events: &mut HashMap<PlayerId, Vec<Event>>,
        _question_status: &mut QuestionStatus,
    ) {
        // current_question は apply_selected_rule 側で渡す
        // ここでは x/y 反映のみ
        // 規則により勝利・失格なし
        for (player_id, events) in player_events.iter() {
            let status = player_statuses
                .entry(*player_id)
                .or_insert_with(PlayerStatus::new);

            let corrects = events.iter().filter(|e| matches!(e, Event::Correct)).count() as u32;
            let wrongs = events.iter().filter(|e| matches!(e, Event::Wrong)).count() as u32;

            status.correct_count += corrects;
            status.wrong_count += wrongs;
            if wrongs > 0 {
                status.freeze_count = 1 + 1;
            }

            // x/y 計算は apply_selected_rule直前渡しの current_question が必要
            // SpecialBy用に別途処理します
        }
    }
}

/// ルール選択に基づいて適切なルールを適用する
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
    current_question: u32,
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
        crate::data::RuleOption::RenDatsuNCorrectMWrong => {
            RenDatsuNCorrectMWrong::new(n, m).apply(player_statuses, player_events, question_status);
        }
        crate::data::RuleOption::QuickBoard => {
            QuickBoard.apply(player_statuses, player_events, question_status);
        }
        crate::data::RuleOption::SpecialBy => {
            SpecialBy.apply(player_statuses, player_events, question_status);
            for (player_id, events) in player_events.iter() {
                let status = player_statuses
                    .entry(*player_id)
                    .or_insert_with(PlayerStatus::new);

                if events.iter().any(|e| matches!(e, Event::Correct)) {
                    if (0..=19).contains(&current_question) || (40..=59).contains(&current_question) {
                        status.x += 1;
                    } else if (20..=39).contains(&current_question) {
                        status.y += 1;
                    }
                }
                status.score = status.x as i32 * status.y as i32;
            }
        }
    }
}

