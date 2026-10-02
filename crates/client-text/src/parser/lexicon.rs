//! Lexicon, vocabulary definitions, and parts of speech.

use tor_protocol::Direction;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    Look,
    Examine,
    Read,
    Inventory,
    Take,
    Drop,
    Put,
    Open,
    Close,
    Unlock,
    Lock,
    Attack,
    Go,
    Climb,
    Enter,
    Exit,
    Wait,
    Help,
    Quit,
    Stop,
    Step,
    Say,
    Again,
    Listen,
    Smell,
    Touch,
    Search,
    Verbose,
    Brief,
    Superbrief,
    Drink,
    Eat,
    Wear,
    Wield,
    Remove,
    Give,
    Show,
    Throw,
    Fire,
    Push,
    Pull,
    Turn,
    Kick,
    Break,
    Cut,
    Burn,
    Light,
    Extinguish,
    Dig,
    Fill,
    Pour,
    Apply,
    Engrave,
    Zap,
    Rub,
    Tie,
    Untie,
    Wave,
    Knock,
    Sit,
    Jump,
    Swim,
    Sleep,
    Pray,
    Talk,
    Ask,
    Tell,
    Diagnose,
}

impl Verb {
    pub fn as_str(&self) -> &'static str {
        match self {
            Verb::Look => "look",
            Verb::Examine => "examine",
            Verb::Read => "read",
            Verb::Inventory => "inventory",
            Verb::Take => "take",
            Verb::Drop => "drop",
            Verb::Put => "put",
            Verb::Open => "open",
            Verb::Close => "close",
            Verb::Unlock => "unlock",
            Verb::Lock => "lock",
            Verb::Attack => "attack",
            Verb::Go => "go",
            Verb::Climb => "climb",
            Verb::Enter => "enter",
            Verb::Exit => "exit",
            Verb::Wait => "wait",
            Verb::Help => "help",
            Verb::Quit => "quit",
            Verb::Stop => "stop",
            Verb::Step => "step",
            Verb::Say => "say",
            Verb::Again => "again",
            Verb::Listen => "listen",
            Verb::Smell => "smell",
            Verb::Touch => "touch",
            Verb::Search => "search",
            Verb::Verbose => "verbose",
            Verb::Brief => "brief",
            Verb::Superbrief => "superbrief",
            Verb::Drink => "drink",
            Verb::Eat => "eat",
            Verb::Wear => "wear",
            Verb::Wield => "wield",
            Verb::Remove => "remove",
            Verb::Give => "give",
            Verb::Show => "show",
            Verb::Throw => "throw",
            Verb::Fire => "fire",
            Verb::Push => "push",
            Verb::Pull => "pull",
            Verb::Turn => "turn",
            Verb::Kick => "kick",
            Verb::Break => "break",
            Verb::Cut => "cut",
            Verb::Burn => "burn",
            Verb::Light => "light",
            Verb::Extinguish => "extinguish",
            Verb::Dig => "dig",
            Verb::Fill => "fill",
            Verb::Pour => "pour",
            Verb::Apply => "use",
            Verb::Engrave => "engrave",
            Verb::Zap => "zap",
            Verb::Rub => "rub",
            Verb::Tie => "tie",
            Verb::Untie => "untie",
            Verb::Wave => "wave",
            Verb::Knock => "knock",
            Verb::Sit => "sit",
            Verb::Jump => "jump",
            Verb::Swim => "swim",
            Verb::Sleep => "sleep",
            Verb::Pray => "pray",
            Verb::Talk => "talk",
            Verb::Ask => "ask",
            Verb::Tell => "tell",
            Verb::Diagnose => "diagnose",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preposition {
    At,
    In,
    On,
    Under,
    Behind,
    With,
    From,
    To,
    Through,
    Off,
    About,
}

impl Preposition {
    pub fn as_str(&self) -> &'static str {
        match self {
            Preposition::At => "at",
            Preposition::In => "in",
            Preposition::On => "on",
            Preposition::Under => "under",
            Preposition::Behind => "behind",
            Preposition::With => "with",
            Preposition::From => "from",
            Preposition::To => "to",
            Preposition::Through => "through",
            Preposition::Off => "off",
            Preposition::About => "about",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pronoun {
    It,
    Them,
    Him,
    Her,
}

pub fn parse_verb(word: &str) -> Option<Verb> {
    match word {
        "look" | "l" => Some(Verb::Look),
        "examine" | "x" | "inspect" | "study" | "check" | "describe" => Some(Verb::Examine),
        "read" | "peruse" | "skim" => Some(Verb::Read),
        "inventory" | "i" | "inv" => Some(Verb::Inventory),
        "take" | "get" | "grab" | "carry" | "acquire" | "collect" | "hold" | "lift" => {
            Some(Verb::Take)
        }
        "drop" | "discard" | "leave" | "release" => Some(Verb::Drop),
        "put" | "place" | "insert" | "set" | "stow" => Some(Verb::Put),
        "open" => Some(Verb::Open),
        "close" | "shut" | "slam" => Some(Verb::Close),
        "unlock" => Some(Verb::Unlock),
        "lock" => Some(Verb::Lock),
        "attack" | "hit" | "fight" | "strike" | "slay" | "kill" | "smite" | "slash" | "stab"
        | "murder" | "punch" | "assault" => Some(Verb::Attack),
        "go" | "walk" | "head" | "run" | "travel" | "proceed" | "move" => Some(Verb::Go),
        "climb" | "ascend" | "descend" | "scale" => Some(Verb::Climb),
        "enter" => Some(Verb::Enter),
        "exit" | "out" => Some(Verb::Exit),
        "wait" | "z" | "pause" | "rest" => Some(Verb::Wait),
        "help" | "?" | "hint" | "hints" => Some(Verb::Help),
        "quit" | "q" => Some(Verb::Quit),
        "stop" | "cancel" | "halt" => Some(Verb::Stop),
        "step" => Some(Verb::Step),
        "say" | "shout" | "whisper" | "yell" | "scream" | "sing" => Some(Verb::Say),
        "again" | "g" => Some(Verb::Again),
        "listen" | "hear" => Some(Verb::Listen),
        "smell" | "sniff" => Some(Verb::Smell),
        "touch" | "feel" | "pat" | "stroke" => Some(Verb::Touch),
        "search" | "seek" => Some(Verb::Search),
        "verbose" => Some(Verb::Verbose),
        "brief" => Some(Verb::Brief),
        "superbrief" => Some(Verb::Superbrief),
        "drink" | "quaff" | "sip" | "swallow" => Some(Verb::Drink),
        "eat" | "taste" | "consume" | "devour" | "bite" => Some(Verb::Eat),
        "wear" | "don" => Some(Verb::Wear),
        "wield" | "equip" | "brandish" | "ready" => Some(Verb::Wield),
        "remove" | "doff" | "unequip" | "unwield" => Some(Verb::Remove),
        "give" | "offer" | "hand" | "feed" => Some(Verb::Give),
        "show" | "display" => Some(Verb::Show),
        "throw" | "toss" | "hurl" | "fling" | "chuck" => Some(Verb::Throw),
        "fire" | "shoot" => Some(Verb::Fire),
        "push" | "shove" | "press" => Some(Verb::Push),
        "pull" | "drag" | "tug" | "yank" => Some(Verb::Pull),
        "turn" | "rotate" | "twist" | "spin" => Some(Verb::Turn),
        "kick" => Some(Verb::Kick),
        "break" | "smash" | "shatter" | "destroy" | "wreck" => Some(Verb::Break),
        "cut" | "slice" | "chop" | "carve" => Some(Verb::Cut),
        "burn" | "ignite" => Some(Verb::Burn),
        "light" | "kindle" => Some(Verb::Light),
        "extinguish" | "douse" | "snuff" | "quench" => Some(Verb::Extinguish),
        "dig" | "excavate" => Some(Verb::Dig),
        "fill" | "refill" => Some(Verb::Fill),
        "pour" | "spill" | "empty" => Some(Verb::Pour),
        "use" | "apply" | "employ" | "activate" => Some(Verb::Apply),
        "engrave" | "inscribe" | "write" | "scrawl" => Some(Verb::Engrave),
        "zap" | "invoke" => Some(Verb::Zap),
        "rub" | "polish" | "wipe" | "clean" => Some(Verb::Rub),
        "tie" | "fasten" | "attach" | "bind" => Some(Verb::Tie),
        "untie" | "unfasten" | "detach" | "unbind" => Some(Verb::Untie),
        "wave" => Some(Verb::Wave),
        "knock" | "rap" => Some(Verb::Knock),
        "sit" | "kneel" => Some(Verb::Sit),
        "jump" | "leap" | "hop" => Some(Verb::Jump),
        "swim" | "dive" | "wade" => Some(Verb::Swim),
        "sleep" | "nap" | "doze" => Some(Verb::Sleep),
        "pray" | "worship" => Some(Verb::Pray),
        "talk" | "speak" | "chat" | "converse" | "greet" | "hello" | "hi" => Some(Verb::Talk),
        "ask" | "question" | "query" => Some(Verb::Ask),
        "tell" | "inform" => Some(Verb::Tell),
        "diagnose" | "health" | "status" => Some(Verb::Diagnose),
        _ => None,
    }
}

pub fn parse_direction(word: &str) -> Option<Direction> {
    match word {
        "north" | "n" => Some(Direction::North),
        "east" | "e" => Some(Direction::East),
        "south" | "s" => Some(Direction::South),
        "west" | "w" => Some(Direction::West),
        "northeast" | "ne" => Some(Direction::NorthEast),
        "southeast" | "se" => Some(Direction::SouthEast),
        "southwest" | "sw" => Some(Direction::SouthWest),
        "northwest" | "nw" => Some(Direction::NorthWest),
        "up" | "u" | "upward" | "upwards" => Some(Direction::Up),
        "down" | "d" | "downward" | "downwards" => Some(Direction::Down),
        _ => None,
    }
}

pub fn parse_preposition(word: &str) -> Option<Preposition> {
    match word {
        "at" => Some(Preposition::At),
        "in" | "into" | "inside" => Some(Preposition::In),
        "on" | "onto" | "upon" => Some(Preposition::On),
        "under" | "underneath" | "beneath" | "below" => Some(Preposition::Under),
        "behind" => Some(Preposition::Behind),
        "with" | "using" => Some(Preposition::With),
        "from" => Some(Preposition::From),
        "to" | "toward" | "towards" => Some(Preposition::To),
        "through" => Some(Preposition::Through),
        "off" => Some(Preposition::Off),
        "about" => Some(Preposition::About),
        _ => None,
    }
}

pub fn parse_pronoun(word: &str) -> Option<Pronoun> {
    match word {
        "it" => Some(Pronoun::It),
        "them" | "these" | "those" => Some(Pronoun::Them),
        "him" => Some(Pronoun::Him),
        "her" => Some(Pronoun::Her),
        _ => None,
    }
}

pub fn parse_ordinal(word: &str) -> Option<usize> {
    match word {
        "first" | "1st" => Some(1),
        "second" | "2nd" => Some(2),
        "third" | "3rd" => Some(3),
        "fourth" | "4th" => Some(4),
        "fifth" | "5th" => Some(5),
        "sixth" | "6th" => Some(6),
        "seventh" | "7th" => Some(7),
        "eighth" | "8th" => Some(8),
        "ninth" | "9th" => Some(9),
        "tenth" | "10th" => Some(10),
        _ => None,
    }
}

pub fn is_determiner(word: &str) -> bool {
    matches!(
        word,
        "the"
            | "a"
            | "an"
            | "some"
            | "this"
            | "that"
            | "these"
            | "those"
            | "my"
            | "your"
            | "our"
            | "their"
            | "its"
    )
}

pub fn is_all(word: &str) -> bool {
    matches!(word, "all" | "everything")
}

pub fn is_except(word: &str) -> bool {
    matches!(word, "except" | "but")
}
