use std::fmt;

/// A Steam ID packed into a 64-bit integer.
///
/// Layout (LSB to MSB):
/// - Bits 0..31:  Account ID (32 bits)
/// - Bits 32..51: Instance (20 bits)
/// - Bits 52..55: Account Type (4 bits)
/// - Bits 56..59: Universe (4 bits)
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SteamId(u64);

impl SteamId {
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }

    pub const fn account_id(self) -> u32 {
        self.0 as u32
    }

    pub const fn instance(self) -> u32 {
        ((self.0 >> 32) & 0xF_FFFF) as u32
    }

    pub const fn account_type(self) -> u8 {
        ((self.0 >> 52) & 0xF) as u8
    }

    pub const fn universe(self) -> u8 {
        ((self.0 >> 56) & 0xF) as u8
    }

    pub const fn from_parts(
        universe: u8,
        account_type: u8,
        instance: u32,
        account_id: u32,
    ) -> Self {
        let mut id = account_id as u64;
        id |= (instance as u64 & 0xF_FFFF) << 32;
        id |= (account_type as u64 & 0xF) << 52;
        id |= (universe as u64 & 0xF) << 56;
        Self(id)
    }
}

impl fmt::Debug for SteamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SteamId({} | U:{} T:{} I:{} A:{})",
            self.0,
            self.universe(),
            self.account_type(),
            self.instance(),
            self.account_id()
        )
    }
}

impl fmt::Display for SteamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Steam3 format: [U:1:account_id]
        let letter = match self.account_type() {
            0 => 'I',  // Invalid
            1 => 'U',  // Individual
            2 => 'M',  // Multiseat
            3 => 'G',  // GameServer
            4 => 'A',  // AnonGameServer
            5 => 'P',  // Pending
            6 => 'C',  // ContentServer
            7 => 'g',  // Clan
            8 => 'T',  // Chat (clan)
            10 => 'a', // AnonUser
            _ => '?',
        };
        write!(f, "[{}:{}:{}]", letter, self.universe(), self.account_id())
    }
}

impl From<u64> for SteamId {
    fn from(raw: u64) -> Self {
        Self(raw)
    }
}

impl From<SteamId> for u64 {
    fn from(id: SteamId) -> u64 {
        id.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_parts() {
        let id = SteamId::from_parts(1, 1, 1, 12345678);
        assert_eq!(id.universe(), 1);
        assert_eq!(id.account_type(), 1);
        assert_eq!(id.instance(), 1);
        assert_eq!(id.account_id(), 12345678);
    }

    #[test]
    fn display_steam3() {
        let id = SteamId::from_parts(1, 1, 1, 12345678);
        assert_eq!(id.to_string(), "[U:1:12345678]");
    }
}
