use std::collections::{HashMap, HashSet};
use std::io::{self, Write};

type PlayerId = usize;

#[derive(Clone)]
struct Player {
    id: PlayerId,
    name: String,
}

enum Event {
    Buzz(PlayerId),
    Correct(PlayerId),
    Wrong(PlayerId),
    Pass,
}

struct QuestionState {
    answered: bool,
    locked: HashSet<PlayerId>,
}

impl QuestionState {
    fn new() -> Self {
        Self {
            answered: false,
            locked: HashSet::new(),
        }
    }
}

struct FastestRule {
    point: i32,
}

impl FastestRule {
    fn apply(
        &self,
        state: &mut QuestionState,
        event: Event,
        scores: &mut HashMap<PlayerId, i32>,
    ) {
        match event {
            Event::Correct(p) => {
                if !state.answered {
                    *scores.entry(p).or_insert(0) += self.point;
                    state.answered = true;
                }
            }

            Event::Wrong(p) => {
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
    let players = vec![
        Player { id: 0, name: "A".into() },
        Player { id: 1, name: "B".into() },
        Player { id: 2, name: "C".into() },
    ];

    let mut scores: HashMap<PlayerId, i32> = HashMap::new();
    let rule = FastestRule { point: 1 };

    for q in 1..=5 {
        println!("\n=== Question {} ===", q);

        let mut state = QuestionState::new();

        loop {
            print!("event> ");
            io::stdout().flush().unwrap();

            let input = read_line();

            if input == "pass" {
                break;
            }

            let parts: Vec<&str> = input.split_whitespace().collect();
            if parts.len() != 2 {
                println!("format: buzz A / correct A / wrong A");
                continue;
            }

            let action = parts[0];
            let name = parts[1];

            let player = players.iter().find(|p| p.name == name);

            let player = match player {
                Some(p) => p.id,
                None => {
                    println!("unknown player");
                    continue;
                }
            };

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
                    continue;
                }
            };

            rule.apply(&mut state, event, &mut scores);

            if state.answered {
                println!("Question finished");
                break;
            }
        }

        println!("Scores:");
        for p in &players {
            let s = scores.get(&p.id).unwrap_or(&0);
            println!("{}: {}", p.name, s);
        }
    }
}
