//! Lexicon, vocabulary definitions, and parts of speech.

use tor_protocol::Direction;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    Look,
    Examine,
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
    Search,
    Verbose,
    Brief,
    Superbrief,
    Read,
    Drink,
    Eat,
    Wear,
    Wield,
    Remove,
    Give,
    Push,
    Pull,
    Turn,
    Talk,
    Diagnose,
}

impl Verb {
    pub fn as_str(&self) -> &'static str {
        match self {
            Verb::Look => "look",
            Verb::Examine => "examine",
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
            Verb::Search => "search",
            Verb::Verbose => "verbose",
            Verb::Brief => "brief",
            Verb::Superbrief => "superbrief",
            Verb::Read => "read",
            Verb::Drink => "drink",
            Verb::Eat => "eat",
            Verb::Wear => "wear",
            Verb::Wield => "wield",
            Verb::Remove => "remove",
            Verb::Give => "give",
            Verb::Push => "push",
            Verb::Pull => "pull",
            Verb::Turn => "turn",
            Verb::Talk => "talk",
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
        "examine" | "x" | "inspect" | "study" | "check" => Some(Verb::Examine),
        "inventory" | "i" => Some(Verb::Inventory),
        "take" | "get" | "grab" | "carry" | "acquire" => Some(Verb::Take),
        "drop" | "discard" | "leave" => Some(Verb::Drop),
        "put" | "place" | "insert" => Some(Verb::Put),
        "open" => Some(Verb::Open),
        "close" | "shut" => Some(Verb::Close),
        "unlock" => Some(Verb::Unlock),
        "lock" => Some(Verb::Lock),
        "attack" | "hit" | "fight" | "strike" | "slay" | "kill" | "smite" | "slash" => {
            Some(Verb::Attack)
        }
        "go" | "walk" | "head" | "run" | "travel" | "proceed" => Some(Verb::Go),
        "climb" | "ascend" | "descend" => Some(Verb::Climb),
        "enter" => Some(Verb::Enter),
        "exit" => Some(Verb::Exit),
        "wait" | "z" | "pause" | "rest" => Some(Verb::Wait),
        "help" | "?" => Some(Verb::Help),
        "quit" | "q" => Some(Verb::Quit),
        "stop" | "cancel" | "halt" => Some(Verb::Stop),
        "step" => Some(Verb::Step),
        "say" | "shout" | "whisper" => Some(Verb::Say),
        "again" | "g" => Some(Verb::Again),
        "listen" | "hear" => Some(Verb::Listen),
        "smell" | "sniff" => Some(Verb::Smell),
        "search" => Some(Verb::Search),
        "verbose" => Some(Verb::Verbose),
        "brief" => Some(Verb::Brief),
        "superbrief" => Some(Verb::Superbrief),
        "read" => Some(Verb::Read),
        "drink" | "quaff" | "sip" => Some(Verb::Drink),
        "eat" | "taste" | "consume" => Some(Verb::Eat),
        "wear" | "don" => Some(Verb::Wear),
        "wield" | "equip" | "brandish" => Some(Verb::Wield),
        "remove" | "doff" | "unequip" => Some(Verb::Remove),
        "give" | "offer" | "hand" => Some(Verb::Give),
        "push" | "shove" | "press" => Some(Verb::Push),
        "pull" | "drag" | "tug" => Some(Verb::Pull),
        "turn" | "rotate" | "twist" => Some(Verb::Turn),
        "talk" | "speak" | "ask" => Some(Verb::Talk),
        "diagnose" => Some(Verb::Diagnose),
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
