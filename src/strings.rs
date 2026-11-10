//! The interface's words in English and Russian.

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Language {
    English,
    Russian,
}

impl Language {
    pub fn other(self) -> Language {
        match self {
            Language::English => Language::Russian,
            Language::Russian => Language::English,
        }
    }
}

macro_rules! strings {
    ($($id:ident: $english:literal, $russian:literal;)*) => {
        #[derive(Clone, Copy, PartialEq, Eq)]
        pub enum Str {
            $($id),*
        }

        const TABLE: &[(&str, &str)] = &[$(($english, $russian)),*];
    };
}

strings! {
    Title: "3D Nardy", "Нарды 3D";
    Game: "Game", "Игра";
    GameLong: "Long nardy", "Длинные нарды";
    GameShort: "Short nardy", "Короткие нарды";
    Mode: "Mode", "Режим";
    ModeComputer: "Vs computer", "С компьютером";
    ModeTwoPlayers: "Two players", "Два игрока";
    ModeDemo: "Demo game", "Демонстрация";
    Level: "Level", "Уровень";
    Level1: "Novice", "Новичок";
    Level2: "Beginner", "Начинающий";
    Level3: "Amateur", "Любитель";
    Level4: "Strong", "Сильный";
    Level5: "Master", "Мастер";
    Theme: "Theme", "Оформление";
    Sound: "Sound", "Звук";
    LanguageName: "English", "Русский";
    AutoRoll: "Auto roll", "Автобросок";
    Camera: "View", "Вид";
    CameraOverview: "overview", "обзор";
    CameraTop: "from above", "сверху";
    NewGame: "New game", "Новая игра";
    Undo: "Undo", "Отменить";
    Roll: "Roll", "Бросить кости";
    Hint: "Hint", "Подсказка";
    NameYou: "You", "Вы";
    NameComputer: "Computer", "Компьютер";
    NameWhite: "White", "Белые";
    NameBlack: "Black", "Чёрные";
    TurnYou: "Your turn", "Ваш ход";
    TurnComputer: "Computer's turn", "Ход компьютера";
    TurnWhite: "White to play", "Ход белых";
    TurnBlack: "Black to play", "Ход чёрных";
    WinYou: "You win!", "Вы победили!";
    WinComputer: "The computer wins", "Победил компьютер";
    WinWhite: "White wins", "Победа белых";
    WinBlack: "Black wins", "Победа чёрных";
    StartsYou: "You go first", "Вы начинаете";
    StartsComputer: "The computer goes first", "Компьютер начинает";
    StartsWhite: "White goes first", "Начинают белые";
    StartsBlack: "Black goes first", "Начинают чёрные";
    Opening: "Who goes first", "Кто начинает";
    OpeningDetail: "Each player throws one die", "Каждый бросает одну кость";
    OpeningTie: "A tie, {}:{} — throwing again", "Поровну, {}:{} — бросаем снова";
    RollPrompt: "Roll: Space or click the board", "Бросок: пробел или щелчок по доске";
    Rolling: "The dice are rolling…", "Кости катятся…";
    ToPlay: "To play: {}", "Сыграть: {}";
    TurnDone: "Turn played", "Ход сыгран";
    Thinking: "Thinking…", "Думает…";
    ComputerMoving: "Moving…", "Ходит…";
    NoMoves: "no moves", "ходов нет";
    NoMovesToast: "No moves — the turn passes", "Ходов нет — ход переходит";
    PressNew: "N or New game to play again", "N — новая игра";
    DemoNext: "A new game begins shortly", "Скоро новая партия";
    Pips: "Pips {}  ·  Off {}", "Пипсы {}  ·  Сброшено {}";
    Score: "Score {} : {}", "Счёт {} : {}";
    PointsOne: "A single game, 1 point", "Обычная победа, 1 очко";
    PointsMars: "Mars, 2 points", "Марс, 2 очка";
    PointsKoks: "Koks, 3 points", "Кокс, 3 очка";
    History: "Moves", "Ходы";
    NoTurnsYet: "No moves yet", "Ходов пока нет";
    NothingToUndo: "Nothing to undo", "Нечего отменять";
    EnterFirst: "First bring in the checker from the bar", "Сначала введите шашку с бара";
    HeadRule: "Only one checker may leave the head each turn", "С головы за ход можно снять только одну шашку";
    CannotMove: "That checker cannot move now", "Эта шашка сейчас не может ходить";
    HintText: "Hint: {}", "Подсказка: {}";
    Bar: "bar", "бар";
    Off: "off", "сброс";
    Footer: "F1 help · Tab panel · F11 full screen", "F1 справка · Tab панель · F11 весь экран";
    Controls: "Controls", "Управление";
    KeyClick: "Click", "Щелчок";
    KeyDrag: "Drag", "Перетаскивание";
    KeyDoubleClick: "Double-click", "Двойной щелчок";
    KeyRightClick: "Right-click  Backspace", "Правый щелчок  Backspace";
    KeyRightDrag: "Right-drag", "Правой кнопкой";
    KeyWheel: "Wheel", "Колесо";
    HelpClick: "Choose a checker, then where it goes", "Выбрать шашку, затем куда";
    HelpDrag: "Move a checker", "Переставить шашку";
    HelpDoubleClick: "Play the higher die", "Сыграть большую кость";
    HelpTakeBack: "Take back a step", "Вернуть шаг";
    HelpRotate: "Turn the view", "Повернуть вид";
    HelpZoom: "Zoom", "Приблизить";
    HelpRoll: "Roll the dice; end the turn", "Бросить кости; закончить ход";
    HelpHint: "Hint; again to play it", "Подсказка; ещё раз — сыграть её";
    HelpUndo: "Undo the turn", "Отменить ход";
    HelpAutoRoll: "Roll by itself", "Автобросок";
    HelpCamera: "Change or reset the view", "Сменить или вернуть вид";
    HelpNew: "New game", "Новая игра";
    HelpGame: "Long or short nardy", "Длинные или короткие";
    HelpMode: "Mode", "Режим";
    HelpLevel: "Computer's level", "Уровень компьютера";
    HelpTheme: "Theme", "Оформление";
    HelpSound: "Sound on or off", "Звук";
    HelpLanguage: "Language", "Язык";
    HelpPanel: "Show or hide the panel", "Показать или скрыть панель";
    HelpFullScreen: "Full screen", "Весь экран";
    HelpClose: "Close this help", "Закрыть справку";
    Rules: "Rules", "Правила";
    RulesLong: "Long nardy. Each player starts with fifteen checkers on the head, and both run the same way round the board, from the head to their home and off. A checker moves the number of each die; a double is played four times. A single checker holds a point: you may not land where the opponent stands. Only one checker may leave the head a turn, two on a first roll of 6-6, 4-4 or 3-3. You may not hold six points in a row unless an opponent's checker is already past them. Play both dice if you can, else the higher one. Once all fifteen are home they are borne off; the first to bear off all wins, two points (mars) if the opponent has borne off none.",
        "Длинные нарды. У каждого пятнадцать шашек на «голове», и оба игрока идут по доске в одну сторону: от головы к своему дому и дальше за доску. Шашка ходит на число очков каждой кости, дубль играется четыре раза. Одна шашка уже занимает пункт: туда, где стоит соперник, ходить нельзя. С головы за ход снимается одна шашка, а в первый ход при 6-6, 4-4 и 3-3 — две. Нельзя ставить шесть пунктов подряд, если впереди них нет ни одной шашки соперника. Нужно сыграть обе кости, если можно, иначе большую. Когда все пятнадцать в доме, их выбрасывают; кто первым выбросил все — победил, а если у соперника нет ни одной выброшенной — марс, два очка.";
    RulesShort: "Short nardy (backgammon). The players run toward each other. A checker moves the number of each die; a double is played four times. A point with two or more opposing checkers is closed; a lone checker may be hit, and goes to the bar, from where it must enter the opponent's home before any other checker moves. Play both dice if you can, else the higher one. Once all fifteen are home they are borne off. A win is one point, mars (the opponent has borne off none) two, koks (and still has a checker on the bar or in the winner's home) three.",
        "Короткие нарды (бэкгаммон). Игроки идут навстречу друг другу. Шашка ходит на число очков каждой кости, дубль играется четыре раза. Пункт, где стоят две шашки соперника и больше, закрыт; одиночную шашку можно побить — она уходит на бар, и прежде чем ходить другими, её нужно ввести в дом соперника. Нужно сыграть обе кости, если можно, иначе большую. Когда все пятнадцать в доме, их выбрасывают. Победа — одно очко, марс (у соперника нет ни одной выброшенной) — два, кокс (и вдобавок его шашка на баре или в доме победителя) — три.";
}

impl Str {
    /// The name of a computer level from 1 to 5.
    pub fn level(level: i32) -> Str {
        [Str::Level1, Str::Level2, Str::Level3, Str::Level4, Str::Level5][(level.clamp(1, 5) - 1) as usize]
    }
}

pub fn tr(id: Str, language: Language) -> &'static str {
    let (english, russian) = TABLE[id as usize];
    match language {
        Language::English => english,
        Language::Russian => russian,
    }
}

/// A text with each "{}" in it replaced by the next of the values.
pub fn fill(text: &str, values: &[&dyn std::fmt::Display]) -> String {
    let mut parts = text.split("{}");
    let mut filled = parts.next().unwrap_or_default().to_owned();
    for (part, value) in parts.zip(values) {
        filled += &value.to_string();
        filled += part;
    }
    filled
}
