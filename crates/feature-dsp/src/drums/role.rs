//! Responsibility: names the drum-kit pieces a groove can address.
//!
//! Grooves never name a kit's note numbers: they name a role, and each kit
//! maps its own instruments to roles. A role the kit lacks falls back to the
//! closest piece it has, so any groove plays on any kit.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DrumRole {
    Kick,
    Snare,
    SnareRim,
    SideStick,
    Clap,
    HatClosed,
    HatPedal,
    HatOpen,
    TomHigh,
    TomMid,
    TomFloor,
    Crash,
    Crash2,
    China,
    Splash,
    Ride,
    RideBell,
    Tambourine,
    Cowbell,
}

impl DrumRole {
    /// Every role, in discriminant order (so `ALL[r.index()] == r`).
    pub const ALL: [DrumRole; 19] = [
        DrumRole::Kick,
        DrumRole::Snare,
        DrumRole::SnareRim,
        DrumRole::SideStick,
        DrumRole::Clap,
        DrumRole::HatClosed,
        DrumRole::HatPedal,
        DrumRole::HatOpen,
        DrumRole::TomHigh,
        DrumRole::TomMid,
        DrumRole::TomFloor,
        DrumRole::Crash,
        DrumRole::Crash2,
        DrumRole::China,
        DrumRole::Splash,
        DrumRole::Ride,
        DrumRole::RideBell,
        DrumRole::Tambourine,
        DrumRole::Cowbell,
    ];

    pub const COUNT: usize = Self::ALL.len();

    pub fn index(self) -> usize {
        self as usize
    }

    /// Stable key used in kit and groove files.
    pub fn key(self) -> &'static str {
        match self {
            DrumRole::Kick => "kick",
            DrumRole::Snare => "snare",
            DrumRole::SnareRim => "snare_rim",
            DrumRole::SideStick => "side_stick",
            DrumRole::Clap => "clap",
            DrumRole::HatClosed => "hat_closed",
            DrumRole::HatPedal => "hat_pedal",
            DrumRole::HatOpen => "hat_open",
            DrumRole::TomHigh => "tom_high",
            DrumRole::TomMid => "tom_mid",
            DrumRole::TomFloor => "tom_floor",
            DrumRole::Crash => "crash",
            DrumRole::Crash2 => "crash2",
            DrumRole::China => "china",
            DrumRole::Splash => "splash",
            DrumRole::Ride => "ride",
            DrumRole::RideBell => "ride_bell",
            DrumRole::Tambourine => "tambourine",
            DrumRole::Cowbell => "cowbell",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|role| role.key() == key)
    }

    /// The role a General MIDI percussion note plays, including the extra
    /// hi-hat edge notes (22, 26) found in recorded drum performances.
    pub fn from_general_midi(note: u8) -> Option<Self> {
        Some(match note {
            35 | 36 => DrumRole::Kick,
            37 => DrumRole::SideStick,
            38 => DrumRole::Snare,
            39 => DrumRole::Clap,
            40 => DrumRole::SnareRim,
            41 | 43 => DrumRole::TomFloor,
            22 | 42 => DrumRole::HatClosed,
            44 => DrumRole::HatPedal,
            26 | 46 => DrumRole::HatOpen,
            45 | 47 => DrumRole::TomMid,
            48 | 50 => DrumRole::TomHigh,
            49 | 58 => DrumRole::Crash,
            51 | 59 => DrumRole::Ride,
            52 => DrumRole::China,
            53 => DrumRole::RideBell,
            54 => DrumRole::Tambourine,
            55 => DrumRole::Splash,
            56 => DrumRole::Cowbell,
            57 => DrumRole::Crash2,
            _ => return None,
        })
    }

    /// The piece to play when a kit has no sample for this role.
    pub fn fallback(self) -> Option<Self> {
        match self {
            DrumRole::SnareRim | DrumRole::Clap => Some(DrumRole::Snare),
            DrumRole::SideStick => Some(DrumRole::SnareRim),
            DrumRole::HatPedal | DrumRole::HatOpen => Some(DrumRole::HatClosed),
            DrumRole::TomHigh => Some(DrumRole::TomMid),
            DrumRole::TomMid => Some(DrumRole::TomFloor),
            DrumRole::Crash2 | DrumRole::China | DrumRole::Splash => Some(DrumRole::Crash),
            DrumRole::RideBell => Some(DrumRole::Ride),
            DrumRole::Ride => Some(DrumRole::Crash),
            DrumRole::Kick
            | DrumRole::Snare
            | DrumRole::HatClosed
            | DrumRole::TomFloor
            | DrumRole::Crash
            | DrumRole::Tambourine
            | DrumRole::Cowbell => None,
        }
    }
}
