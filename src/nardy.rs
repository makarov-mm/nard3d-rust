//! The rules of the two games of nardy. In long nardy both sides run the same way round the board, a single checker
//! holds a point, only one checker a turn may leave the head, and no six points in a row may be held while they shut
//! the opponent in. In short nardy (backgammon) the sides run against each other, and a lone checker is hit to the
//! bar and must enter again.

use std::cell::RefCell;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Variant {
    Long,
    Short,
}

pub const POINTS: usize = 24;
pub const CHECKERS: i8 = 15;
/// Where long nardy starts all fifteen.
pub const HEAD: i8 = 23;
/// A step entering from the bar.
pub const FROM_BAR: i8 = 24;
/// A step bearing a checker off.
pub const BEAR_OFF: i8 = -1;

/// A position as the player to move sees it: points[i] above zero are their checkers, below zero the opponent's. The
/// player to move brings their checkers from point 23 down to point 0, their home is points 0 to 5, and they bear off
/// below point 0. Points are numbered for people from 1, as the mover sees them.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct Position {
    pub points: [i8; POINTS],
    pub bar: [i8; 2], // the mover's, then the opponent's: short nardy only
    pub off: [i8; 2], // borne off
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct Step {
    pub from: i8, // a point, or `FROM_BAR`
    pub to: i8,   // a point, or `BEAR_OFF`
    pub die: i8,
    pub hit: bool,
}

/// A whole turn: up to four steps, and where they leave the board, still from the mover's side.
#[derive(Clone, Copy, Default)]
pub struct Move {
    steps: [Step; 4],
    count: usize,
    pub after: Position,
}

impl Move {
    pub fn steps(&self) -> &[Step] {
        &self.steps[..self.count]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Roll {
    pub first: i8,
    pub second: i8,
}

impl Roll {
    pub fn is_double(self) -> bool {
        self.first == self.second
    }

    /// The dice in the order they are played: two, or four of a double.
    pub fn dice(self) -> Vec<i8> {
        if self.is_double() { vec![self.first; 4] } else { vec![self.first, self.second] }
    }
}

pub fn start(variant: Variant) -> Position {
    let mut position = Position::default();
    let stacks: &[(usize, i8)] = match variant {
        Variant::Long => &[(HEAD as usize, CHECKERS), (11, -CHECKERS)],
        Variant::Short => &[(23, 2), (12, 5), (7, 3), (5, 5), (0, -2), (11, -5), (16, -3), (18, -5)],
    };
    for &(point, checkers) in stacks {
        position.points[point] = checkers;
    }
    position
}

/// The same position seen by the other player.
pub fn flip(position: &Position, variant: Variant) -> Position {
    // Short nardy is a mirror; in long nardy the opponent's start lies half way round from the mover's.
    let seen_from = |point: usize| match variant {
        Variant::Short => POINTS - 1 - point,
        Variant::Long => (point + 12) % POINTS,
    };
    Position {
        points: std::array::from_fn(|point| -position.points[seen_from(point)]),
        bar: [position.bar[1], position.bar[0]],
        off: [position.off[1], position.off[0]],
    }
}

pub fn apply(position: &Position, step: Step) -> Position {
    let mut after = *position;
    if step.from == FROM_BAR {
        after.bar[0] -= 1;
    } else {
        after.points[step.from as usize] -= 1;
    }
    if step.to == BEAR_OFF {
        after.off[0] += 1;
    } else {
        let there = &mut after.points[step.to as usize];
        if *there < 0 {
            // Only a lone checker can be there to hit.
            *there = 0;
            after.bar[1] += 1;
        }
        *there += 1;
    }
    after
}

/// The pips the mover (side 0) or the opponent (side 1) still has to travel.
pub fn pips(position: &Position, side: usize, variant: Variant) -> i32 {
    let seen = if side == 0 { *position } else { flip(position, variant) };
    let on_points: i32 = seen.points.iter().zip(1..).filter(|(checkers, _)| **checkers > 0).map(|(checkers, distance)| i32::from(*checkers) * distance).sum();
    i32::from(seen.bar[0]) * 25 + on_points
}

/// Long nardy: the mover holds no six points in a row unless an opponent's checker has already passed them.
pub fn block_allowed(position: &Position) -> bool {
    // The points in the order the opponent passes them: their last point is the mover's 12, their head the mover's 11.
    let at = |order: usize| position.points[(order + 12) % POINTS];
    let first_opponent = (0..POINTS).find(|order| at(*order) < 0).unwrap_or(POINTS);
    let mut run = 0;
    for order in 0..POINTS {
        run = if at(order) > 0 { run + 1 } else { 0 };
        // Six in a row from order - run + 1: allowed only with an opponent's checker already past its start.
        if run >= 6 && first_opponent + run > order {
            return false;
        }
    }
    true
}

/// The ways to play a roll, each final position once, with one order of steps that reaches it. As many dice as
/// possible must be played, and the higher one when only one can be. `first_move` lets two checkers leave the head of
/// long nardy on a first roll of 6-6, 4-4 or 3-3. Empty when nothing can be played.
pub fn legal_moves(position: &Position, variant: Variant, roll: Roll, first_move: bool) -> Vec<Move> {
    collect(position, variant, roll, first_move, true)
}

/// Every order of steps that plays the roll legally, for a player choosing step by step.
pub fn legal_sequences(position: &Position, variant: Variant, roll: Roll, first_move: bool) -> Vec<Move> {
    collect(position, variant, roll, first_move, false)
}

/// After the mover has borne off their last checker: 1, 2 for a gammon (марс), or 3 for a backgammon.
pub fn win_points(position: &Position, variant: Variant) -> i32 {
    if position.off[1] > 0 {
        1
    } else if variant == Variant::Long {
        2
    } else if position.bar[1] > 0 || position.points[..6].iter().any(|checkers| *checkers < 0) {
        // A backgammon: the loser still has a checker on the bar or in the winner's home.
        3
    } else {
        2
    }
}

/// Keys met in one search, cleared at once by moving on to a new epoch.
struct SeenSet {
    keys: Vec<u64>,
    epochs: Vec<u32>,
    epoch: u32,
}

impl SeenSet {
    const SIZE: usize = 1 << 14;

    fn new() -> SeenSet {
        SeenSet { keys: vec![0; SeenSet::SIZE], epochs: vec![0; SeenSet::SIZE], epoch: 1 }
    }

    fn clear(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        if self.epoch == 0 {
            self.epochs.fill(0);
            self.epoch = 1;
        }
    }

    /// False when the key was already there. A crowded table lets keys through rather than look further.
    fn insert(&mut self, key: u64) -> bool {
        let mut slot = (key ^ (key >> 29)) as usize & (SeenSet::SIZE - 1);
        for _ in 0..32 {
            if self.epochs[slot] != self.epoch {
                self.epochs[slot] = self.epoch;
                self.keys[slot] = key;
                return true;
            }
            if self.keys[slot] == key {
                return false;
            }
            slot = (slot + 1) & (SeenSet::SIZE - 1);
        }
        true
    }
}

thread_local! {
    static SEEN: RefCell<SeenSet> = RefCell::new(SeenSet::new());
}

fn hash(position: &Position, extra: u32) -> u64 {
    let values = position.points.iter().chain(&position.bar).chain(&position.off);
    values.fold(1_469_598_103_934_665_603 ^ u64::from(extra), |hash, value| (hash ^ u64::from(*value as u8)).wrapping_mul(1_099_511_628_211))
}

/// The single steps the mover can make with one die: one at most for each of the checkers.
#[derive(Clone, Copy, Default)]
struct Steps {
    list: [Step; CHECKERS as usize],
    count: usize,
}

impl Steps {
    fn add(&mut self, from: i8, to: i8, die: i8, hit: bool) {
        self.list[self.count] = Step { from, to, die, hit };
        self.count += 1;
    }
}

fn steps_for(position: &Position, variant: Variant, die: i8) -> Steps {
    let mut steps = Steps::default();
    // What stands on a point lets a checker land: nothing or the mover's own, and in short nardy a lone opponent.
    let open = |checkers: i8| if variant == Variant::Long { checkers >= 0 } else { checkers >= -1 };
    if variant == Variant::Short && position.bar[0] > 0 {
        // Checkers on the bar enter first, into the opponent's home.
        let to = FROM_BAR - die;
        let there = position.points[to as usize];
        if open(there) {
            steps.add(FROM_BAR, to, die, there == -1);
        }
        return steps;
    }
    let Some(highest) = position.points.iter().rposition(|checkers| *checkers > 0) else { return steps };
    let home = highest < 6;
    for from in (0..=highest as i8).rev().filter(|from| position.points[*from as usize] > 0) {
        let to = from - die;
        if to >= 0 {
            let there = position.points[to as usize];
            if open(there) {
                steps.add(from, to, die, there == -1);
            }
        } else if home && (to == BEAR_OFF || from == highest as i8) {
            // Off exactly, or from the farthest checker with a die larger than it needs.
            steps.add(from, BEAR_OFF, die, false);
        }
    }
    steps
}

struct Search<'a> {
    variant: Variant,
    distinct: bool, // each position once, for the computer; every order, for a person
    head_limit: u32,
    nodes: Vec<Move>,
    seen: &'a mut SeenSet,
}

/// What tells apart the turns that the same position may go on to: the dice left and the checkers gone from the head.
fn key(dice_left: &[i8], head_moves: u32) -> u32 {
    let (low, high) = dice_left.iter().fold((7, 0), |(low, high), die| (low.min(*die as u32), high.max(*die as u32)));
    dice_left.len() as u32 | low << 3 | high << 6 | head_moves << 9
}

impl Search<'_> {
    /// Every way of playing the dice still left, each prefix kept as a candidate.
    fn explore(&mut self, position: &Position, dice: &mut [i8; 4], left: usize, current: &mut Move, head_moves: u32) {
        current.after = *position;
        self.nodes.push(*current);
        for k in 0..left {
            let die = dice[k];
            if dice[..k].contains(&die) {
                continue; // tried already
            }
            let steps = steps_for(position, self.variant, die);
            if steps.count == 0 {
                continue;
            }
            // The die taken out of the ones left: the others stay in front.
            dice.swap(k, left - 1);
            for step in &steps.list[..steps.count] {
                let head_moves = head_moves + u32::from(self.variant == Variant::Long && step.from == HEAD);
                if head_moves > self.head_limit {
                    continue;
                }
                let next = apply(position, *step);
                if self.distinct && !self.seen.insert(hash(&next, key(&dice[..left - 1], head_moves))) {
                    continue;
                }
                current.steps[current.count] = *step;
                current.count += 1;
                self.explore(&next, dice, left - 1, current, head_moves);
                current.count -= 1;
            }
            dice.swap(k, left - 1);
        }
    }
}

fn collect(position: &Position, variant: Variant, roll: Roll, first_move: bool, distinct: bool) -> Vec<Move> {
    SEEN.with_borrow_mut(|seen| {
        seen.clear();
        let head_limit = match variant {
            Variant::Short => 4,
            Variant::Long if first_move && roll.is_double() && matches!(roll.first, 6 | 4 | 3) => 2,
            Variant::Long => 1,
        };
        let mut search = Search { variant, distinct, head_limit, nodes: Vec::with_capacity(256), seen };
        let mut dice = [roll.first, roll.second, roll.first, roll.first];
        search.explore(position, &mut dice, if roll.is_double() { 4 } else { 2 }, &mut Move::default(), 0);
        let Search { nodes, seen, .. } = search;

        // As many dice as can be played, by moves that keep to the block rule.
        let valid = |candidate: &Move| variant == Variant::Short || block_allowed(&candidate.after);
        let longest = nodes.iter().filter(|candidate| valid(candidate)).map(|candidate| candidate.count).max().unwrap_or(0);
        if longest == 0 {
            return Vec::new();
        }
        let mut moves: Vec<Move> = nodes.into_iter().filter(|candidate| candidate.count == longest && valid(candidate)).collect();
        // When only one die can be played, the higher one if it can.
        if longest == 1 && !roll.is_double() {
            let higher = roll.first.max(roll.second);
            if moves.iter().any(|candidate| candidate.steps[0].die == higher) {
                moves.retain(|candidate| candidate.steps[0].die == higher);
            }
        }
        if distinct {
            seen.clear();
            moves.retain(|candidate| seen.insert(hash(&candidate.after, 0xFFFF)));
        }
        moves
    })
}
