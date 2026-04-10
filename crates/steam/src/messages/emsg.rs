/// Steam network message types.
///
/// Values sourced from SteamKit2's `emsg.steamd`. Only the subset needed for
/// depot downloading is included here; expand as needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct EMsg(pub u32);

impl EMsg {
    pub const INVALID: Self = Self(0);
    pub const MULTI: Self = Self(1);
    pub const SERVICE_METHOD: Self = Self(146);
    pub const SERVICE_METHOD_RESPONSE: Self = Self(147);
    pub const SERVICE_METHOD_CALL_FROM_CLIENT: Self = Self(151);
    pub const SERVICE_METHOD_SEND_TO_CLIENT: Self = Self(152);

    pub const CLIENT_HEART_BEAT: Self = Self(703);
    pub const CLIENT_LOGOFF: Self = Self(706);
    pub const CLIENT_GAMES_PLAYED: Self = Self(742);
    pub const CLIENT_LOG_ON_RESPONSE: Self = Self(751);
    pub const CLIENT_SET_HEARTBEAT_RATE: Self = Self(755);
    pub const CLIENT_LOGGED_OFF: Self = Self(757);
    pub const CLIENT_PERSONA_STATE: Self = Self(766);
    pub const CLIENT_FRIENDS_LIST: Self = Self(767);
    pub const CLIENT_ACCOUNT_INFO: Self = Self(768);
    pub const CLIENT_LICENSE_LIST: Self = Self(780);
    pub const CLIENT_PING: Self = Self(764);
    pub const CLIENT_GET_APP_OWNERSHIP_TICKET: Self = Self(857);
    pub const CLIENT_GET_APP_OWNERSHIP_TICKET_RESPONSE: Self = Self(858);

    pub const CHANNEL_ENCRYPT_REQUEST: Self = Self(1303);
    pub const CHANNEL_ENCRYPT_RESPONSE: Self = Self(1304);
    pub const CHANNEL_ENCRYPT_RESULT: Self = Self(1305);

    pub const CLIENT_LOGON: Self = Self(5514);

    pub const CLIENT_CHECK_APP_BETA_PASSWORD: Self = Self(5450);
    pub const CLIENT_CHECK_APP_BETA_PASSWORD_RESPONSE: Self = Self(5451);
    pub const CLIENT_GET_DEPOT_DECRYPTION_KEY: Self = Self(5438);
    pub const CLIENT_GET_DEPOT_DECRYPTION_KEY_RESPONSE: Self = Self(5439);
    pub const CLIENT_GET_CDN_AUTH_TOKEN: Self = Self(5546);
    pub const CLIENT_GET_CDN_AUTH_TOKEN_RESPONSE: Self = Self(5547);

    pub const CLIENT_PICS_CHANGES_SINCE_REQUEST: Self = Self(8901);
    pub const CLIENT_PICS_CHANGES_SINCE_RESPONSE: Self = Self(8902);
    pub const CLIENT_PICS_PRODUCT_INFO_REQUEST: Self = Self(8903);
    pub const CLIENT_PICS_PRODUCT_INFO_RESPONSE: Self = Self(8904);
    pub const CLIENT_PICS_ACCESS_TOKEN_REQUEST: Self = Self(8905);
    pub const CLIENT_PICS_ACCESS_TOKEN_RESPONSE: Self = Self(8906);

    pub const SERVICE_METHOD_CALL_FROM_CLIENT_NON_AUTHED: Self = Self(9804);
    pub const CLIENT_HELLO: Self = Self(9805);
}

impl From<u32> for EMsg {
    fn from(v: u32) -> Self {
        Self(v)
    }
}

impl From<EMsg> for u32 {
    fn from(v: EMsg) -> Self {
        v.0
    }
}

impl std::fmt::Display for EMsg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Named display for known values, numeric for unknown
        let name = match *self {
            Self::INVALID => "Invalid",
            Self::MULTI => "Multi",
            Self::SERVICE_METHOD => "ServiceMethod",
            Self::SERVICE_METHOD_RESPONSE => "ServiceMethodResponse",
            Self::SERVICE_METHOD_CALL_FROM_CLIENT => "ServiceMethodCallFromClient",
            Self::SERVICE_METHOD_SEND_TO_CLIENT => "ServiceMethodSendToClient",
            Self::CLIENT_HEART_BEAT => "ClientHeartBeat",
            Self::CLIENT_LOGOFF => "ClientLogOff",
            Self::CLIENT_LOG_ON_RESPONSE => "ClientLogOnResponse",
            Self::CLIENT_LOGGED_OFF => "ClientLoggedOff",
            Self::CLIENT_LICENSE_LIST => "ClientLicenseList",
            Self::CLIENT_ACCOUNT_INFO => "ClientAccountInfo",
            Self::CHANNEL_ENCRYPT_REQUEST => "ChannelEncryptRequest",
            Self::CHANNEL_ENCRYPT_RESPONSE => "ChannelEncryptResponse",
            Self::CHANNEL_ENCRYPT_RESULT => "ChannelEncryptResult",
            Self::CLIENT_CHECK_APP_BETA_PASSWORD => "ClientCheckAppBetaPassword",
            Self::CLIENT_CHECK_APP_BETA_PASSWORD_RESPONSE => "ClientCheckAppBetaPasswordResponse",
            Self::CLIENT_LOGON => "ClientLogon",
            Self::CLIENT_GET_DEPOT_DECRYPTION_KEY => "ClientGetDepotDecryptionKey",
            Self::CLIENT_GET_DEPOT_DECRYPTION_KEY_RESPONSE => "ClientGetDepotDecryptionKeyResponse",
            Self::CLIENT_GET_CDN_AUTH_TOKEN => "ClientGetCDNAuthToken",
            Self::CLIENT_GET_CDN_AUTH_TOKEN_RESPONSE => "ClientGetCDNAuthTokenResponse",
            Self::CLIENT_PICS_PRODUCT_INFO_REQUEST => "ClientPICSProductInfoRequest",
            Self::CLIENT_PICS_PRODUCT_INFO_RESPONSE => "ClientPICSProductInfoResponse",
            Self::CLIENT_PICS_ACCESS_TOKEN_REQUEST => "ClientPICSAccessTokenRequest",
            Self::CLIENT_PICS_ACCESS_TOKEN_RESPONSE => "ClientPICSAccessTokenResponse",
            Self::CLIENT_HELLO => "ClientHello",
            _ => return write!(f, "EMsg({})", self.0),
        };
        write!(f, "{name}")
    }
}
