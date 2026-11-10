//! Teaches a position network by playing against itself (temporal-difference learning, as TD-Gammon was taught), and
//! measures it against a plain greedy player or another network. It is not part of the game: the networks it makes
//! are built into the game from assets/long.net and assets/short.net.
//!
//!   train long|short games out.net [in.net|-] [rival.net|-] [rate scale]
//!       teaches for `games` games, from a random network or from in.net, measuring it every 25000 games
//!   train long|short compare a.net b.net|greedy games
//!       plays a.net against b.net, or against the greedy player
//!
//! The game's networks: short nardy 800000 games, then 1000000 more at rate scale 0.3; long nardy 300000 games, then
//! 400000 more at rate scale 0.3 and 400000 at 0.15.

use std::io::Write as _;
use std::process::ExitCode;
use std::time::Instant;

use nardy3d::nardy::{CHECKERS, Move, Position, Roll, Variant, flip, legal_moves, pips, start, win_points};
use nardy3d::network::{Network, Value};
use nardy3d::random::Random;

const USAGE: &str = "train long|short games out.net [in.net|-] [rival.net|-] [rate scale]\ntrain long|short compare a.net b.net|greedy games";

/// Who plays against the network being taught or measured.
#[derive(Clone, Copy)]
enum Opponent<'a> {
    Itself,
    Greedy,
    Rival(&'a Network),
}

fn throw(random: &mut Random) -> Roll {
    Roll { first: random.die(), second: random.die() }
}

/// The value to the mover of a move's result.
fn move_value(net: &Network, played: &Move, variant: Variant) -> Value {
    if played.after.off[0] == CHECKERS {
        let gammon = win_points(&played.after, variant) >= 2;
        return Value { win: 1.0, win_gammon: if gammon { 1.0 } else { 0.0 }, lose_gammon: 0.0 };
    }
    net.evaluate(&flip(&played.after, variant), variant).flipped()
}

/// A plain player: as far forward as it can, hitting and covering in short nardy.
fn greedy(moves: &[Move], variant: Variant, random: &mut Random) -> usize {
    let mut best = (0, f64::MIN);
    for (index, candidate) in moves.iter().enumerate() {
        let after = &candidate.after;
        let mut score = f64::from(-pips(after, 0, variant)) + 25.0 * f64::from(after.off[0]);
        if variant == Variant::Short {
            score += 15.0 * f64::from(after.bar[1]);
            // A lone checker may be hit: the further from home, the more it would lose.
            score -= after.points.iter().zip(0u8..).filter(|(checkers, _)| **checkers == 1).map(|(_, point)| 4.0 + 0.4 * f64::from(point)).sum::<f64>();
        }
        score += f64::from(random.unit()) * 0.01; // to choose among equals
        if score > best.1 {
            best = (index, score);
        }
    }
    best.0
}

/// One game: side 0 plays by the network, side 1 by the opponent. With a rate the network learns from the game as
/// it goes. Returns the points side 0 won, negative for a loss.
fn play(net: &mut Network, variant: Variant, random: &mut Random, rate: Option<f32>, opponent: Opponent) -> i32 {
    let mut position = start(variant);
    let mut first = [true; 2];
    let mut side = random.below(2);
    let mut roll = throw(random);
    // Short nardy opens with the two dice that decided who starts, which differ.
    while variant == Variant::Short && roll.is_double() {
        roll = throw(random);
    }
    for turn in 0..3000 {
        if turn > 0 {
            roll = throw(random);
        }
        let moves = legal_moves(&position, variant, roll, first[side]);
        first[side] = false;
        let (next, target, won): (Position, Value, Option<i32>) = if moves.is_empty() {
            let next = flip(&position, variant);
            (next, net.evaluate(&next, variant).flipped(), None)
        } else {
            let (chosen, target) = match opponent {
                Opponent::Greedy if side == 1 => {
                    let chosen = greedy(&moves, variant, random);
                    (chosen, move_value(net, &moves[chosen], variant))
                }
                _ => {
                    let judge = match opponent {
                        Opponent::Rival(rival) if side == 1 => rival,
                        _ => &*net,
                    };
                    // The first of the moves judged best.
                    let values = moves.iter().map(|candidate| move_value(judge, candidate, variant)).enumerate();
                    values
                        .fold(None, |best: Option<(usize, Value)>, (index, value)| match best {
                            Some((_, known)) if value.equity() <= known.equity() => best,
                            _ => Some((index, value)),
                        })
                        .expect("there are moves to choose from")
                }
            };
            let after = &moves[chosen].after;
            (flip(after, variant), target, (after.off[0] == CHECKERS).then(|| win_points(after, variant)))
        };
        if let Some(rate) = rate {
            net.learn(&position, variant, &target, rate);
        }
        if let Some(points) = won {
            return if side == 0 { points } else { -points };
        }
        position = next;
        side = 1 - side;
    }
    0
}

/// Plays a number of games against an opponent: the games side 0 won and the points it made.
fn measure(net: &mut Network, variant: Variant, seed: u64, games: u64, opponent: Opponent) -> (u64, i64) {
    let mut random = Random::new(seed);
    let (mut wins, mut points) = (0, 0);
    for _ in 0..games {
        let result = play(net, variant, &mut random, None, opponent);
        wins += u64::from(result > 0);
        points += i64::from(result);
    }
    (wins, points)
}

fn load(path: &str) -> Result<Network, String> {
    let data = std::fs::read(path).map_err(|error| format!("could not read {path}: {error}"))?;
    Network::load(&data).ok_or_else(|| format!("{path} is not a network of this game"))
}

fn run(arguments: &[String]) -> Result<(), String> {
    let [game, games, rest @ ..] = arguments else { return Err(USAGE.to_owned()) };
    let variant = if game == "long" { Variant::Long } else { Variant::Short };
    let count = |text: &String| text.parse::<u64>().map_err(|_| format!("{text} is not a number of games\n{USAGE}"));

    if games == "compare" {
        let [ours, theirs, games] = rest else { return Err(USAGE.to_owned()) };
        let games = count(games)?.max(1);
        let mut net = load(ours)?;
        let rival = if theirs == "greedy" { None } else { Some(load(theirs)?) };
        let (wins, points) = measure(&mut net, variant, 2024, games, rival.as_ref().map_or(Opponent::Greedy, Opponent::Rival));
        println!("{ours} against {theirs}: {:.1}% won, {:+.3} points a game", 100.0 * wins as f64 / games as f64, points as f64 / games as f64);
        return Ok(());
    }

    let games = count(games)?;
    let [output, rest @ ..] = rest else { return Err(USAGE.to_owned()) };
    let named = |index: usize| rest.get(index).filter(|path| *path != "-");
    let mut net = match named(0) {
        Some(path) => load(path)?,
        None => Network::random(12345),
    };
    // A network to measure against, instead of the greedy player.
    let rival = named(1).map(|path| load(path)).transpose()?;
    let scale = match rest.get(2) {
        Some(text) => text.parse::<f32>().map_err(|_| format!("{text} is not a rate scale\n{USAGE}"))?,
        None => 1.0,
    };
    let save = |net: &Network| std::fs::write(output, net.save()).map_err(|error| format!("could not write {output}: {error}"));

    let mut random = Random::new(987_654_321);
    let started = Instant::now();
    for game in 1..=games {
        let progress = game as f64 / games as f64;
        // The steps grow smaller as the network settles.
        let rate = scale
            * if progress < 0.3 {
                0.04
            } else if progress < 0.7 {
                0.02
            } else {
                0.008
            };
        play(&mut net, variant, &mut random, Some(rate), Opponent::Itself);
        if game % 25000 == 0 || game == games {
            const MATCHES: u64 = 400;
            let (wins, points) = measure(&mut net, variant, game, MATCHES, rival.as_ref().map_or(Opponent::Greedy, Opponent::Rival));
            let against = if rival.is_some() { "rival" } else { "greedy player" };
            let (won, average) = (100.0 * wins as f64 / MATCHES as f64, points as f64 / MATCHES as f64);
            println!("{game} games, {:.0} s: against the {against} {won:.1}% won, {average:+.3} points a game", started.elapsed().as_secs_f64());
            let _ = std::io::stdout().flush();
            save(&net)?;
        }
    }
    save(&net)
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match run(&arguments) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
