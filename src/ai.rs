//! The computer player. It judges the position each way of playing the roll leaves by the network; at the top level
//! it also weighs every roll the opponent may answer with and their best reply. The lower levels misjudge on purpose,
//! the lowest by much.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;

use crate::nardy::{CHECKERS, Move, Position, Roll, Variant, flip, legal_moves, win_points};
use crate::network::Network;
use crate::random::Random;

/// What the computer is to move in: the position as it sees it, the roll, and who has yet to make a first move.
#[derive(Clone, Copy)]
pub struct Task {
    pub position: Position,
    pub variant: Variant,
    pub roll: Roll,
    pub first_move: bool,
    pub opponent_first: bool,
    pub level: i32,
    pub seed: u64,
}

/// The worth to the mover of where a move leaves the board, in points.
fn judge(net: &Network, played: &Move, variant: Variant) -> f32 {
    if played.after.off[0] == CHECKERS {
        return win_points(&played.after, variant) as f32;
    }
    net.evaluate(&flip(&played.after, variant), variant).flipped().equity()
}

/// The worth to the mover of a move, averaged over the opponent's rolls and their best answer to each.
fn look_ahead(net: &Network, played: &Move, task: &Task, stop: &AtomicBool) -> f32 {
    let variant = task.variant;
    if played.after.off[0] == CHECKERS {
        return win_points(&played.after, variant) as f32;
    }
    let theirs = flip(&played.after, variant);
    let mut total = 0.0;
    for first in 1..=6 {
        for second in first..=6 {
            if stop.load(Ordering::Relaxed) {
                return 0.0;
            }
            let weight = if first == second { 1.0 / 36.0 } else { 2.0 / 36.0 };
            let replies = legal_moves(&theirs, variant, Roll { first, second }, task.opponent_first);
            let best = if replies.is_empty() {
                // They cannot move: the mover is to play again.
                -net.evaluate(&played.after, variant).equity()
            } else {
                replies.iter().map(|reply| judge(net, reply, variant)).fold(f32::MIN, f32::max)
            };
            total -= weight * best;
        }
    }
    total
}

/// The move the computer makes; one without steps when nothing can be played.
pub fn choose(net: &Network, task: &Task, stop: &AtomicBool) -> Move {
    let moves = legal_moves(&task.position, task.variant, task.roll, task.first_move);
    if moves.is_empty() {
        return Move::default();
    }
    let mut random = Random::new(task.seed);
    // The lowest level now and then plays whatever comes.
    if task.level <= 1 && random.below(4) == 0 {
        return moves[random.below(moves.len())];
    }
    // How far off each level's judgement is, in points.
    let noise = match task.level {
        ..=1 => 0.45,
        2 => 0.22,
        3 => 0.08,
        _ => 0.0,
    };
    let mut ranked: Vec<(f32, usize)> = moves
        .iter()
        .enumerate()
        .map(|(index, candidate)| (judge(net, candidate, task.variant) + if noise > 0.0 { noise * random.normal() } else { 0.0 }, index))
        .collect();
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then(b.1.cmp(&a.1)));
    if task.level < 5 || moves.len() == 1 {
        return moves[ranked[0].1];
    }

    // The strongest level looks one roll further at the most promising few.
    let mut best = f32::MIN;
    let mut chosen = ranked[0].1;
    for &(_, index) in ranked.iter().take(8) {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let value = look_ahead(net, &moves[index], task, stop);
        if value > best {
            best = value;
            chosen = index;
        }
    }
    moves[chosen]
}

/// Runs a computation on a thread of its own; the window polls for the result.
pub struct Worker<T> {
    thread: Option<JoinHandle<()>>,
    stop: Arc<AtomicBool>,
    result: Arc<Mutex<Option<T>>>,
}

impl<T> Default for Worker<T> {
    fn default() -> Worker<T> {
        Worker { thread: None, stop: Arc::default(), result: Arc::default() }
    }
}

impl<T> Worker<T> {
    pub fn cancel(&mut self) {
        if let Some(thread) = self.thread.take() {
            self.stop.store(true, Ordering::Relaxed);
            let _ = thread.join();
        }
        self.take();
    }

    pub fn take(&mut self) -> Option<T> {
        self.result.lock().unwrap_or_else(PoisonError::into_inner).take()
    }
}

impl<T: Send + 'static> Worker<T> {
    /// Starts a computation, which is to stop early once the flag it is given is set.
    pub fn start(&mut self, function: impl FnOnce(&AtomicBool) -> T + Send + 'static) {
        self.cancel();
        self.stop = Arc::default();
        let (stop, result) = (self.stop.clone(), self.result.clone());
        self.thread = Some(std::thread::spawn(move || {
            let found = function(&stop);
            if !stop.load(Ordering::Relaxed) {
                *result.lock().unwrap_or_else(PoisonError::into_inner) = Some(found);
            }
        }));
    }
}

impl<T> Drop for Worker<T> {
    fn drop(&mut self) {
        self.cancel();
    }
}
