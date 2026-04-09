/// Steam API result codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum EResult {
    Invalid = 0,
    OK = 1,
    Fail = 2,
    NoConnection = 3,
    InvalidPassword = 5,
    LoggedInElsewhere = 6,
    InvalidProtocolVer = 7,
    InvalidParam = 8,
    FileNotFound = 9,
    Busy = 10,
    InvalidState = 11,
    InvalidName = 12,
    InvalidEmail = 13,
    DuplicateName = 14,
    AccessDenied = 15,
    Timeout = 16,
    Banned = 17,
    AccountNotFound = 18,
    InvalidSteamID = 19,
    ServiceUnavailable = 20,
    NotLoggedOn = 21,
    Pending = 22,
    EncryptionFailure = 23,
    InsufficientPrivilege = 24,
    LimitExceeded = 25,
    Revoked = 26,
    Expired = 27,
    AlreadyRedeemed = 28,
    DuplicateRequest = 29,
    AlreadyOwned = 30,
    IPNotFound = 31,
    PersistFailed = 32,
    LockingFailed = 33,
    LogonSessionReplaced = 34,
    AccountLoginDeniedNeedTwoFactor = 85,
    AccountLoginDeniedThrottle = 87,
    TwoFactorCodeMismatch = 88,
    TwoFactorActivationCodeMismatch = 89,
    RateLimitExceeded = 84,
}

impl EResult {
    pub fn from_i32(v: i32) -> Option<Self> {
        Some(match v {
            0 => Self::Invalid,
            1 => Self::OK,
            2 => Self::Fail,
            3 => Self::NoConnection,
            5 => Self::InvalidPassword,
            6 => Self::LoggedInElsewhere,
            7 => Self::InvalidProtocolVer,
            8 => Self::InvalidParam,
            9 => Self::FileNotFound,
            10 => Self::Busy,
            11 => Self::InvalidState,
            12 => Self::InvalidName,
            13 => Self::InvalidEmail,
            14 => Self::DuplicateName,
            15 => Self::AccessDenied,
            16 => Self::Timeout,
            17 => Self::Banned,
            18 => Self::AccountNotFound,
            19 => Self::InvalidSteamID,
            20 => Self::ServiceUnavailable,
            21 => Self::NotLoggedOn,
            22 => Self::Pending,
            23 => Self::EncryptionFailure,
            24 => Self::InsufficientPrivilege,
            25 => Self::LimitExceeded,
            26 => Self::Revoked,
            27 => Self::Expired,
            28 => Self::AlreadyRedeemed,
            29 => Self::DuplicateRequest,
            30 => Self::AlreadyOwned,
            31 => Self::IPNotFound,
            32 => Self::PersistFailed,
            33 => Self::LockingFailed,
            34 => Self::LogonSessionReplaced,
            84 => Self::RateLimitExceeded,
            85 => Self::AccountLoginDeniedNeedTwoFactor,
            87 => Self::AccountLoginDeniedThrottle,
            88 => Self::TwoFactorCodeMismatch,
            89 => Self::TwoFactorActivationCodeMismatch,
            _ => return None,
        })
    }

    pub fn is_ok(self) -> bool {
        self == Self::OK
    }
}

impl std::fmt::Display for EResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} ({})", self, *self as i32)
    }
}

/// Depot file flags from the manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DepotFileFlags(pub u32);

impl DepotFileFlags {
    pub const NONE: u32 = 0x00;
    pub const DIRECTORY: u32 = 0x40;
    pub const EXECUTABLE: u32 = 0x04;
    pub const HIDDEN: u32 = 0x80;
    pub const READ_ONLY: u32 = 0x100;

    pub fn is_directory(self) -> bool {
        self.0 & Self::DIRECTORY != 0
    }

    pub fn is_executable(self) -> bool {
        self.0 & Self::EXECUTABLE != 0
    }
}

/// Steam account types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EAccountType {
    Invalid = 0,
    Individual = 1,
    Multiseat = 2,
    GameServer = 3,
    AnonGameServer = 4,
    Pending = 5,
    ContentServer = 6,
    Clan = 7,
    Chat = 8,
    AnonUser = 10,
}

/// Steam universes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EUniverse {
    Invalid = 0,
    Public = 1,
    Beta = 2,
    Internal = 3,
    Dev = 4,
}

/// Client OS type for logon messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum EOSType {
    LinuxUnknown = -203,
    MacOSUnknown = -102,
    WindowsUnknown = 0,
    Windows11 = 203,
    Windows10 = 202,
}

/// Manifest section magic values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ManifestMagic {
    PayloadV5 = 0x71F6_17D0,
    Metadata = 0x1F48_12BE,
    Signature = 0x1B81_B817,
    EndOfManifest = 0x32C4_15AB,
    V4 = 0x1634_9781,
}

impl ManifestMagic {
    pub fn from_u32(v: u32) -> Option<Self> {
        Some(match v {
            0x71F6_17D0 => Self::PayloadV5,
            0x1F48_12BE => Self::Metadata,
            0x1B81_B817 => Self::Signature,
            0x32C4_15AB => Self::EndOfManifest,
            0x1634_9781 => Self::V4,
            _ => return None,
        })
    }
}
