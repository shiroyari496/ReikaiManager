use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use serde::Deserialize;

type PlayerId = usize;

#[derive(Clone)]
struct Player {
    id: PlayerId,
    name: String,
}

#[derive(Deserialize)]
struct PlayerRow {
    name: String,
}

fn load_players(path: &str) -> Vec<Player> {
    let mut rdr = csv::Reader::from_path(path).unwrap();

    // id, name の形式
    rdr.deserialize()
        .enumerate()
        .map(|(i, result)| {
            let record: PlayerRow = result.unwrap();
            Player {
                id: i,
                name: record.name,
            }    
        })    
        .collect()
}

enum Event {
    Buzz(PlayerId),
    Correct(PlayerId),
    Wrong(PlayerId),
    Pass,
    Next,
    EndRound,
}

struct QuestionState {
    finished: bool,
    locked: HashSet<PlayerId>,
}

impl QuestionState {
    fn new() -> Self {
        Self {
            finished: false,
            locked: HashSet::new(),
        }
    }
}

struct FreeBatting;

impl FreeBatting {
    fn apply(
        &self,
        state: &mut QuestionState,
        event: Event,
        correct_count: &mut HashMap<PlayerId, i32>,
        wrong_count: &mut HashMap<PlayerId, i32>,
    ) {
        match event {
            Event::Correct(p) => {
                if !state.finished {
                    *correct_count.entry(p).or_insert(0) += 1;
                    state.finished = true;
                }
            }

            Event::Wrong(p) => {
                *wrong_count.entry(p).or_insert(0) += 1;
                state.locked.insert(p);
            }

            _ => {}
        }
    }
}

fn read_line() -> String {
    let mut s = String::new();
    io::stdin().read_line(&mut s).unwrap();
    s.trim().to_string()
}

fn main() {
    // let players = vec![
    //     Player { id: 0, name: "A".into() },
    //     Player { id: 1, name: "B".into() },
    //     Player { id: 2, name: "C".into() },
    // ];
    let players = load_players("players.csv");

    // let mut scores: HashMap<PlayerId, i32> = HashMap::new();
    let mut correct_count: HashMap<PlayerId, i32> = HashMap::new();
    let mut wrong_count: HashMap<PlayerId, i32> = HashMap::new();
    let rule = FreeBatting;
    let question_num = 1000;

    for q in 1..=question_num {
        println!("\n=== Question {} ===", q);

        let mut state = QuestionState::new();

        loop {
            print!("event> ");
            io::stdout().flush().unwrap();

            let input = read_line();

            if input == "pass" {
                state.finished = true;
                continue;
                // break;
            }

            if input == "next" {
                break;
            }

            let parts: Vec<&str> = input.split_whitespace().collect();
            if parts.len() != 2 {
                println!("format: buzz A / correct A / wrong A / pass / next");
                continue;
            }
            
            let action = parts[0];
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
            let name = players.iter().find(|p| p.id == id).map(|p| p.name.clone()).unwrap_or("unknown".into());
            
            let player = players.iter().find(|p| p.id == id);
            
            let player = match player {
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
            
            if state.finished {
                println!("Question finished");
                continue;
                // break;
            }

            if state.locked.contains(&player) {
                println!("{} already locked out", name);
                continue;
            }
            
            let event = match action {
                "buzz" => Event::Buzz(player),
                "correct" => Event::Correct(player),
                "wrong" => Event::Wrong(player),
                _ => {
                    println!("unknown action");
                    println!("format: buzz A / correct A / wrong A / pass / next");
                    continue;
                }
            };

            rule.apply(&mut state, event, &mut correct_count, &mut wrong_count);
        }

        println!("Scores:");
        for p in &players {
            let correct = correct_count.get(&p.id).unwrap_or(&0);
            let wrong = wrong_count.get(&p.id).unwrap_or(&0);
            println!("{}. {} - Correct: {}, Wrong: {}", p.id, p.name, correct, wrong);
        }
    }
}
