/// Steam API error result codes. Success (EResult=1) is not represented -
/// use `EResultError::from_i32(v)` which returns `Ok(())` for success
/// and `Err(EResultError)` for any failure code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EResultError {
    #[error("invalid (0)")]
    Invalid,
    #[error("fail (2)")]
    Fail,
    #[error("no connection (3)")]
    NoConnection,
    #[error("invalid password (5)")]
    InvalidPassword,
    #[error("logged in elsewhere (6)")]
    LoggedInElsewhere,
    #[error("invalid protocol version (7)")]
    InvalidProtocolVer,
    #[error("invalid param (8)")]
    InvalidParam,
    #[error("file not found (9)")]
    FileNotFound,
    #[error("busy (10)")]
    Busy,
    #[error("invalid state (11)")]
    InvalidState,
    #[error("invalid name (12)")]
    InvalidName,
    #[error("invalid email (13)")]
    InvalidEmail,
    #[error("duplicate name (14)")]
    DuplicateName,
    #[error("access denied (15)")]
    AccessDenied,
    #[error("timeout (16)")]
    Timeout,
    #[error("banned (17)")]
    Banned,
    #[error("account not found (18)")]
    AccountNotFound,
    #[error("invalid steam ID (19)")]
    InvalidSteamID,
    #[error("service unavailable (20)")]
    ServiceUnavailable,
    #[error("not logged on (21)")]
    NotLoggedOn,
    #[error("pending (22)")]
    Pending,
    #[error("encryption failure (23)")]
    EncryptionFailure,
    #[error("insufficient privilege (24)")]
    InsufficientPrivilege,
    #[error("limit exceeded (25)")]
    LimitExceeded,
    #[error("revoked (26)")]
    Revoked,
    #[error("expired (27)")]
    Expired,
    #[error("already redeemed (28)")]
    AlreadyRedeemed,
    #[error("duplicate request (29)")]
    DuplicateRequest,
    #[error("already owned (30)")]
    AlreadyOwned,
    #[error("IP not found (31)")]
    IPNotFound,
    #[error("persist failed (32)")]
    PersistFailed,
    #[error("locking failed (33)")]
    LockingFailed,
    #[error("logon session replaced (34)")]
    LogonSessionReplaced,
    #[error("rate limit exceeded (84)")]
    RateLimitExceeded,
    #[error("two factor required (85)")]
    TwoFactorRequired,
    #[error("login denied, throttled (87)")]
    LoginDeniedThrottle,
    #[error("two factor code mismatch (88)")]
    TwoFactorCodeMismatch,
    #[error("two factor activation code mismatch (89)")]
    TwoFactorActivationCodeMismatch,
    #[error("unknown error ({0})")]
    Unknown(i32),
}

impl EResultError {
    /// Returns `Ok(())` for success (1), `Err` for everything else.
    pub fn from_i32(v: i32) -> Result<(), Self> {
        match v {
            1 => Ok(()),
            0 => Err(Self::Invalid),
            2 => Err(Self::Fail),
            3 => Err(Self::NoConnection),
            5 => Err(Self::InvalidPassword),
            6 => Err(Self::LoggedInElsewhere),
            7 => Err(Self::InvalidProtocolVer),
            8 => Err(Self::InvalidParam),
            9 => Err(Self::FileNotFound),
            10 => Err(Self::Busy),
            11 => Err(Self::InvalidState),
            12 => Err(Self::InvalidName),
            13 => Err(Self::InvalidEmail),
            14 => Err(Self::DuplicateName),
            15 => Err(Self::AccessDenied),
            16 => Err(Self::Timeout),
            17 => Err(Self::Banned),
            18 => Err(Self::AccountNotFound),
            19 => Err(Self::InvalidSteamID),
            20 => Err(Self::ServiceUnavailable),
            21 => Err(Self::NotLoggedOn),
            22 => Err(Self::Pending),
            23 => Err(Self::EncryptionFailure),
            24 => Err(Self::InsufficientPrivilege),
            25 => Err(Self::LimitExceeded),
            26 => Err(Self::Revoked),
            27 => Err(Self::Expired),
            28 => Err(Self::AlreadyRedeemed),
            29 => Err(Self::DuplicateRequest),
            30 => Err(Self::AlreadyOwned),
            31 => Err(Self::IPNotFound),
            32 => Err(Self::PersistFailed),
            33 => Err(Self::LockingFailed),
            34 => Err(Self::LogonSessionReplaced),
            84 => Err(Self::RateLimitExceeded),
            85 => Err(Self::TwoFactorRequired),
            87 => Err(Self::LoginDeniedThrottle),
            88 => Err(Self::TwoFactorCodeMismatch),
            89 => Err(Self::TwoFactorActivationCodeMismatch),
            other => Err(Self::Unknown(other)),
        }
    }
}

/// Depot file flags from the manifest (bitfield).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DepotFileFlags(pub u32);

impl DepotFileFlags {
    pub const NONE: Self = Self(0x00);
    pub const EXECUTABLE: Self = Self(0x04);
    pub const DIRECTORY: Self = Self(0x40);
    pub const HIDDEN: Self = Self(0x80);
    pub const READ_ONLY: Self = Self(0x100);

    pub fn contains(self, flag: Self) -> bool {
        self.0 & flag.0 != 0
    }

    pub fn is_directory(self) -> bool {
        self.contains(Self::DIRECTORY)
    }

    pub fn is_executable(self) -> bool {
        self.contains(Self::EXECUTABLE)
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

/// Auth token platform type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum EAuthTokenPlatformType {
    Unknown = 0,
    SteamClient = 1,
    WebBrowser = 2,
    MobileApp = 3,
}

/// Session persistence mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ESessionPersistence {
    Ephemeral = 0,
    Persistent = 1,
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
