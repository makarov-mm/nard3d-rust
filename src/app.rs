//! The game's window: it runs the games, the computer players and the interface, and draws them.

mod widgets;
mod window;

use std::sync::Arc;
use std::time::Instant;

use winit::event_loop::ActiveEventLoop;
use winit::keyboard::ModifiersState;

use crate::Result;
use crate::ai::{self, Task, Worker};
use crate::board_model as board;
use crate::board_view::{BAR_PLACE, BoardView, OFF_PLACE, PLACES};
use crate::camera::{BoardCamera, CameraMode};
use crate::gfx;
use crate::math::{Vec3, Vec4, vec2, vec3};
use crate::nardy::{self, BEAR_OFF, CHECKERS, FROM_BAR, HEAD, Move, POINTS, Position, Roll, Step, Variant};
use crate::network::Network;
use crate::platform::GlWindow;
use crate::random::Random;
use crate::renderer::{FrostedPanel, Renderer, Scene};
use crate::settings::{Settings, WindowPlacement};
use crate::sound::{Effect, SoundBank};
use crate::strings::{Language, Str, fill, tr};
use crate::text::FontLibrary;
use crate::themes::{THEME_COUNT, themes};
use crate::ui::{self, FontId, Painter, Rect, rect, rgb, rgba};

pub use window::run;

use widgets::{MUTED_COLOR, TEXT_COLOR, Toast};
use window::{Key, Pointer};

// The window's size where none is remembered, and its smallest, on a screen of the usual scale.
const DEFAULT_SIZE: (f64, f64) = (1500.0, 900.0);
const MIN_SIZE: (f64, f64) = (960.0, 680.0);

const MIN_THINK: f32 = 0.5; // seconds the computer takes over a move, at least
const STEP_PAUSE: f32 = 0.45; // between the computer's steps
const TURN_PAUSE: f32 = 0.55; // after the last step, before the turn passes
const PASS_PAUSE: f32 = 1.6; // when nothing can be played
const DRAG_START: f32 = 6.0; // pixels the pointer travels before a press becomes a drag

// The widgets; a selector takes two numbers, one for each of its arrows.
const HELP_BUTTON: i32 = 1;
const ROLL_BUTTON: i32 = 2;
const HINT_BUTTON: i32 = 3;
const UNDO_BUTTON: i32 = 4;
const NEW_GAME_BUTTON: i32 = 5;
const AUTO_ROLL_BUTTON: i32 = 6;
const CAMERA_BUTTON: i32 = 7;
const SOUND_BUTTON: i32 = 8;
const LANGUAGE_BUTTON: i32 = 9;
const GAME_SELECTOR: i32 = 10;
const MODE_SELECTOR: i32 = 12;
const LEVEL_SELECTOR: i32 = 14;
const THEME_SELECTOR: i32 = 16;
const HELP_CLOSE: i32 = 101;

const NOTE_COLOR: ui::Color = rgb(255, 200, 130);

const HELP_ROWS: usize = 11; // in the longer of the two columns
const HELP_WIDTH: f32 = 900.0;
const HELP_ROW: f32 = 26.0;
const HELP_PAD: f32 = 28.0;

/// The keys of the help, each with what it does: a column of the mouse and the turn, a column of the game and
/// the settings.
const HELP_LINES: [(Option<Str>, &str, Str); 2 * HELP_ROWS - 1] = [
    (Some(Str::KeyClick), "", Str::HelpClick),
    (Some(Str::KeyDrag), "", Str::HelpDrag),
    (Some(Str::KeyDoubleClick), "", Str::HelpDoubleClick),
    (Some(Str::KeyRightClick), "", Str::HelpTakeBack),
    (Some(Str::KeyRightDrag), "", Str::HelpRotate),
    (Some(Str::KeyWheel), "", Str::HelpZoom),
    (None, "Space  Enter", Str::HelpRoll),
    (None, "H", Str::HelpHint),
    (None, "Ctrl+Z", Str::HelpUndo),
    (None, "A", Str::HelpAutoRoll),
    (None, "C  Home", Str::HelpCamera),
    (None, "N", Str::HelpNew),
    (None, "G", Str::HelpGame),
    (None, "M", Str::HelpMode),
    (None, "1 – 5", Str::HelpLevel),
    (None, "T", Str::HelpTheme),
    (None, "V", Str::HelpSound),
    (None, "L", Str::HelpLanguage),
    (None, "Tab", Str::HelpPanel),
    (None, "F11  Alt+Enter", Str::HelpFullScreen),
    (None, "Esc  F1", Str::HelpClose),
];

/// Who plays whom.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    VsComputer,
    TwoPlayers,
    Demo,
}

impl Mode {
    const ALL: [Mode; 3] = [Mode::VsComputer, Mode::TwoPlayers, Mode::Demo];

    /// The mode a number of steps on, round and round.
    fn stepped(self, steps: i32) -> Mode {
        Mode::ALL[(self as i32 + steps).rem_euclid(3) as usize]
    }

    fn name(self) -> Str {
        match self {
            Mode::VsComputer => Str::ModeComputer,
            Mode::TwoPlayers => Str::ModeTwoPlayers,
            Mode::Demo => Str::ModeDemo,
        }
    }
}

/// The players throw a die each to see who starts; then each turn the dice are thrown and fly, and the checkers
/// are moved: by hand, or by the computer, which thinks and then plays its steps one by one. Then the game is over.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Opening,
    ToRoll,
    Rolling,
    Moving,
    Thinking,
    Playing,
    Over,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Gesture {
    None,
    Press,
    Drag,
    TurnCamera,
}

/// A human's turn as it began, with its dice: what taking back a turn returns to.
#[derive(Clone, Copy)]
struct Snapshot {
    board: Position,
    to_play: usize,
    first_move: [bool; 2],
    roll: Roll,
    history: usize,
    turn: u32,
}

/// What a line of the history tells of.
#[derive(Clone, Copy, PartialEq, Eq)]
enum HistoryKind {
    Turn(usize), // White's or Black's
    Note,
    End, // of the game
}

impl HistoryKind {
    /// As the settings keep it: the side of a turn, or a number of its own.
    fn digit(self) -> usize {
        match self {
            HistoryKind::Turn(side) => side,
            HistoryKind::Note => 2,
            HistoryKind::End => 3,
        }
    }

    fn from_digit(digit: char) -> Option<HistoryKind> {
        match digit {
            '0' => Some(HistoryKind::Turn(0)),
            '1' => Some(HistoryKind::Turn(1)),
            '2' => Some(HistoryKind::Note),
            '3' => Some(HistoryKind::End),
            _ => None,
        }
    }
}

struct HistoryLine {
    text: String,
    kind: HistoryKind,
}

/// A way for the checker chosen to reach a place: the steps, one checker's, in order.
#[derive(Clone)]
struct Route {
    to: i8, // in the mover's numbers: a point or `BEAR_OFF`
    steps: Vec<Step>,
}

/// What is shown on a point of the board; the shader reads each from a component of the point's vector.
#[derive(Clone, Copy)]
enum Mark {
    Target,  // a place to move to
    Movable, // a checker that may move
    LastMove,
    Hint,
}

fn mark(points: &mut [Vec4; POINTS], place: usize, mark: Mark, strength: f32) {
    // The bar and the tray are no points.
    let Some(point) = points.get_mut(place) else { return };
    let shown = match mark {
        Mark::Target => &mut point.x,
        Mark::Movable => &mut point.y,
        Mark::LastMove => &mut point.z,
        Mark::Hint => &mut point.w,
    };
    *shown = shown.max(strength);
}

/// A die's face with its pips, darkened once played.
fn draw_die(painter: &mut Painter, x: f32, y: f32, size: f32, value: i8, dim: f32) {
    // Where the pips may be, in thirds of the face, and which of them make each number.
    const SPOTS: [(f32, f32); 9] = [(-1.0, -1.0), (0.0, -1.0), (1.0, -1.0), (-1.0, 0.0), (0.0, 0.0), (1.0, 0.0), (-1.0, 1.0), (0.0, 1.0), (1.0, 1.0)];
    const LAYOUTS: [&[usize]; 6] = [&[4], &[0, 8], &[0, 4, 8], &[0, 2, 6, 8], &[0, 2, 4, 6, 8], &[0, 2, 3, 5, 6, 8]];
    let face = rect(x - size / 2.0, y - size / 2.0, size, size);
    painter.shadow(face, size * 0.15, rgba(0, 0, 0, (70.0 * (1.0 - dim * 0.6)) as u8));
    painter.fill(face, size * 0.2, rgb(240, 236, 226).mix(rgb(90, 92, 100), dim * 0.75));
    let pip = rgb(24, 22, 22).mix(rgb(60, 62, 70), dim);
    for &spot in LAYOUTS[(value.clamp(1, 6) - 1) as usize] {
        let (across, down) = SPOTS[spot];
        painter.disc(x + across * size * 0.27, y + down * size * 0.27, size * 0.095, pip);
    }
}

pub struct App {
    renderer: Renderer,
    painter: Painter,
    fonts: FontLibrary,
    sound: SoundBank,
    settings: Settings,
    language: Language,
    width: i32,
    height: i32,
    scale: f32,      // of the interface: 1 at 96 dots per inch
    font_scale: f32, // the scale the fonts were made for
    dirty: bool,     // something to draw since the last frame
    idle: bool,      // waiting for input: the time waited is not animation time
    full_screen: bool,
    maximized: bool,
    placement: Option<WindowPlacement>,
    last_frame: Instant,
    time: f32,

    // The game.
    nets: [Arc<Network>; 2], // long and short nardy
    variant: Variant,
    board: Position, // as White sees it
    to_play: usize,
    first_move: [bool; 2],
    roll: Roll,
    rolled: bool,
    opening: [i8; 2], // the dice thrown to see who starts
    turn_start: Position,
    played: Vec<Step>,     // this turn's steps so far, in the mover's numbers
    sequences: Vec<Move>,  // every legal way to play this turn's roll
    last_move: Vec<usize>, // the places the last turn moved from and to, as the board's view counts them
    score: [i32; 2],
    winner: Option<usize>,
    win_points: i32,
    stage: Stage,
    stage_clock: f32,
    done_clock: f32, // counts down after the last step of a turn, before it ends
    mode: Mode,
    level: i32,
    theme: usize,
    auto_roll: bool,
    turn: u32,
    random: Random,
    undo: Vec<Snapshot>,
    history: Vec<HistoryLine>,
    view: BoardView,

    // The computer.
    computer: Worker<Move>,
    computer_move: Option<Move>,
    computer_step: usize,
    step_clock: f32,
    hint_worker: Worker<Move>,
    hint_wanted: bool,
    hint: Option<Move>,

    // The view and the input.
    camera: BoardCamera,
    pointer: Pointer,
    modifiers: ModifiersState,
    gesture: Gesture,
    press_x: f32,
    press_y: f32,
    // The pointer when the camera last turned.
    drag_x: f32,
    drag_y: f32,
    press_right: bool,
    press_double: bool,
    press_place: Option<usize>,
    toggled_place: Option<usize>, // where the last click chose or let go of a checker, for a double click
    selected: Option<usize>,      // the place chosen to move from, as the board's view counts places
    hover_place: Option<usize>,
    cursor_hand: bool,
    shown_cursor_hand: bool,

    // The interface.
    active_widget: i32,
    panel_visible: bool,
    panel_slide: f32,
    help_visible: bool,
    history_scroll: f32,
    follow_history: bool,
    banner_visible: bool,
    banner_fade: f32,
    // Kept while the banner fades.
    banner_title: String,
    banner_detail: String,
    toasts: Vec<Toast>,

    // The window goes last: what was made in its OpenGL context is dropped before it.
    gl: GlWindow,
}

impl App {
    fn new(event_loop: &ActiveEventLoop) -> Result<App> {
        let settings = Settings::load();
        let language = settings.language;
        let gl = window::open(event_loop, tr(Str::Title, language), settings.window, settings.maximized)?;
        gfx::set_glsl_version(gl.version());

        let mut painter = Painter::new()?;
        let mut fonts = FontLibrary::new();
        let scale = gl.window.scale_factor() as f32;
        painter.set_fonts(widgets::build_fonts(&mut fonts, scale)?);
        gl.window.set_visible(true);
        let size = gl.window.inner_size();
        widgets::show_loading(&gl, &mut painter, tr(Str::Title, language));

        let theme = settings.theme as usize;
        let renderer = Renderer::new(&themes()[theme])?;
        let mut camera = BoardCamera::new();
        camera.set_mode(if settings.camera == 1 { CameraMode::Top } else { CameraMode::Overview });
        // The computer's judgement of positions, one network for each game, built into the program.
        let network = |data: &[u8]| Network::load(data).map(Arc::new).ok_or("The computer player's network built into the program is damaged.");
        let nets = [network(include_bytes!("../assets/long.net"))?, network(include_bytes!("../assets/short.net"))?];
        let mut sound = SoundBank::new();
        sound.enabled = settings.sound;

        let mut app = App {
            renderer,
            painter,
            fonts,
            sound,
            language,
            width: size.width as i32,
            height: size.height as i32,
            scale,
            font_scale: scale,
            dirty: true,
            idle: false,
            full_screen: false,
            maximized: settings.maximized,
            placement: settings.window,
            last_frame: Instant::now(),
            time: 0.0,
            nets,
            variant: if settings.game == 1 { Variant::Short } else { Variant::Long },
            board: Position::default(),
            to_play: 0,
            first_move: [true; 2],
            roll: Roll { first: 1, second: 1 },
            rolled: false,
            opening: [1; 2],
            turn_start: Position::default(),
            played: Vec::new(),
            sequences: Vec::new(),
            last_move: Vec::new(),
            score: settings.score,
            winner: None,
            win_points: 0,
            stage: Stage::Opening,
            stage_clock: 0.0,
            done_clock: -1.0,
            mode: Mode::ALL[settings.mode as usize],
            level: settings.level,
            theme,
            auto_roll: settings.auto_roll,
            turn: 0,
            random: Random::from_entropy(),
            undo: Vec::new(),
            history: Vec::new(),
            view: BoardView::new(),
            computer: Worker::default(),
            computer_move: None,
            computer_step: 0,
            step_clock: 0.0,
            hint_worker: Worker::default(),
            hint_wanted: false,
            hint: None,
            camera,
            pointer: Pointer::default(),
            modifiers: ModifiersState::empty(),
            gesture: Gesture::None,
            press_x: 0.0,
            press_y: 0.0,
            drag_x: 0.0,
            drag_y: 0.0,
            press_right: false,
            press_double: false,
            press_place: None,
            toggled_place: None,
            selected: None,
            hover_place: None,
            cursor_hand: false,
            shown_cursor_hand: false,
            active_widget: 0,
            panel_visible: settings.panel,
            panel_slide: if settings.panel { 1.0 } else { 0.0 },
            help_visible: false,
            history_scroll: 0.0,
            follow_history: true,
            banner_visible: false,
            banner_fade: 0.0,
            banner_title: String::new(),
            banner_detail: String::new(),
            toasts: Vec::new(),
            settings,
            gl,
        };
        app.track_placement();

        // The game left unfinished, with its history as far as it was kept: it comes first, so that taking a turn
        // back does not take the history with it.
        let (game, history) = (std::mem::take(&mut app.settings.game_state), std::mem::take(&mut app.settings.history));
        for line in history.split('|') {
            let mut characters = line.chars();
            if let Some(kind) = characters.next().and_then(HistoryKind::from_digit) {
                if !characters.as_str().is_empty() {
                    app.history.push(HistoryLine { text: characters.as_str().to_owned(), kind });
                }
            }
        }
        if !app.load_game(&game) {
            app.new_game();
        }
        Ok(app)
    }

    fn frame(&mut self) -> Result<()> {
        let Some(seconds) = self.begin_frame()? else { return Ok(()) };
        self.renderer.resize(self.width, self.height)?;

        self.update(seconds);
        let scene = self.build_scene();
        self.renderer.render(&scene);

        self.cursor_hand = false;
        self.painter.begin(self.width, self.height);
        self.draw_interface();
        self.painter.flush();
        self.handle_board_pointer();

        self.end_frame();
        Ok(())
    }

    fn update(&mut self, seconds: f32) {
        let space = self.slide_panel(seconds);
        self.camera.set_space(space);

        self.stage_clock += seconds;
        match self.stage {
            Stage::Opening => {
                if !self.view.dice_rolling() && self.stage_clock > 1.6 {
                    self.finish_opening();
                }
            }
            Stage::ToRoll => {
                let human = self.is_human(self.to_play);
                if (!human && self.stage_clock > 0.45) || (human && self.auto_roll && self.stage_clock > 0.3) {
                    self.roll_dice();
                }
            }
            Stage::Rolling => {
                if !self.view.dice_rolling() {
                    self.begin_moves();
                }
            }
            Stage::Moving => {
                if self.done_clock > 0.0 {
                    self.done_clock -= seconds;
                    if self.done_clock <= 0.0 {
                        self.end_turn();
                    }
                }
            }
            Stage::Thinking | Stage::Playing => self.update_computer(seconds),
            Stage::Over => {
                if self.mode == Mode::Demo && self.stage_clock > 6.0 {
                    self.new_game();
                }
            }
        }
        self.update_hint();
        self.view.update(seconds);
        self.play_sounds();
        self.camera.update(seconds);

        self.age_toasts(seconds);
        let banner_target = if self.banner_visible { 1.0 } else { 0.0 };
        self.banner_fade += (banner_target - self.banner_fade) * (1.0 - (-seconds * 7.0).exp());
        if (banner_target - self.banner_fade).abs() < 0.002 {
            self.banner_fade = banner_target;
        }
    }

    /// What the board was heard to do: a few of each kind at most, no two quite alike.
    fn play_sounds(&mut self) {
        let sounds = self.view.take_sounds();
        let mut play = |effect, volume, pan| {
            let pitch = self.random.between(0.93, 1.07);
            self.sound.play_at(effect, volume, pan, pitch);
        };
        for _ in 0..sounds.landings.min(3) {
            play(Effect::Checker, 0.8, 0.0);
        }
        for _ in 0..sounds.hits.min(2) {
            play(Effect::Hit, 0.9, 0.0);
        }
        for _ in 0..sounds.trays.min(3) {
            play(Effect::Tray, 0.8, 0.4);
        }
        for volume in sounds.dice {
            play(Effect::Die, volume, 0.0);
        }
    }

    fn needs_animation(&self) -> bool {
        let waiting = match self.stage {
            Stage::Opening | Stage::Rolling | Stage::Thinking | Stage::Playing => true,
            Stage::ToRoll => !self.is_human(self.to_play) || self.auto_roll,
            Stage::Moving => self.done_clock > 0.0,
            Stage::Over => self.mode == Mode::Demo,
        };
        // The places a checker may go pulse, and so does the hint.
        let pulsing = self.human_to_move() && (self.selected.is_some() || self.hint.is_some());
        let banner_target = if self.banner_visible { 1.0 } else { 0.0 };
        let panel_target = if self.panel_visible { 1.0 } else { 0.0 };
        waiting
            || pulsing
            || self.hint_wanted
            || self.camera.moving()
            || self.view.animating()
            || !self.toasts.is_empty()
            || self.banner_fade != banner_target
            || (self.panel_slide - panel_target).abs() > 0.001
    }

    fn build_scene(&self) -> Scene {
        let mut points = [Vec4::default(); POINTS];
        let mut trays = Vec4::default();
        // The last turn's steps, until the next player moves.
        if self.played.is_empty() || self.stage == Stage::Over {
            for place in &self.last_move {
                mark(&mut points, *place, Mark::LastMove, 1.0);
            }
        }

        // The checkers that may move glow, the one under the pointer more; once one is chosen, the places it may go.
        let mut glow = [0.0f32; PLACES];
        let mut glow_side = None;
        if self.human_to_move() && self.done_clock <= 0.0 {
            glow_side = Some(self.to_play);
            for step in self.next_steps() {
                let place = self.to_place(step.from);
                glow[place] = glow[place].max(if self.selected.is_none() { 0.32 } else { 0.16 });
                if self.selected.is_none() {
                    mark(&mut points, place, Mark::Movable, 1.0);
                }
            }
            if let Some(hover) = self.hover_place.filter(|hover| glow[*hover] > 0.0) {
                glow[hover] = 0.7;
            }
            if let Some(selected) = self.selected {
                for route in self.routes_from(self.point_at(selected)) {
                    let place = self.to_place(route.to);
                    let strength = if self.hover_place == Some(place) { 1.6 } else { 1.0 };
                    if place != OFF_PLACE {
                        mark(&mut points, place, Mark::Target, strength);
                    } else if self.to_play == 0 {
                        trays.x = strength;
                    } else {
                        trays.y = strength;
                    }
                }
            }
        }
        if let Some(hint) = self.hint.as_ref().filter(|_| self.human_to_move()) {
            for step in hint.steps() {
                mark(&mut points, self.to_place(step.from), Mark::Hint, 1.0);
                if step.to == BEAR_OFF {
                    trays.z = if self.to_play == 0 { 1.0 } else { -1.0 };
                } else {
                    mark(&mut points, self.to_place(step.to), Mark::Hint, 1.0);
                }
            }
        }

        let mut pieces = Vec::new();
        self.view.add_pieces(&mut pieces, &glow, glow_side);
        Scene {
            view: self.camera.view(),
            projection: self.camera.projection(self.width as f32 / self.height.max(1) as f32),
            eye: self.camera.eye(),
            time: self.time,
            pieces,
            points,
            trays,
            panels: self.frosted_panels(),
        }
    }

    // The game ---------------------------------------------------------------------------------------

    fn net(&self) -> Arc<Network> {
        self.nets[self.variant as usize].clone()
    }

    fn is_human(&self, side: usize) -> bool {
        match self.mode {
            Mode::VsComputer => side == 0,
            Mode::TwoPlayers => true,
            Mode::Demo => false,
        }
    }

    fn human_to_move(&self) -> bool {
        self.stage == Stage::Moving && self.is_human(self.to_play)
    }

    /// The board as the player to move sees it.
    fn mover_view(&self) -> Position {
        if self.to_play == 0 { self.board } else { nardy::flip(&self.board, self.variant) }
    }

    fn set_from_mover(&mut self, mover: &Position) {
        self.board = if self.to_play == 0 { *mover } else { nardy::flip(mover, self.variant) };
    }

    /// A place in the mover's numbers (a point, `FROM_BAR` or `BEAR_OFF`) as the board's view counts places.
    fn to_place(&self, point: i8) -> usize {
        match (point, self.to_play, self.variant) {
            (FROM_BAR, ..) => BAR_PLACE,
            (BEAR_OFF, ..) => OFF_PLACE,
            (point, 0, _) => point as usize,
            // Black counts the points from the other end: short nardy the opposite way round, long nardy from the far side.
            (point, _, Variant::Short) => POINTS - 1 - point as usize,
            (point, _, Variant::Long) => (point as usize + 12) % POINTS,
        }
    }

    /// A place of the board's view in the mover's numbers.
    fn point_at(&self, place: usize) -> i8 {
        match place {
            BAR_PLACE => FROM_BAR,
            OFF_PLACE => BEAR_OFF,
            point => self.to_place(point as i8) as i8, // the same reckoning, both ways
        }
    }

    /// A game in progress as text: the game, who is to play, the first moves still to make, the dice if thrown, and
    /// the board as White sees it when the turn began. Empty when there is nothing to take up later.
    fn save_game(&self) -> String {
        if matches!(self.stage, Stage::Opening | Stage::Over) {
            return String::new();
        }
        let board = if self.rolled { &self.turn_start } else { &self.board };
        let turn = [self.variant as i32, self.to_play as i32, i32::from(self.first_move[0]), i32::from(self.first_move[1]), i32::from(self.rolled)];
        let dice = [self.roll.first, self.roll.second];
        let checkers = dice.iter().chain(&board.points).chain(&board.bar).chain(&board.off).map(|count| i32::from(*count));
        turn.into_iter().chain(checkers).map(|value| value.to_string()).collect::<Vec<_>>().join(" ")
    }

    /// Takes up a game kept as text; false when the text holds no game of the kind chosen that can be played on.
    fn load_game(&mut self, text: &str) -> bool {
        let Ok(values) = text.split_ascii_whitespace().map(str::parse::<i32>).collect::<std::result::Result<Vec<_>, _>>() else { return false };
        let die = |value: i32| (1..=6).contains(&value);
        if values.len() != 35 || values[0] != self.variant as i32 || !(0..=1).contains(&values[1]) || !die(values[5]) || !die(values[6]) {
            return false;
        }
        let all = i32::from(CHECKERS);
        let mut board = Position::default();
        let mut count = [0; 2];
        for (point, &checkers) in board.points.iter_mut().zip(&values[7..]) {
            if checkers.abs() > all {
                return false;
            }
            *point = checkers as i8;
            count[usize::from(checkers < 0)] += checkers.abs();
        }
        for side in 0..2 {
            let (bar, off) = (values[31 + side], values[33 + side]);
            if !(0..=all).contains(&bar) || !(0..all).contains(&off) || (bar > 0 && self.variant == Variant::Long) {
                return false;
            }
            board.bar[side] = bar as i8;
            board.off[side] = off as i8;
            count[side] += bar + off;
        }
        if count != [all; 2] {
            return false;
        }

        self.board = board;
        self.to_play = values[1] as usize;
        self.first_move = [values[2] != 0, values[3] != 0];
        self.roll = Roll { first: values[5] as i8, second: values[6] as i8 };
        self.winner = None;
        self.view.reset(&self.board);
        self.view.clear_dice();
        self.turn = 1;
        if values[4] != 0 {
            self.rolled = true;
            self.turn_start = self.board;
            self.view.show_dice(self.to_play, self.roll.first, self.roll.second);
            self.begin_moves();
        } else {
            self.start_turn();
        }
        true
    }

    fn new_game(&mut self) {
        self.stop_computer();
        self.board = nardy::start(self.variant);
        self.first_move = [true; 2];
        self.rolled = false;
        self.played.clear();
        self.sequences.clear();
        self.last_move.clear();
        self.winner = None;
        self.win_points = 0;
        self.turn = 0;
        self.done_clock = -1.0;
        self.undo.clear();
        self.history.clear();
        self.history_scroll = 0.0;
        self.follow_history = true;
        self.banner_visible = false;
        self.select(None);
        self.view.reset(&self.board);
        self.view.clear_dice();
        self.sound.play(Effect::Setup);
        self.throw_opening();
        self.save();
    }

    fn throw_opening(&mut self) {
        self.opening = [self.random.die(), self.random.die()];
        self.view.throw_opening(self.opening[0], self.opening[1], &mut self.random);
        self.sound.play(Effect::Rattle);
        self.stage = Stage::Opening;
        self.stage_clock = 0.0;
    }

    /// The dice of the opening have settled: the higher starts, or both throw again.
    fn finish_opening(&mut self) {
        let [white, black] = self.opening;
        if white == black {
            self.show_toast(fill(self.t(Str::OpeningTie), &[&white, &black]));
            self.throw_opening();
            return;
        }
        self.to_play = usize::from(white < black);
        let starts = self.pick(self.to_play, Str::StartsYou, Str::StartsComputer, Str::StartsWhite, Str::StartsBlack);
        self.show_toast(starts);
        self.add_history(format!("{starts} ({white}:{black})"), HistoryKind::Note);
        if self.variant == Variant::Short {
            // In short nardy the opening dice are the first roll.
            self.turn += 1;
            self.roll = Roll { first: white, second: black };
            self.rolled = true;
            self.turn_start = self.board;
            self.begin_moves();
        } else {
            self.start_turn();
        }
        self.save();
    }

    fn start_turn(&mut self) {
        self.turn += 1;
        self.stage = Stage::ToRoll;
        self.stage_clock = 0.0;
        self.rolled = false;
        self.played.clear();
        self.sequences.clear();
        self.done_clock = -1.0;
        self.hint = None;
        self.hint_wanted = false;
        self.select(None);
    }

    fn roll_dice(&mut self) {
        if self.stage != Stage::ToRoll {
            return;
        }
        self.roll = Roll { first: self.random.die(), second: self.random.die() };
        self.rolled = true;
        self.turn_start = self.board;
        self.view.throw(self.to_play, self.roll.first, self.roll.second, &mut self.random);
        self.sound.play(Effect::Rattle);
        self.stage = Stage::Rolling;
        self.stage_clock = 0.0;
    }

    /// What the computer is asked to move in, at a level of play.
    fn task(&mut self, level: i32) -> Task {
        Task {
            position: self.mover_view(),
            variant: self.variant,
            roll: self.roll,
            first_move: self.first_move[self.to_play],
            opponent_first: self.first_move[1 - self.to_play],
            level,
            seed: self.random.next_u64(),
        }
    }

    /// The dice lie still: every way to play them is found, and the player to move takes over.
    fn begin_moves(&mut self) {
        self.turn_start = self.board;
        self.played.clear();
        self.sequences = nardy::legal_sequences(&self.mover_view(), self.variant, self.roll, self.first_move[self.to_play]);
        self.done_clock = -1.0;
        self.hint = None;
        self.hint_wanted = false;
        self.hint_worker.cancel();
        self.select(None);
        self.view.use_dice(0, self.roll.is_double(), 0);
        self.stage_clock = 0.0;
        if self.sequences.is_empty() {
            // Nothing can be played: the turn passes.
            self.stage = Stage::Moving;
            self.done_clock = PASS_PAUSE;
            self.show_toast(self.t(Str::NoMovesToast));
            self.sound.play(Effect::NoMove);
            return;
        }
        if self.is_human(self.to_play) {
            if self.undo.last().is_some_and(|last| last.turn >= self.turn) {
                self.undo.pop();
            }
            let history = self.history.len();
            self.undo.push(Snapshot { board: self.board, to_play: self.to_play, first_move: self.first_move, roll: self.roll, history, turn: self.turn });
            if self.undo.len() > 300 {
                self.undo.remove(0);
            }
            self.stage = Stage::Moving;
            return;
        }
        self.stage = Stage::Thinking;
        self.computer_move = None;
        let (net, task) = (self.net(), self.task(self.level));
        self.computer.start(move |stop| ai::choose(&net, &task, stop));
    }

    fn play_step(&mut self, step: Step) {
        let after = nardy::apply(&self.mover_view(), step);
        self.view.move_checker(self.to_play, self.to_place(step.from), self.to_place(step.to), 0.0);
        self.set_from_mover(&after);
        if self.played.is_empty() {
            self.last_move.clear();
        }
        self.played.push(step);
        self.view.use_dice(self.played.len(), self.roll.is_double(), usize::from(self.played[0].die != self.roll.first));
        self.hint = None;
        self.hint_wanted = false;
        self.hint_worker.cancel();
        self.select(None);
        if self.is_human(self.to_play) && self.next_steps().is_empty() {
            self.done_clock = TURN_PAUSE;
        }
    }

    fn take_back_step(&mut self) {
        if !self.human_to_move() {
            return;
        }
        let Some(step) = self.played.pop() else { return };
        let start = if self.to_play == 0 { self.turn_start } else { nardy::flip(&self.turn_start, self.variant) };
        let mover = self.played.iter().fold(start, |position, played| nardy::apply(&position, *played));
        self.set_from_mover(&mover);
        // The checker flies back, and one it knocked off returns from the bar.
        self.view.move_checker(self.to_play, self.to_place(step.to), self.to_place(step.from), 0.0);
        if step.hit {
            self.view.move_checker(1 - self.to_play, BAR_PLACE, self.to_place(step.to), 0.0);
        }
        let first_used = usize::from(self.played.first().is_some_and(|first| first.die != self.roll.first));
        self.view.use_dice(self.played.len(), self.roll.is_double(), first_used);
        self.done_clock = -1.0;
        self.select(None);
    }

    fn take_back_turn(&mut self) {
        while self.human_to_move() && !self.played.is_empty() {
            self.take_back_step();
        }
    }

    /// Takes back this turn's steps, or else goes back to the start of the last turn a person played, with its dice.
    fn undo(&mut self) {
        if self.mode == Mode::Demo {
            return;
        }
        if self.human_to_move() && !self.played.is_empty() {
            self.take_back_turn();
            return;
        }
        while self.stage != Stage::Over && self.undo.last().is_some_and(|last| last.turn >= self.turn) {
            self.undo.pop();
        }
        let Some(snapshot) = self.undo.last().copied() else {
            self.show_toast(self.t(Str::NothingToUndo));
            return;
        };
        self.stop_computer();
        if let (Stage::Over, Some(winner)) = (self.stage, self.winner) {
            self.score[winner] -= self.win_points;
            self.banner_visible = false;
        }
        self.winner = None;
        self.board = snapshot.board;
        self.to_play = snapshot.to_play;
        self.first_move = snapshot.first_move;
        self.roll = snapshot.roll;
        self.turn = snapshot.turn;
        self.history.truncate(snapshot.history);
        self.last_move.clear();
        self.rolled = true;
        self.view.reset(&self.board);
        self.view.show_dice(self.to_play, self.roll.first, self.roll.second);
        self.begin_moves();
        self.save();
    }

    fn end_turn(&mut self) {
        self.done_clock = -1.0;
        let side = self.to_play;
        let (high, low) = (self.roll.first.max(self.roll.second), self.roll.first.min(self.roll.second));
        let moves = if self.played.is_empty() { self.t(Str::NoMoves).to_owned() } else { self.move_text(&self.played) };
        self.add_history(format!("{} {high}-{low}: {moves}", self.name(side)), HistoryKind::Turn(side));
        // Where each checker moved from and to: a checker going on with the next step marks only its start and its end.
        let mut marked = Vec::new();
        for (i, step) in self.played.iter().enumerate() {
            if i == 0 || step.from != self.played[i - 1].to {
                marked.push(self.to_place(step.from));
            }
            if self.played.get(i + 1).is_none_or(|next| next.from != step.to) {
                marked.push(self.to_place(step.to));
            }
        }
        self.last_move = marked;
        self.first_move[side] = false;
        if self.board.off[side] == CHECKERS {
            self.finish_game();
            return;
        }
        self.to_play = 1 - side;
        self.start_turn();
        self.save();
    }

    fn finish_game(&mut self) {
        let side = self.to_play;
        self.winner = Some(side);
        self.win_points = nardy::win_points(&self.mover_view(), self.variant);
        self.score[side] += self.win_points;
        self.stage = Stage::Over;
        self.stage_clock = 0.0;
        let title = self.pick(side, Str::WinYou, Str::WinComputer, Str::WinWhite, Str::WinBlack);
        let points = self.t(match self.win_points {
            3.. => Str::PointsKoks,
            2 => Str::PointsMars,
            _ => Str::PointsOne,
        });
        let score = fill(self.t(Str::Score), &[&self.score[0], &self.score[1]]);
        self.banner_title = title.to_owned();
        self.banner_detail = format!("{points}  ·  {score}");
        self.banner_visible = true;
        self.add_history(format!("{title} — {points}"), HistoryKind::End);
        self.sound.play(Effect::Win);
        self.save();
    }

    fn stop_computer(&mut self) {
        self.computer.cancel();
        self.computer_move = None;
        self.hint_worker.cancel();
        self.hint_wanted = false;
    }

    fn update_computer(&mut self, seconds: f32) {
        if self.stage == Stage::Thinking {
            if self.computer_move.is_none() {
                self.computer_move = self.computer.take();
            }
            if self.computer_move.is_some() && self.stage_clock >= MIN_THINK {
                self.stage = Stage::Playing;
                self.stage_clock = 0.0;
                self.computer_step = 0;
                self.step_clock = 0.1;
            }
            return;
        }
        self.step_clock -= seconds;
        let Some(chosen) = self.computer_move.filter(|_| self.step_clock <= 0.0) else { return };
        match chosen.steps().get(self.computer_step) {
            Some(step) => {
                self.play_step(*step);
                self.computer_step += 1;
                self.step_clock = if self.computer_step < chosen.steps().len() { STEP_PAUSE } else { TURN_PAUSE + 0.2 };
            }
            None => self.end_turn(),
        }
    }

    /// The computer's advice for the whole roll, at its best: any steps already played are taken back first.
    fn request_hint(&mut self) {
        if !self.human_to_move() || self.sequences.is_empty() || self.done_clock > 0.0 {
            return;
        }
        if let Some(advised) = self.hint {
            // Asked again: the move is played.
            for step in advised.steps() {
                self.play_step(*step);
            }
            return;
        }
        self.take_back_turn();
        self.hint_wanted = true;
        let (net, task) = (self.net(), self.task(5));
        self.hint_worker.start(move |stop| ai::choose(&net, &task, stop));
    }

    fn update_hint(&mut self) {
        if !self.hint_wanted {
            return;
        }
        let Some(advised) = self.hint_worker.take() else { return };
        self.hint_wanted = false;
        if !self.human_to_move() || !self.played.is_empty() {
            return;
        }
        self.hint = Some(advised);
        self.show_toast(fill(self.t(Str::HintText), &[&self.move_text(advised.steps())]));
    }

    /// The steps that may be played next.
    fn next_steps(&self) -> Vec<Step> {
        let mut steps = Vec::new();
        for sequence in self.sequences.iter().filter(|sequence| sequence.steps().starts_with(&self.played)) {
            if let Some(next) = sequence.steps().get(self.played.len()) {
                if !steps.contains(next) {
                    steps.push(*next);
                }
            }
        }
        steps
    }

    /// Where the checker on `point` may go this turn, by one step or by several of its own; a shorter way is
    /// preferred, and among equals one that hits on the way.
    fn routes_from(&self, point: i8) -> Vec<Route> {
        let hits = |steps: &[Step]| steps.iter().filter(|step| step.hit).count();
        let played = self.played.len();
        let mut routes: Vec<Route> = Vec::new();
        for sequence in &self.sequences {
            let steps = sequence.steps();
            if !steps.starts_with(&self.played) || steps.get(played).is_none_or(|next| next.from != point) {
                continue;
            }
            let mut chain: Vec<Step> = Vec::new();
            for step in &steps[played..] {
                if chain.last().is_some_and(|last| step.from != last.to || last.to == BEAR_OFF) {
                    break;
                }
                chain.push(*step);
                match routes.iter_mut().find(|route| route.to == step.to) {
                    None => routes.push(Route { to: step.to, steps: chain.clone() }),
                    Some(known) => {
                        if chain.len() < known.steps.len() || (chain.len() == known.steps.len() && hits(&chain) > hits(&known.steps)) {
                            known.steps.clone_from(&chain);
                        }
                    }
                }
            }
        }
        routes
    }

    fn play_route(&mut self, route: &Route) {
        for step in &route.steps {
            self.play_step(*step);
        }
    }

    /// The checker on `point` goes by the higher die it may use, else the lower.
    fn play_larger_die(&mut self, point: i8) {
        if let Some(best) = self.next_steps().into_iter().filter(|step| step.from == point).max_by_key(|step| step.die) {
            self.play_step(best);
        }
    }

    /// Chooses the place to move from, or none: its top checker is raised.
    fn select(&mut self, place: Option<usize>) {
        self.selected = place;
        self.view.lift(place.map(|place| (self.to_play, place)));
    }

    /// Why a checker of the player to move may not go now, in words.
    fn why_not(&self, point: i8) -> &'static str {
        let short = self.variant == Variant::Short;
        if short && self.mover_view().bar[0] > 0 && point != FROM_BAR {
            self.t(Str::EnterFirst)
        } else if !short && point == HEAD && self.played.iter().any(|step| step.from == HEAD) {
            self.t(Str::HeadRule)
        } else {
            self.t(Str::CannotMove)
        }
    }

    /// The other of the two games, begun anew.
    fn switch_game(&mut self) {
        self.variant = match self.variant {
            Variant::Long => Variant::Short,
            Variant::Short => Variant::Long,
        };
        self.score = [0; 2];
        self.new_game();
    }

    fn set_mode(&mut self, mode: Mode) {
        if mode == self.mode {
            return;
        }
        let was_human = self.is_human(self.to_play);
        self.mode = mode;
        self.score = [0; 2];
        // The player to move may have changed hands: their turn starts again for the new player.
        let in_turn = matches!(self.stage, Stage::Moving | Stage::Thinking | Stage::Playing) && self.done_clock <= 0.0;
        if was_human != self.is_human(self.to_play) && in_turn {
            self.stop_computer();
            self.board = self.turn_start;
            self.view.reset(&self.board);
            self.begin_moves();
        }
        self.save();
    }

    fn set_level(&mut self, level: i32) {
        self.level = level.clamp(1, 5);
        self.save();
    }

    fn set_theme(&mut self, theme: i32) {
        self.theme = theme.rem_euclid(THEME_COUNT as i32) as usize;
        self.renderer.set_theme(self.theme());
        self.save();
    }

    /// The other of the two views.
    fn switch_camera(&mut self) {
        self.camera.set_mode(match self.camera.mode() {
            CameraMode::Overview => CameraMode::Top,
            CameraMode::Top => CameraMode::Overview,
        });
        self.save();
    }

    fn set_auto_roll(&mut self, on: bool) {
        self.auto_roll = on;
        self.stage_clock = 0.0;
        self.save();
    }

    fn save(&mut self) {
        self.settings.theme = self.theme as i32;
        self.settings.level = self.level;
        self.settings.mode = self.mode as i32;
        self.settings.game = self.variant as i32;
        self.settings.camera = self.camera.mode() as i32;
        self.settings.auto_roll = self.auto_roll;
        self.settings.score = self.score;
        self.settings.sound = self.sound.enabled;
        self.settings.panel = self.panel_visible;
        self.settings.language = self.language;
        self.settings.game_state = self.save_game();
        // The history goes with the game, as far back as it is likely to be read.
        self.settings.history = if self.settings.game_state.is_empty() {
            String::new()
        } else {
            let recent = &self.history[self.history.len().saturating_sub(80)..];
            recent.iter().map(|line| format!("{}{}", line.kind.digit(), line.text)).collect::<Vec<_>>().join("|")
        };
        self.settings.window = self.placement;
        self.settings.maximized = self.maximized;
        self.settings.save();
    }

    fn toggle_language(&mut self) {
        self.language = self.language.other();
        self.gl.window.set_title(self.t(Str::Title));
        self.save();
    }

    fn add_history(&mut self, text: String, kind: HistoryKind) {
        self.history.push(HistoryLine { text, kind });
        self.follow_history = true;
    }

    /// A turn as players write it, from the mover's side: one checker's steps joined ("24/13", or "24/18*/13" when
    /// it hit on the way), and the same move made twice counted ("13/7(2)").
    fn move_text(&self, steps: &[Step]) -> String {
        let name = |point: i8| match point {
            FROM_BAR => self.t(Str::Bar).to_owned(),
            BEAR_OFF => self.t(Str::Off).to_owned(),
            point => (point + 1).to_string(),
        };
        let mut parts: Vec<String> = Vec::new();
        let mut first = 0;
        while first < steps.len() {
            let mut last = first;
            while steps.get(last + 1).is_some_and(|next| next.from == steps[last].to && steps[last].to != BEAR_OFF) {
                last += 1;
            }
            let mut part = name(steps[first].from);
            for (index, step) in steps.iter().enumerate().take(last + 1).skip(first) {
                if index == last || step.hit {
                    part += &format!("/{}{}", name(step.to), if step.hit { "*" } else { "" });
                }
            }
            parts.push(part);
            first = last + 1;
        }
        let mut text = String::new();
        for (index, part) in parts.iter().enumerate() {
            if parts[..index].contains(part) {
                continue;
            }
            let times = parts.iter().filter(|other| *other == part).count();
            if !text.is_empty() {
                text.push(' ');
            }
            text += part;
            if times > 1 {
                text += &format!("({times})");
            }
        }
        text
    }

    // Input ------------------------------------------------------------------------------------------

    /// Where the ray through the pointer meets the plane at a height, looking down on it.
    fn pointer_on_plane(&self, height: f32) -> Option<Vec3> {
        if self.pointer.x < 0.0 || self.pointer.y < 0.0 {
            return None;
        }
        let (origin, direction) = self.camera.ray(self.pointer.x, self.pointer.y, self.width as f32, self.height as f32);
        if direction.y > -1e-4 {
            return None;
        }
        let distance = (height - origin.y) / direction.y;
        (distance >= 0.0).then(|| origin + direction * distance)
    }

    /// The place under the pointer, as the board's view counts places.
    fn place_under_pointer(&self) -> Option<usize> {
        let hit = self.pointer_on_plane(board::CHECKER_HEIGHT)?;
        if let Some(point) = board::point_at(vec2(hit.x, hit.z)) {
            return Some(point);
        }
        if hit.x.abs() < board::BAR_HALF && hit.z.abs() < board::HALF_DEPTH {
            return Some(BAR_PLACE);
        }
        let tray_left = board::FIELD_RIGHT + board::RIM_WIDTH * 0.5;
        let in_tray = hit.x > tray_left && hit.x < tray_left + board::RIM_WIDTH + board::TRAY_WIDTH && hit.z.abs() < board::HALF_DEPTH + board::RIM_WIDTH;
        in_tray.then_some(OFF_PLACE)
    }

    fn handle_board_pointer(&mut self) {
        let free = !self.pointer.consumed && !self.help_visible;
        self.hover_place = if free && self.gesture != Gesture::TurnCamera { self.place_under_pointer() } else { None };
        let moving = self.human_to_move() && self.done_clock <= 0.0;

        // The places whose top checker may move now, and where the checker chosen may go.
        let mut movable_places = [false; PLACES];
        let mut targets: Vec<(usize, Route)> = Vec::new();
        if moving {
            for step in self.next_steps() {
                movable_places[self.to_place(step.from)] = true;
            }
            if let Some(selected) = self.selected {
                targets = self.routes_from(self.point_at(selected)).into_iter().map(|route| (self.to_place(route.to), route)).collect();
            }
        }
        let movable = |place: Option<usize>| place.is_some_and(|place| movable_places[place]);
        let route_to = |place: Option<usize>| targets.iter().find(|(to, _)| Some(*to) == place).map(|(_, route)| route.clone());

        let awaiting_roll = self.stage == Stage::ToRoll && self.is_human(self.to_play) && !self.auto_roll;
        if movable(self.hover_place) || route_to(self.hover_place).is_some() || (free && awaiting_roll && self.hover_place.is_some()) {
            self.cursor_hand = true;
        }

        if free && self.gesture == Gesture::None && (self.pointer.left_pressed || self.pointer.right_pressed) {
            self.gesture = Gesture::Press;
            self.press_right = !self.pointer.left_pressed;
            (self.press_x, self.press_y) = (self.pointer.x, self.pointer.y);
            (self.drag_x, self.drag_y) = (self.pointer.x, self.pointer.y);
            self.press_place = self.hover_place;
            self.press_double = self.pointer.double_click;
        }
        let down = if self.press_right { self.pointer.right_down } else { self.pointer.left_down };

        if self.gesture == Gesture::Press {
            let moved = (self.pointer.x - self.press_x).hypot(self.pointer.y - self.press_y) > DRAG_START * self.scale;
            if moved && down {
                if !self.press_right && moving && movable(self.press_place) {
                    self.gesture = Gesture::Drag;
                    self.select(self.press_place);
                } else {
                    self.gesture = Gesture::TurnCamera;
                }
            } else if !down {
                self.gesture = Gesture::None;
                self.click(self.press_place, awaiting_roll, moving, movable(self.press_place), route_to(self.press_place));
            }
        }

        if self.gesture == Gesture::TurnCamera {
            if down {
                self.camera.rotate(-(self.pointer.x - self.drag_x) * 0.006, (self.pointer.y - self.drag_y) * 0.005);
                (self.drag_x, self.drag_y) = (self.pointer.x, self.pointer.y);
            } else {
                self.gesture = Gesture::None;
            }
        }

        if self.gesture == Gesture::Drag {
            match self.selected.filter(|_| moving) {
                None => {
                    self.gesture = Gesture::None;
                    self.view.carry(None);
                }
                Some(_) if self.pointer.left_down => {
                    if let Some(at) = self.pointer_on_plane(0.03) {
                        self.view.carry(Some(at));
                    }
                }
                Some(selected) => {
                    // Dropped: where it may go, else back to its place.
                    self.gesture = Gesture::None;
                    match route_to(self.place_under_pointer()) {
                        Some(route) => self.play_route(&route),
                        None => {
                            self.view.move_checker(self.to_play, selected, selected, 0.0);
                            self.select(None);
                        }
                    }
                }
            }
        }

        if free && self.pointer.wheel != 0.0 {
            self.camera.zoom(self.pointer.wheel);
        }
    }

    /// A click on a place of the board, or beside it: with the right button it takes a step back; with the left it
    /// rolls the dice, or chooses a checker, or sends the chosen one along `route`.
    fn click(&mut self, place: Option<usize>, awaiting_roll: bool, moving: bool, movable: bool, route: Option<Route>) {
        if self.press_right {
            if self.human_to_move() && !self.played.is_empty() {
                self.take_back_step();
            } else {
                self.select(None);
            }
        } else if awaiting_roll {
            if place.is_some() {
                self.roll_dice();
            }
        } else if moving {
            let toggled = self.toggled_place.take();
            match (place, route) {
                // The second click of a double click on a checker.
                (Some(place), _) if self.press_double && toggled == Some(place) && movable => self.play_larger_die(self.point_at(place)),
                (_, Some(route)) => self.play_route(&route),
                (Some(_), None) if movable => {
                    self.select(if place == self.selected { None } else { place });
                    self.toggled_place = place;
                }
                _ => {
                    // A checker of the mover's own that may not go: why not.
                    let mover = self.mover_view();
                    let own = match place.map(|place| self.point_at(place)) {
                        Some(FROM_BAR) => (mover.bar[0] > 0).then_some(FROM_BAR),
                        Some(point) => mover.points.get(point as usize).is_some_and(|checkers| *checkers > 0).then_some(point),
                        None => None,
                    };
                    if let Some(point) = own {
                        self.show_toast(self.why_not(point));
                    }
                    self.select(None);
                }
            }
        }
    }

    fn handle_key(&mut self, key: Key) {
        if self.help_visible {
            // Over the help only its own keys work, and full screen.
            match key {
                Key::Escape | Key::F1 => self.help_visible = false,
                Key::F11 => self.toggle_full_screen(),
                _ => {}
            }
            return;
        }
        if self.modifiers.control_key() {
            match key {
                Key::Character('Z') => self.undo(),
                Key::Character('N') => self.new_game(),
                _ => {}
            }
            return;
        }

        let step = if self.modifiers.shift_key() { -1 } else { 1 };
        match key {
            Key::Space | Key::Enter => {
                if self.stage == Stage::ToRoll && self.is_human(self.to_play) {
                    self.roll_dice();
                } else if self.human_to_move() && self.done_clock > 0.0 {
                    self.end_turn();
                }
            }
            Key::Character('H') => self.request_hint(),
            Key::Backspace => {
                if self.human_to_move() && !self.played.is_empty() {
                    self.take_back_step();
                } else {
                    self.undo();
                }
            }
            Key::Character('A') => self.set_auto_roll(!self.auto_roll),
            Key::Character('C') => self.switch_camera(),
            Key::Home => self.camera.reset(),
            Key::Character('N') => self.new_game(),
            Key::Character('G') => self.switch_game(),
            Key::Character('M') => self.set_mode(self.mode.stepped(step)),
            Key::Character('T') => self.set_theme(self.theme as i32 + step),
            Key::Character('V') => {
                self.sound.enabled = !self.sound.enabled;
                self.save();
            }
            Key::Character('L') => self.toggle_language(),
            Key::Tab => {
                self.panel_visible = !self.panel_visible;
                self.save();
            }
            Key::F1 => self.help_visible = !self.help_visible,
            Key::F11 => self.toggle_full_screen(),
            Key::Escape => {
                if let (Gesture::Drag, Some(selected)) = (self.gesture, self.selected) {
                    // The checker carried goes back to its place.
                    self.gesture = Gesture::None;
                    self.view.move_checker(self.to_play, selected, selected, 0.0);
                    self.select(None);
                } else if self.selected.is_some() {
                    self.select(None);
                } else if self.full_screen {
                    self.toggle_full_screen();
                }
            }
            Key::Character(level @ '1'..='5') => self.set_level(level as i32 - '0' as i32),
            _ => {}
        }
    }

    // The interface ----------------------------------------------------------------------------------

    /// Whether a dialog is open over everything else.
    fn modal(&self) -> bool {
        self.help_visible
    }

    /// A text in lines that fit a width, broken at its spaces.
    fn wrap(&self, text: &str, width: f32) -> Vec<String> {
        let mut lines = Vec::new();
        let mut line = String::new();
        for word in text.split(' ') {
            let longer = if line.is_empty() { word.to_owned() } else { format!("{line} {word}") };
            if !line.is_empty() && self.painter.measure(FontId::Body, &longer) > width {
                lines.push(std::mem::replace(&mut line, word.to_owned()));
            } else {
                line = longer;
            }
        }
        if !line.is_empty() {
            lines.push(line);
        }
        lines
    }

    /// A text cut to fit a width, with an ellipsis where it was cut.
    fn fitted(&self, font: FontId, text: &str, width: f32) -> String {
        if self.painter.measure(font, text) <= width {
            return text.to_owned();
        }
        let mut cut = text.to_owned();
        while cut.chars().nth(1).is_some() && self.painter.measure(font, &format!("{cut}…")) > width {
            cut.pop();
        }
        cut + "…"
    }

    /// The rules of the game on the board, in a few sentences.
    fn rules(&self) -> &'static str {
        self.t(match self.variant {
            Variant::Long => Str::RulesLong,
            Variant::Short => Str::RulesShort,
        })
    }

    fn help_card(&self) -> Rect {
        let scale = self.scale;
        let free = self.free_area();
        let width = (HELP_WIDTH * scale).min(free.width - 20.0 * scale);
        let rows = (HELP_ROWS + 1 + self.wrap(self.rules(), width - 2.0 * HELP_PAD * scale).len()) as f32;
        let height = (2.0 * HELP_PAD + 44.0 + 12.0 + rows * HELP_ROW) * scale;
        rect(free.x + (free.width - width) / 2.0, (10.0 * scale).max((self.height as f32 - height) / 2.0), width, height)
    }

    fn banner_card(&self) -> Rect {
        let scale = self.scale;
        let title = self.painter.measure(FontId::Display, &self.banner_title) + 80.0 * scale;
        let detail = self.painter.measure(FontId::Body, &self.banner_detail) + 60.0 * scale;
        let width = title.max(detail).max(320.0 * scale);
        let free = self.free_area();
        rect(free.x + (free.width - width) / 2.0, 26.0 * scale - (1.0 - self.banner_fade) * 20.0 * scale, width, 92.0 * scale)
    }

    fn frosted_panels(&self) -> Vec<FrostedPanel> {
        let mut panels = Vec::new();
        if self.panel_slide > 0.001 {
            panels.push(self.frosted(self.panel_rect(), 18.0, 1.0));
        }
        if self.banner_fade > 0.001 {
            panels.push(self.frosted(self.banner_card(), 18.0, self.banner_fade));
        }
        if self.help_visible {
            panels.push(self.frosted(self.help_card(), 20.0, 1.0));
        }
        panels
    }

    /// The words for one side: "you" or "the computer" against the computer, else "White" or "Black".
    fn pick(&self, side: usize, you: Str, computer: Str, white: Str, black: Str) -> &'static str {
        self.t(match (self.mode, side) {
            (Mode::VsComputer, 0) => you,
            (Mode::VsComputer, _) => computer,
            (_, 0) => white,
            _ => black,
        })
    }

    fn name(&self, side: usize) -> &'static str {
        self.pick(side, Str::NameYou, Str::NameComputer, Str::NameWhite, Str::NameBlack)
    }

    fn status_title(&self) -> &'static str {
        match (self.stage, self.winner) {
            (Stage::Opening, _) => self.t(Str::Opening),
            (Stage::Over, Some(winner)) => self.pick(winner, Str::WinYou, Str::WinComputer, Str::WinWhite, Str::WinBlack),
            _ => self.pick(self.to_play, Str::TurnYou, Str::TurnComputer, Str::TurnWhite, Str::TurnBlack),
        }
    }

    fn status_detail(&self) -> String {
        let detail = match self.stage {
            Stage::Opening => Str::OpeningDetail,
            Stage::Over if self.mode == Mode::Demo => Str::DemoNext,
            Stage::Over => Str::PressNew,
            Stage::ToRoll if self.is_human(self.to_play) && !self.auto_roll => Str::RollPrompt,
            Stage::ToRoll | Stage::Rolling => Str::Rolling,
            Stage::Thinking => Str::Thinking,
            Stage::Playing => Str::ComputerMoving,
            Stage::Moving if self.sequences.is_empty() => Str::NoMovesToast,
            Stage::Moving if self.done_clock > 0.0 => Str::TurnDone,
            Stage::Moving => {
                // The dice still to play.
                let mut dice = self.roll.dice();
                for step in &self.played {
                    if let Some(used) = dice.iter().position(|die| *die == step.die) {
                        dice.remove(used);
                    }
                }
                let left = dice.iter().map(i8::to_string).collect::<Vec<_>>().join(", ");
                return fill(self.t(Str::ToPlay), &[&left]);
            }
        };
        self.t(detail).to_owned()
    }

    fn draw_interface(&mut self) {
        self.draw_panel();
        self.draw_banner();
        self.draw_toasts();
        if self.help_visible {
            self.draw_help();
        }
    }

    /// A checker seen from above, in its side's colors from the theme.
    fn draw_checker(&mut self, x: f32, y: f32, radius: f32, side: usize) {
        let linear = self.theme().checkers[side].color;
        // Brightened toward what the lit board shows.
        let color = ui::Color::from_linear(linear * 1.25 + vec3(0.02, 0.02, 0.02), 1.0);
        let contrast = if linear.x + linear.y + linear.z > 0.6 { rgb(0, 0, 0) } else { rgb(255, 255, 255) };
        let painter = &mut self.painter;
        painter.disc(x, y + radius * 0.14, radius * 1.06, rgba(0, 0, 0, 80));
        painter.disc(x, y, radius, color.mix(contrast, 0.18));
        painter.disc(x, y, radius * 0.84, color);
        painter.disc(x, y, radius * 0.5, color.mix(contrast, 0.1));
        painter.disc(x, y, radius * 0.42, color);
    }

    fn draw_status(&mut self, area: Rect) {
        let scale = self.scale;
        let accent = self.accent();
        let middle = area.y + area.height / 2.0;
        self.painter.fill(area, 12.0 * scale, rgba(255, 255, 255, 14));

        // A checker of the side to move, or of the winner.
        let side = if self.stage == Stage::Over { self.winner } else { Some(self.to_play) };
        if let Some(side) = side.filter(|_| self.stage != Stage::Opening) {
            self.draw_checker(area.x + 24.0 * scale, middle, 11.0 * scale, side);
        }

        // The dice of the turn on the right, the played ones darkened.
        let mut right = area.right() - 10.0 * scale;
        if self.rolled && !matches!(self.stage, Stage::Rolling | Stage::ToRoll | Stage::Over) {
            let dice = self.roll.dice();
            let (size, gap) = (if dice.len() > 2 { 17.0 * scale } else { 22.0 * scale }, 5.0 * scale);
            let mut used = vec![false; dice.len()];
            for step in &self.played {
                if let Some(index) = (0..dice.len()).find(|index| !used[*index] && dice[*index] == step.die) {
                    used[index] = true;
                }
            }
            for (die, used) in dice.into_iter().zip(used).rev() {
                draw_die(&mut self.painter, right - size / 2.0, middle, size, die, if used { 1.0 } else { 0.0 });
                right -= size + gap;
            }
            right -= 4.0 * scale;
        }
        if self.stage == Stage::Thinking {
            self.painter.spinner(right - 10.0 * scale, middle, 8.0 * scale, 2.2 * scale, self.time * 5.0, accent);
            right -= 26.0 * scale;
        }

        let text_x = area.x + if self.stage == Stage::Opening { 16.0 * scale } else { 46.0 * scale };
        let line_height = self.painter.line_height(FontId::Heading);
        let title = self.fitted(FontId::Heading, self.status_title(), right - text_x);
        let detail = self.fitted(FontId::Body, &self.status_detail(), right - text_x);
        self.painter.text(FontId::Heading, text_x, middle - line_height + 3.0 * scale, &title, if self.stage == Stage::Over { accent } else { TEXT_COLOR });
        self.painter.text(FontId::Body, text_x, middle + 2.0 * scale, &detail, MUTED_COLOR);
    }

    /// The players: who is to move, their pips and checkers borne off, and the points they have won.
    fn draw_players(&mut self, area: Rect) {
        let scale = self.scale;
        let accent = self.accent();
        let row_height = 46.0 * scale;
        for side in 0..2 {
            let y = area.y + side as f32 * row_height;
            let to_move = !matches!(self.stage, Stage::Over | Stage::Opening) && self.to_play == side;
            let winner = self.stage == Stage::Over && self.winner == Some(side);
            let lit = to_move || winner;
            if lit {
                let row = rect(area.x - 6.0 * scale, y + 2.0 * scale, area.width + 12.0 * scale, row_height - 4.0 * scale);
                self.painter.fill(row, 9.0 * scale, accent.fade(if winner { 0.3 } else { 0.16 }));
            }
            self.draw_checker(area.x + 10.0 * scale, y + row_height / 2.0, 9.0 * scale, side);

            let score = self.score[side].to_string();
            let score_x = area.right() - self.painter.measure(FontId::Display, &score);
            let score_y = y + (row_height - self.painter.line_height(FontId::Display)) / 2.0;
            self.painter.text(FontId::Display, score_x, score_y, &score, if lit { TEXT_COLOR } else { TEXT_COLOR.fade(0.7) });

            // The name and the count, as far as the score leaves them room.
            let mut name = self.name(side).to_owned();
            if !self.is_human(side) {
                name += &format!("  ·  {}", self.t(Str::level(self.level)));
            }
            let pips = fill(self.t(Str::Pips), &[&nardy::pips(&self.board, side, self.variant), &self.board.off[side]]);
            let text_x = area.x + 28.0 * scale;
            let room = score_x - 8.0 * scale - text_x;
            let body_height = self.painter.line_height(FontId::Body);
            let (name, pips) = (self.fitted(FontId::Body, &name, room), self.fitted(FontId::Body, &pips, room));
            self.painter.text(FontId::Body, text_x, y + row_height / 2.0 - body_height + scale, &name, if lit { TEXT_COLOR } else { TEXT_COLOR.fade(0.8) });
            self.painter.text(FontId::Body, text_x, y + row_height / 2.0 + scale, &pips, MUTED_COLOR);
        }
    }

    fn draw_history(&mut self, area: Rect) {
        let scale = self.scale;
        self.painter.fill(area, 12.0 * scale, rgba(0, 0, 0, 40));
        if self.history.is_empty() {
            self.painter.text_centered(FontId::Body, area, self.t(Str::NoTurnsYet), MUTED_COLOR);
            return;
        }

        // The last turns in lines that fit: each with its kind, and whether it begins its turn.
        let inner = area.inset(9.0 * scale);
        let mut lines = Vec::new();
        for entry in &self.history[self.history.len().saturating_sub(120)..] {
            let wrapped = self.wrap(&entry.text, inner.width - 16.0 * scale);
            lines.extend(wrapped.into_iter().enumerate().map(|(index, line)| (line, entry.kind, index == 0)));
        }
        let row_height = (22.0 * scale).round();
        let visible = ((inner.height / row_height) as usize).max(1);
        let max_scroll = lines.len().saturating_sub(visible) as f32;
        if area.contains(self.pointer.x, self.pointer.y) && self.pointer.wheel != 0.0 && !self.help_visible {
            self.history_scroll = (self.history_scroll - self.pointer.wheel * 3.0).clamp(0.0, max_scroll);
            self.follow_history = self.history_scroll >= max_scroll;
            self.pointer.wheel = 0.0;
        }
        if self.follow_history {
            self.history_scroll = max_scroll;
        }
        self.history_scroll = self.history_scroll.clamp(0.0, max_scroll);

        let accent = self.accent();
        let theme = self.theme();
        let first = self.history_scroll as usize;
        let body_height = self.painter.line_height(FontId::Body);
        for (row, (index, (text, kind, begins))) in lines.iter().enumerate().skip(first).take(visible).enumerate() {
            let y = inner.y + row as f32 * row_height;
            let color = match kind {
                HistoryKind::Turn(_) => TEXT_COLOR,
                HistoryKind::Note => NOTE_COLOR,
                HistoryKind::End => accent,
            };
            if let (true, HistoryKind::Turn(side)) = (*begins, kind) {
                let checker = ui::Color::from_linear(theme.checkers[*side].color * 1.25 + vec3(0.03, 0.03, 0.03), 1.0);
                self.painter.disc(inner.x + 4.0 * scale, y + row_height / 2.0, 3.5 * scale, checker);
            } else if *begins {
                self.painter.disc(inner.x + 4.0 * scale, y + row_height / 2.0, 2.5 * scale, color.fade(0.7));
            }
            // The turns before the last are a little dimmer.
            let bright = index + 1 == lines.len() || !matches!(kind, HistoryKind::Turn(_));
            self.painter.text(FontId::Body, inner.x + 16.0 * scale, y + (row_height - body_height) / 2.0, text, if bright { color } else { color.fade(0.82) });
        }
        if max_scroll > 0.0 {
            let bar_height = inner.height * visible as f32 / lines.len() as f32;
            let bar_y = inner.y + (inner.height - bar_height) * (self.history_scroll / max_scroll);
            self.painter.fill(rect(area.right() - 5.0 * scale, bar_y, 3.0 * scale, bar_height), 1.5 * scale, rgba(255, 255, 255, 50));
        }
    }

    fn draw_panel(&mut self) {
        if self.panel_slide <= 0.001 {
            return;
        }
        let scale = self.scale;
        let panel = self.panel_rect();
        if panel.contains(self.pointer.x, self.pointer.y) && (self.pointer.left_pressed || self.pointer.right_pressed) && self.gesture == Gesture::None {
            self.pointer.consumed = true;
        }
        self.painter.shadow(panel, 22.0 * scale, rgba(0, 0, 0, 70));
        self.painter.fill(panel, 18.0 * scale, rgba(16, 18, 26, 120));
        self.painter.outline(panel, 18.0 * scale, rgba(255, 255, 255, 24));

        // In a low window everything is packed closer, and the footer goes.
        let compact = panel.height < 820.0 * scale;
        let sized = |packed: f32, roomy: f32| if compact { packed * scale } else { roomy * scale };
        let pad = sized(16.0, 20.0);
        let (x, width) = (panel.x + pad, panel.width - 2.0 * pad);
        let mut y = panel.y + pad;

        self.painter.text(FontId::Heading, x, y, self.t(Str::Title), TEXT_COLOR);
        if self.button(HELP_BUTTON, rect(panel.right() - pad - 30.0 * scale, y - 2.0 * scale, 30.0 * scale, 30.0 * scale), "?", true, false) {
            self.help_visible = !self.help_visible;
        }
        y += sized(38.0, 44.0);

        let status_height = sized(54.0, 62.0);
        self.draw_status(rect(x, y, width, status_height));
        y += status_height + sized(6.0, 10.0);

        let players_height = 92.0 * scale;
        self.draw_players(rect(x + 4.0 * scale, y, width - 8.0 * scale, players_height));
        y += players_height + sized(6.0, 10.0);

        // The buttons of the turn.
        let (row_height, gap) = (sized(29.0, 34.0), sized(5.0, 8.0));
        let half = (width - gap) / 2.0;
        let button_height = sized(31.0, 38.0);
        let can_roll = self.stage == Stage::ToRoll && self.is_human(self.to_play);
        if self.button(ROLL_BUTTON, rect(x, y, half, button_height), self.t(Str::Roll), can_roll, can_roll) {
            self.roll_dice();
        }
        let can_hint = self.human_to_move() && !self.sequences.is_empty() && self.done_clock <= 0.0 && !self.hint_wanted;
        if self.button(HINT_BUTTON, rect(x + half + gap, y, half, button_height), self.t(Str::Hint), can_hint, can_hint && self.hint.is_some()) {
            self.request_hint();
        }
        y += button_height + gap;
        let earlier = self.undo.iter().any(|snapshot| snapshot.turn < self.turn || self.stage == Stage::Over);
        let can_undo = self.mode != Mode::Demo && ((self.human_to_move() && !self.played.is_empty()) || earlier);
        if self.button(UNDO_BUTTON, rect(x, y, half, button_height), self.t(Str::Undo), can_undo, false) {
            self.undo();
        }
        if self.button(NEW_GAME_BUTTON, rect(x + half + gap, y, half, button_height), self.t(Str::NewGame), true, self.stage == Stage::Over) {
            self.new_game();
        }
        y += button_height + sized(8.0, 12.0);

        // From the bottom up: the footer, the switches and the settings; the moves take the space between.
        let mut bottom = panel.bottom() - pad;
        if !compact {
            let footer_height = self.painter.line_height(FontId::Body);
            self.painter.text_centered(FontId::Body, rect(x, bottom - footer_height, width, footer_height), self.t(Str::Footer), MUTED_COLOR.fade(0.8));
            bottom -= footer_height + 14.0 * scale;
        }

        bottom -= row_height;
        if self.button(SOUND_BUTTON, rect(x, bottom, half, row_height), self.t(Str::Sound), true, self.sound.enabled) {
            self.sound.enabled = !self.sound.enabled;
            self.save();
        }
        if self.button(LANGUAGE_BUTTON, rect(x + half + gap, bottom, half, row_height), self.t(Str::LanguageName), true, false) {
            self.toggle_language();
        }
        bottom -= gap + row_height;
        if self.button(AUTO_ROLL_BUTTON, rect(x, bottom, half, row_height), self.t(Str::AutoRoll), true, self.auto_roll) {
            self.set_auto_roll(!self.auto_roll);
        }
        let view = self.t(match self.camera.mode() {
            CameraMode::Overview => Str::CameraOverview,
            CameraMode::Top => Str::CameraTop,
        });
        if self.button(CAMERA_BUTTON, rect(x + half + gap, bottom, half, row_height), &format!("{}: {view}", self.t(Str::Camera)), true, false) {
            self.switch_camera();
        }
        bottom -= gap;

        let theme = self.theme();
        let game = match self.variant {
            Variant::Long => Str::GameLong,
            Variant::Short => Str::GameShort,
        };
        let choices = [
            (THEME_SELECTOR, Str::Theme, (if self.language == Language::Russian { theme.name_russian } else { theme.name_english }).to_owned()),
            (LEVEL_SELECTOR, Str::Level, format!("{} · {}", self.level, self.t(Str::level(self.level)))),
            (MODE_SELECTOR, Str::Mode, self.t(self.mode.name()).to_owned()),
            (GAME_SELECTOR, Str::Game, self.t(game).to_owned()),
        ];
        for (id, label, value) in choices {
            bottom -= row_height;
            let change = self.selector(id, rect(x, bottom, width, row_height), self.t(label), &value);
            if change != 0 {
                match id {
                    THEME_SELECTOR => self.set_theme(self.theme as i32 + change),
                    LEVEL_SELECTOR => self.set_level(self.level + change),
                    MODE_SELECTOR => self.set_mode(self.mode.stepped(change)),
                    _ => self.switch_game(),
                }
            }
            bottom -= sized(3.0, 6.0);
        }
        bottom -= sized(6.0, 10.0);

        // The moves so far, where there is room for them.
        let label_height = self.painter.line_height(FontId::Body) + 6.0 * scale;
        if bottom - y - label_height > 36.0 * scale {
            self.painter.text(FontId::Body, x, y, self.t(Str::History), MUTED_COLOR);
            self.draw_history(rect(x, y + label_height, width, bottom - y - label_height));
        }
    }

    fn draw_banner(&mut self) {
        if self.banner_fade <= 0.001 {
            return;
        }
        let scale = self.scale;
        let fade = self.banner_fade;
        let card = self.banner_card();
        self.painter.shadow(card, 24.0 * scale, rgba(0, 0, 0, 120).fade(fade));
        self.painter.fill(card, 18.0 * scale, rgba(14, 16, 24, 150).fade(fade));
        self.painter.outline(card, 18.0 * scale, self.accent().fade(0.6 * fade));
        self.painter.text_centered(FontId::Display, rect(card.x, card.y + 8.0 * scale, card.width, 48.0 * scale), &self.banner_title, TEXT_COLOR.fade(fade));
        self.painter.text_centered(FontId::Body, rect(card.x, card.y + 54.0 * scale, card.width, 26.0 * scale), &self.banner_detail, MUTED_COLOR.fade(fade));
    }

    fn draw_help(&mut self) {
        let scale = self.scale;
        self.painter.fill(rect(0.0, 0.0, self.width as f32, self.height as f32), 0.0, rgba(0, 0, 0, 110));

        let (row_height, pad) = (HELP_ROW * scale, HELP_PAD * scale);
        let card = self.help_card();
        let accent = self.accent();
        self.painter.shadow(card, 26.0 * scale, rgba(0, 0, 0, 140));
        self.painter.fill(card, 20.0 * scale, rgba(18, 20, 28, 175));
        self.painter.outline(card, 20.0 * scale, rgba(255, 255, 255, 30));
        self.painter.text(FontId::Heading, card.x + pad, card.y + pad - 4.0 * scale, self.t(Str::Controls), TEXT_COLOR);
        let top = card.y + pad + 44.0 * scale;
        // The first column, one line shorter, takes a little over half the card, or what its longest line needs.
        let first_rows = HELP_ROWS - 1;
        let (left_keys, right_keys) = (228.0 * scale, 130.0 * scale);
        let longest = HELP_LINES[..first_rows].iter().map(|(_, _, action)| self.painter.measure(FontId::Body, self.t(*action))).fold(0.0, f32::max);
        let left_width = ((card.width - 2.0 * pad) * 0.57).max(left_keys + longest + 20.0 * scale);
        for (index, (named, keys, action)) in HELP_LINES.into_iter().enumerate() {
            let (column_x, keys_width, row) =
                if index < first_rows { (card.x + pad, left_keys, index) } else { (card.x + pad + left_width, right_keys, index - first_rows) };
            let y = top + row as f32 * row_height;
            self.painter.text(FontId::Body, column_x, y, named.map_or(keys, |id| self.t(id)), accent);
            self.painter.text(FontId::Body, column_x + keys_width, y, self.t(action), TEXT_COLOR);
        }
        let mut y = top + HELP_ROWS as f32 * row_height + 12.0 * scale;
        self.painter.text(FontId::Body, card.x + pad, y, self.t(Str::Rules), accent);
        for line in self.wrap(self.rules(), card.width - 2.0 * pad) {
            y += row_height;
            self.painter.text(FontId::Body, card.x + pad, y, &line, MUTED_COLOR);
        }

        let close = self.button(HELP_CLOSE, rect(card.right() - pad - 30.0 * scale, card.y + pad - 8.0 * scale, 30.0 * scale, 30.0 * scale), "×", true, false);
        if close || (self.pointer.left_pressed && !card.contains(self.pointer.x, self.pointer.y)) {
            self.help_visible = false;
        }
        if self.pointer.left_pressed || self.pointer.right_pressed {
            self.pointer.consumed = true;
        }
    }
}
