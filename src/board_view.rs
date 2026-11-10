//! How the checkers and the dice are shown: every checker in its place, flying in an arc when it moves, lifted when
//! chosen or dragged; and the dice thrown onto the board, tumbling and coming to rest on the numbers rolled.

use crate::board_model as board;
use crate::math::{Mat4, PI, Vec3, Vec4, lerp, smooth_step, vec3, vec4};
use crate::nardy::{POINTS, Position};
use crate::random::Random;
use crate::renderer::{PieceDraw, PieceKind};

// A place for checkers: a point (as White counts them), the bar, or the tray.
pub const BAR_PLACE: usize = 24;
pub const OFF_PLACE: usize = 25;
pub const PLACES: usize = 26;

/// What was heard since the last call: checkers landing, knocking a lone one off or dropping into the tray, and
/// the dice striking the board, each as loud as it strikes.
#[derive(Default)]
pub struct Sounds {
    pub landings: usize,
    pub hits: usize,
    pub trays: usize,
    pub dice: Vec<f32>,
}

#[derive(Clone, Copy)]
struct Checker {
    side: usize,
    place: usize,
    // The flight.
    from: Vec3,
    to: Vec3,
    time: f32,
    duration: f32,
    delay: f32,
    lift: f32,       // raised when chosen
    stand_from: f32, // 1 if it starts its flight standing in the tray
    hit_on_arrival: bool,
}

impl Checker {
    /// At rest in a place.
    fn at(side: usize, place: usize, spot: Vec3) -> Checker {
        let stand_from = if place == OFF_PLACE { 1.0 } else { 0.0 };
        Checker { side, place, from: spot, to: spot, time: 1.0, duration: 1.0, delay: 0.0, lift: 0.0, stand_from, hit_on_arrival: false }
    }
}

#[derive(Clone, Copy)]
struct Die {
    side: usize,
    value: i8,
    from: Vec3,
    to: Vec3,
    rest: Vec4, // the quaternion it comes to rest in
    spin_axis: Vec3,
    spin: f32,
    time: f32,
    duration: f32,
    dim: f32,
    dim_target: f32,
}

impl Die {
    fn rolling(&self) -> bool {
        self.time < self.duration
    }
}

// Quaternions as (x, y, z, w).
fn axis_angle(axis: Vec3, angle: f32) -> Vec4 {
    let (s, c) = (angle / 2.0).sin_cos();
    vec4(axis.x * s, axis.y * s, axis.z * s, c)
}

fn multiply(a: Vec4, b: Vec4) -> Vec4 {
    vec4(
        a.w * b.x + a.x * b.w + a.y * b.z - a.z * b.y,
        a.w * b.y - a.x * b.z + a.y * b.w + a.z * b.x,
        a.w * b.z + a.x * b.y - a.y * b.x + a.z * b.w,
        a.w * b.w - a.x * b.x - a.y * b.y - a.z * b.z,
    )
}

fn rotation(q: Vec4) -> Mat4 {
    let Vec4 { x, y, z, w } = q;
    let mut m = Mat4::IDENTITY;
    m.set(0, 0, 1.0 - 2.0 * (y * y + z * z));
    m.set(0, 1, 2.0 * (x * y - w * z));
    m.set(0, 2, 2.0 * (x * z + w * y));
    m.set(1, 0, 2.0 * (x * y + w * z));
    m.set(1, 1, 1.0 - 2.0 * (x * x + z * z));
    m.set(1, 2, 2.0 * (y * z - w * x));
    m.set(2, 0, 2.0 * (x * z - w * y));
    m.set(2, 1, 2.0 * (y * z + w * x));
    m.set(2, 2, 1.0 - 2.0 * (x * x + y * y));
    m
}

/// The turn that brings the face with `value` to the top. The die's faces: 1 on +y, 2 on +z, 3 on +x, and the
/// opposite faces adding up to seven.
fn face_up(value: i8) -> Vec4 {
    let h = 0.5f32.sqrt();
    match value {
        2 => vec4(-h, 0.0, 0.0, h),
        3 => vec4(0.0, 0.0, h, h),
        4 => vec4(0.0, 0.0, -h, h),
        5 => vec4(h, 0.0, 0.0, h),
        6 => vec4(1.0, 0.0, 0.0, 0.0),
        _ => vec4(0.0, 0.0, 0.0, 1.0),
    }
}

/// The stages of a die's flight where it strikes the board, and how loud.
const CONTACTS: [(f32, f32); 3] = [(0.182, 1.0), (0.545, 0.55), (0.909, 0.3)];

fn spot(side: usize, place: usize, index: usize) -> Vec3 {
    match place {
        BAR_PLACE => board::bar_spot(side, index),
        OFF_PLACE => board::off_spot(side, index),
        point => board::checker_spot(point, index),
    }
}

/// A die leaving the hand at `from`, to tumble to rest with `value` up.
fn thrown_die(side: usize, index: usize, value: i8, from: Vec3, random: &mut Random) -> Die {
    let mut unit = || random.between(-1.0, 1.0);
    Die {
        side,
        value,
        from,
        to: board::die_spot(side, index) + vec3(unit() * 0.008, 0.0, unit() * 0.006),
        rest: multiply(axis_angle(Vec3::Y, unit() * 0.5), face_up(value)),
        spin_axis: (vec3(unit(), unit() * 0.4, unit()) + vec3(0.3, 0.0, 0.3)).normalized(),
        spin: 2.0 * PI * (2.2 + unit() * 0.6),
        time: 0.0,
        duration: 0.95 + 0.12 * unit(),
        dim: 0.0,
        dim_target: 0.0,
    }
}

pub struct BoardView {
    checkers: Vec<Checker>,
    stacks: [[Vec<usize>; PLACES]; 2], // the checkers at each place, bottom first
    dice: [Option<Die>; 2],
    lifted: Option<(usize, usize)>, // the side and the place of the checker raised
    carried: Option<Vec3>,
    sounds: Sounds,
}

impl BoardView {
    pub fn new() -> BoardView {
        BoardView { checkers: Vec::new(), stacks: Default::default(), dice: [None; 2], lifted: None, carried: None, sounds: Sounds::default() }
    }

    /// Every checker in its place at once; `board` is seen from White's side.
    pub fn reset(&mut self, board: &Position) {
        self.checkers.clear();
        self.stacks = Default::default();
        let mut add = |side: usize, place: usize, count: i8| {
            for _ in 0..count {
                let stack = &mut self.stacks[side][place];
                stack.push(self.checkers.len());
                self.checkers.push(Checker::at(side, place, spot(side, place, stack.len() - 1)));
            }
        };
        for (point, checkers) in board.points.into_iter().enumerate() {
            add(usize::from(checkers < 0), point, checkers.abs());
        }
        for side in 0..2 {
            add(side, BAR_PLACE, board.bar[side]);
            add(side, OFF_PLACE, board.off[side]);
        }
        self.lifted = None;
        self.carried = None;
    }

    /// Whether a checker is the top one of the place raised.
    fn is_lifted(&self, id: usize) -> bool {
        let checker = &self.checkers[id];
        self.lifted == Some((checker.side, checker.place)) && self.stacks[checker.side][checker.place].last() == Some(&id)
    }

    /// The top checker of `side` at `from` goes to `to`, after `delay` seconds; a lone checker hit there goes to the bar.
    pub fn move_checker(&mut self, side: usize, from: usize, to: usize, delay: f32) {
        let Some(id) = self.stacks[side][from].last().copied() else { return };
        let was_lifted = self.lifted == Some((side, from));
        // It starts from where it is drawn: in the pointer's hold, or in flight, or at rest.
        let start = self.carried.filter(|_| was_lifted).unwrap_or_else(|| self.top_of(side, from));
        self.stacks[side][from].pop();
        let end = spot(side, to, self.stacks[side][to].len());
        self.stacks[side][to].push(id);
        let duration = 0.3 + 0.35 * ((end - start).length() / 0.5).min(1.0);
        let stand_from = if from == OFF_PLACE { 1.0 } else { 0.0 };
        self.checkers[id] = Checker { side, place: to, from: start, to: end, time: 0.0, duration, delay, lift: 0.0, stand_from, hit_on_arrival: false };
        if was_lifted {
            self.lifted = None;
        }
        self.carried = None;

        // A lone checker of the other side there is knocked to the bar as this one arrives.
        let other = 1 - side;
        if to < POINTS && !self.stacks[other][to].is_empty() {
            let knocked_from = self.top_of(other, to);
            if let Some(hit) = self.stacks[other][to].pop() {
                let bar = &mut self.stacks[other][BAR_PLACE];
                bar.push(hit);
                let rest = spot(other, BAR_PLACE, bar.len() - 1);
                self.checkers[hit] =
                    Checker { from: knocked_from, time: 0.0, duration: 0.45, delay: delay + duration * 0.85, ..Checker::at(other, BAR_PLACE, rest) };
                self.checkers[id].hit_on_arrival = true;
            }
        }
    }

    /// A checker raised over its place, as chosen: the top one of `side` there. `None` lets it down.
    pub fn lift(&mut self, checker: Option<(usize, usize)>) {
        self.lifted = checker;
    }

    /// The checker raised is carried by the pointer, or let go.
    pub fn carry(&mut self, position: Option<Vec3>) {
        self.carried = position;
    }

    /// Two dice thrown by `side`: from the thrower's hand, at their side of the board, into their right half.
    pub fn throw(&mut self, side: usize, first: i8, second: i8, random: &mut Random) {
        let (x, z, further) = if side == 0 { (0.16, 0.32, 0.02) } else { (-0.16, -0.32, -0.02) };
        let one = thrown_die(side, 0, first, vec3(x - 0.02, 0.16, z), random);
        let mut other = thrown_die(side, 1, second, vec3(x + 0.03, 0.18, z + further), random);
        other.duration += 0.06;
        self.dice = [Some(one), Some(other)];
    }

    /// A die each for the two players at the start.
    pub fn throw_opening(&mut self, white: i8, black: i8, random: &mut Random) {
        let one = thrown_die(0, 0, white, vec3(0.16, 0.16, 0.32), random);
        let other = thrown_die(1, 0, black, vec3(-0.16, 0.16, -0.32), random);
        self.dice = [Some(one), Some(other)];
    }

    /// Two dice of `side` lying still on the numbers given, as a turn taken back left them.
    pub fn show_dice(&mut self, side: usize, first: i8, second: i8) {
        self.throw(side, first, second, &mut Random::new((i32::from(first) * 7 + i32::from(second)) as u64));
        for die in self.dice.iter_mut().flatten() {
            die.time = die.duration;
        }
    }

    pub fn clear_dice(&mut self) {
        self.dice = [None; 2];
    }

    /// How many of the roll's moves are played: the dice darken as they are used. Two different dice darken one by
    /// one, the one played first given by its place in the roll; a double's pair after two moves each.
    pub fn use_dice(&mut self, used: usize, doubles: bool, first_used: usize) {
        let spent = if doubles { [used >= 2, used >= 4] } else { [used >= 2 || (used == 1 && first_used == 0), used >= 2 || (used == 1 && first_used == 1)] };
        for (die, spent) in self.dice.iter_mut().zip(spent) {
            if let Some(die) = die {
                die.dim_target = if spent { 1.0 } else { 0.0 };
            }
        }
    }

    pub fn update(&mut self, seconds: f32) {
        for id in 0..self.checkers.len() {
            let lifted = self.is_lifted(id);
            let checker = &mut self.checkers[id];
            if checker.time < 1.0 {
                if checker.delay > 0.0 {
                    checker.delay -= seconds;
                } else {
                    checker.time = (checker.time + seconds / checker.duration).min(1.0);
                    if checker.time >= 1.0 {
                        if checker.hit_on_arrival {
                            self.sounds.hits += 1;
                        } else if checker.place == OFF_PLACE {
                            self.sounds.trays += 1;
                        } else {
                            self.sounds.landings += 1;
                        }
                    }
                }
            }
            let target = if lifted { 1.0 } else { 0.0 };
            checker.lift += (target - checker.lift) * (1.0 - (-seconds * 18.0).exp());
        }
        for die in self.dice.iter_mut().flatten() {
            if die.rolling() {
                let before = die.time / die.duration;
                die.time = (die.time + seconds).min(die.duration);
                let after = die.time / die.duration;
                self.sounds.dice.extend(CONTACTS.iter().filter(|(stage, _)| before < *stage && after >= *stage).map(|(_, volume)| volume));
            }
            die.dim += (die.dim_target - die.dim) * (1.0 - (-seconds * 10.0).exp());
        }
    }

    pub fn animating(&self) -> bool {
        let checker_moves = |id: usize| {
            let target = if self.is_lifted(id) { 1.0 } else { 0.0 };
            self.checkers[id].time < 1.0 || (target - self.checkers[id].lift).abs() > 0.002
        };
        (0..self.checkers.len()).any(checker_moves) || self.dice.iter().flatten().any(|die| die.rolling() || (die.dim - die.dim_target).abs() > 0.002)
    }

    pub fn dice_rolling(&self) -> bool {
        self.dice.iter().flatten().any(Die::rolling)
    }

    pub fn take_sounds(&mut self) -> Sounds {
        std::mem::take(&mut self.sounds)
    }

    /// Where a checker is drawn now and how it is turned.
    fn checker_model(&self, id: usize) -> Mat4 {
        // In flight: along an arc that rises with the distance; standing on edge in the tray.
        let checker = &self.checkers[id];
        let s = smooth_step(0.0, 1.0, checker.time);
        let distance = (checker.to - checker.from).length();
        let arc = (0.025 + 0.12 * distance).min(0.12);
        let mut position = checker.from.lerp(checker.to, s) + vec3(0.0, arc * 4.0 * s * (1.0 - s) + checker.lift * 0.014, 0.0);
        // Only the top checker of the place is carried.
        if let Some(carried) = self.carried.filter(|_| checker.time >= 1.0 && self.is_lifted(id)) {
            position = carried;
        }
        let stand = lerp(checker.stand_from, if checker.place == OFF_PLACE { 1.0 } else { 0.0 }, s);
        Mat4::translation(position) * Mat4::rotation_x(stand * PI / 2.0) * Mat4::scale(board::CHECKER_RADIUS)
    }

    /// Where the top checker of a place is drawn now, in flight or not.
    fn top_of(&self, side: usize, place: usize) -> Vec3 {
        match self.stacks[side][place].last() {
            Some(id) => {
                let model = self.checker_model(*id);
                vec3(model.at(0, 3), model.at(1, 3), model.at(2, 3))
            }
            None => spot(side, place, 0),
        }
    }

    /// Adds the checkers and the dice to draw; the top checkers of `glow_side` glow as `glow` says for their places.
    pub fn add_pieces(&self, pieces: &mut Vec<PieceDraw>, glow: &[f32; PLACES], glow_side: Option<usize>) {
        for (side, places) in self.stacks.iter().enumerate() {
            for (place, stack) in places.iter().enumerate() {
                for (height, id) in stack.iter().copied().enumerate() {
                    let checker = &self.checkers[id];
                    let top = height + 1 == stack.len();
                    let glowing = top && checker.time >= 1.0 && glow_side == Some(side);
                    let highlight = if glowing { glow[place].max(checker.lift) } else { checker.lift };
                    pieces.push(PieceDraw { model: self.checker_model(id), kind: PieceKind::Checker, side, value: 0, highlight, dim: 0.0 });
                }
            }
        }
        for die in self.dice.iter().flatten() {
            let s = die.time / die.duration;
            // Fast spin at first, slowing as it settles on the face rolled.
            let spin_left = (1.0 - s).powi(3);
            let turn = rotation(multiply(die.rest, axis_angle(die.spin_axis, die.spin * spin_left)));
            let extent = board::DIE_SIZE * (turn.at(1, 0).abs() + turn.at(1, 1).abs() + turn.at(1, 2).abs());
            let travel = 1.0 - (1.0 - s).powf(2.2);
            let bounce = die.from.y * (1.0 - s) * (1.0 - s) * (s * PI * 2.75).cos().abs();
            let mut position = die.from.lerp(die.to, travel);
            position.y = (extent * 0.92).max(board::DIE_SIZE) + bounce;
            let model = Mat4::translation(position) * turn * Mat4::scale(board::DIE_SIZE);
            pieces.push(PieceDraw { model, kind: PieceKind::Die, side: die.side, value: die.value, highlight: 0.0, dim: die.dim });
        }
    }
}
