//! A small neural network that judges positions, taught by playing against itself (temporal-difference learning, as
//! TD-Gammon was): the checkers of both sides on each point, on the bar and borne off, and the pips left, through one
//! hidden layer to the three chances of `Value`.

use crate::nardy::{POINTS, Position, Variant, flip, pips};
use crate::random::Random;

/// What a position is worth to the player to move: the chances of winning, of winning a gammon (марс) and of
/// losing one.
#[derive(Clone, Copy)]
pub struct Value {
    pub win: f32,
    pub win_gammon: f32,
    pub lose_gammon: f32,
}

impl Value {
    /// In points per game.
    pub fn equity(&self) -> f32 {
        2.0 * self.win - 1.0 + self.win_gammon - self.lose_gammon
    }

    /// The same seen by the other player.
    pub fn flipped(&self) -> Value {
        Value { win: 1.0 - self.win, win_gammon: self.lose_gammon, lose_gammon: self.win_gammon }
    }
}

pub const INPUTS: usize = 200;
pub const HIDDEN: usize = 64;
pub const OUTPUTS: usize = 3;

const MAGIC: &[u8; 4] = b"NRD1";
const HEADER: usize = 16;

/// An input that is not zero.
#[derive(Clone, Copy, Default)]
struct Input {
    index: usize,
    value: f32,
}

/// The inputs of a position that are not zero.
struct Encoding {
    inputs: [Input; 112],
    count: usize,
}

impl Encoding {
    fn inputs(&self) -> &[Input] {
        &self.inputs[..self.count]
    }

    fn add(&mut self, index: usize, value: f32) {
        if value != 0.0 {
            self.inputs[self.count] = Input { index, value };
            self.count += 1;
        }
    }
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

pub struct Network {
    hidden_weights: Vec<f32>, // an input's weights to every hidden unit together
    hidden_bias: Vec<f32>,
    output_weights: Vec<f32>, // a hidden unit's weights to every output together
    output_bias: Vec<f32>,
}

impl Network {
    /// A network that knows nothing yet: small random weights, the same for the same seed.
    pub fn random(seed: u64) -> Network {
        let mut random = Random::new(seed);
        let mut small = |count: usize| (0..count).map(|_| random.between(-0.1, 0.1)).collect();
        Network {
            hidden_weights: small(INPUTS * HIDDEN),
            hidden_bias: vec![0.0; HIDDEN],
            output_weights: small(HIDDEN * OUTPUTS),
            output_bias: vec![0.0; OUTPUTS],
        }
    }

    fn parts(&self) -> [&Vec<f32>; 4] {
        [&self.hidden_weights, &self.hidden_bias, &self.output_weights, &self.output_bias]
    }

    /// A network as it was saved: the mark of the format, its shape, and its weights.
    pub fn load(data: &[u8]) -> Option<Network> {
        let mut network = Network {
            hidden_weights: vec![0.0; INPUTS * HIDDEN],
            hidden_bias: vec![0.0; HIDDEN],
            output_weights: vec![0.0; HIDDEN * OUTPUTS],
            output_bias: vec![0.0; OUTPUTS],
        };
        let floats: usize = network.parts().iter().map(|part| part.len()).sum();
        if data.len() != HEADER + floats * size_of::<f32>() || !data.starts_with(MAGIC) {
            return None;
        }
        let shape = data[4..HEADER].chunks_exact(4).map(|bytes| i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
        if !shape.eq([INPUTS as i32, HIDDEN as i32, OUTPUTS as i32]) {
            return None;
        }
        let mut values = data[HEADER..].chunks_exact(4).map(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
        for part in [&mut network.hidden_weights, &mut network.hidden_bias, &mut network.output_weights, &mut network.output_bias] {
            part.iter_mut().zip(&mut values).for_each(|(weight, value)| *weight = value);
        }
        Some(network)
    }

    /// The network in the form `load` reads.
    pub fn save(&self) -> Vec<u8> {
        let mut data = MAGIC.to_vec();
        for size in [INPUTS, HIDDEN, OUTPUTS] {
            data.extend((size as i32).to_le_bytes());
        }
        for part in self.parts() {
            data.extend(part.iter().flat_map(|weight| weight.to_le_bytes()));
        }
        data
    }

    /// The inputs that are not zero: for every point four for each side (one checker, two, three, and the rest), then
    /// the bar, the checkers borne off, the pips left and whether each side may bear off.
    fn encode(position: &Position, variant: Variant) -> Encoding {
        let mut encoding = Encoding { inputs: [Input::default(); 112], count: 0 };
        let flag = |on: bool| if on { 1.0 } else { 0.0 };
        for (point, checkers) in position.points.into_iter().enumerate() {
            let base = point * 8 + if checkers > 0 { 0 } else { 4 };
            let count = checkers.unsigned_abs();
            encoding.add(base, flag(count >= 1));
            encoding.add(base + 1, flag(count >= 2));
            encoding.add(base + 2, flag(count >= 3));
            encoding.add(base + 3, f32::from(count.saturating_sub(3)) * 0.5);
        }
        let start_pips = if variant == Variant::Long { 360.0 } else { 167.0 };
        let flipped = flip(position, variant);
        // Whether a side has every checker home.
        let all_home = |seen: &Position| seen.bar[0] == 0 && seen.points[6..POINTS].iter().all(|checkers| *checkers <= 0);
        encoding.add(192, f32::from(position.bar[0]) * 0.5);
        encoding.add(193, f32::from(position.bar[1]) * 0.5);
        encoding.add(194, f32::from(position.off[0]) / 15.0);
        encoding.add(195, f32::from(position.off[1]) / 15.0);
        encoding.add(196, pips(position, 0, variant) as f32 / start_pips);
        encoding.add(197, pips(position, 1, variant) as f32 / start_pips);
        encoding.add(198, flag(all_home(position)));
        encoding.add(199, flag(all_home(&flipped)));
        encoding
    }

    /// What the hidden units and the outputs make of the inputs.
    fn forward(&self, encoding: &Encoding) -> ([f32; HIDDEN], [f32; OUTPUTS]) {
        let mut hidden = [0.0; HIDDEN];
        hidden.copy_from_slice(&self.hidden_bias);
        for input in encoding.inputs() {
            let weights = &self.hidden_weights[input.index * HIDDEN..][..HIDDEN];
            for (sum, weight) in hidden.iter_mut().zip(weights) {
                *sum += weight * input.value;
            }
        }
        for unit in &mut hidden {
            *unit = sigmoid(*unit);
        }
        let output = std::array::from_fn(|o| {
            let weights = self.output_weights[o..].iter().step_by(OUTPUTS);
            sigmoid(hidden.iter().zip(weights).fold(self.output_bias[o], |sum, (unit, weight)| sum + unit * weight))
        });
        (hidden, output)
    }

    pub fn evaluate(&self, position: &Position, variant: Variant) -> Value {
        let (_, [win, win_gammon, lose_gammon]) = self.forward(&Network::encode(position, variant));
        // A gammon is a win too: its chance cannot be above the win's.
        Value { win, win_gammon: win_gammon.min(win), lose_gammon: lose_gammon.min(1.0 - win) }
    }
}

impl Network {
    /// One step of learning toward `target`, at the given rate.
    pub fn learn(&mut self, position: &Position, variant: Variant, target: &Value, rate: f32) {
        let encoding = Network::encode(position, variant);
        let (hidden, output) = self.forward(&encoding);
        // Cross-entropy on each sigmoid output: its error is simply target minus output.
        let error = [target.win - output[0], target.win_gammon - output[1], target.lose_gammon - output[2]];
        let mut back = [0.0; HIDDEN];
        for (j, weights) in self.output_weights.chunks_exact_mut(OUTPUTS).enumerate() {
            let mut sum = 0.0;
            for (weight, error) in weights.iter_mut().zip(error) {
                sum += *weight * error;
                *weight += rate * error * hidden[j];
            }
            back[j] = sum * hidden[j] * (1.0 - hidden[j]);
        }
        for (bias, error) in self.output_bias.iter_mut().zip(error) {
            *bias += rate * error;
        }
        for input in encoding.inputs() {
            let weights = &mut self.hidden_weights[input.index * HIDDEN..][..HIDDEN];
            for (weight, back) in weights.iter_mut().zip(back) {
                *weight += rate * back * input.value;
            }
        }
        for (bias, back) in self.hidden_bias.iter_mut().zip(back) {
            *bias += rate * back;
        }
    }
}
