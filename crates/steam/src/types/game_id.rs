use std::fmt;

/// A Steam Game ID packed into a 64-bit integer.
///
/// Layout (LSB to MSB):
/// - Bits 0..23:  App ID (24 bits)
/// - Bits 24..31: Type (8 bits)
/// - Bits 32..63: Mod ID (32 bits)
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct GameId(u64);

impl GameId {
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn from_app_id(app_id: u32) -> Self {
        // Type = 1 (app), mod_id = 0
        let raw = (app_id as u64 & 0xFF_FFFF) | (1u64 << 24);
        Self(raw)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }

    pub const fn app_id(self) -> u32 {
        (self.0 & 0xFF_FFFF) as u32
    }

    pub const fn game_type(self) -> u8 {
        ((self.0 >> 24) & 0xFF) as u8
    }

    pub const fn mod_id(self) -> u32 {
        (self.0 >> 32) as u32
    }
}

impl fmt::Debug for GameId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "GameId(app:{} type:{} mod:{})",
            self.app_id(),
            self.game_type(),
            self.mod_id()
        )
    }
}

impl From<u64> for GameId {
    fn from(raw: u64) -> Self {
        Self(raw)
    }
}

impl From<GameId> for u64 {
    fn from(id: GameId) -> u64 {
        id.0
    }
}
