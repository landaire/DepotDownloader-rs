use steam::enums::EResultError;

#[test]
fn success_returns_ok() {
    assert!(EResultError::from_i32(1).is_ok());
}

#[test]
fn zero_is_invalid() {
    assert_eq!(
        EResultError::from_i32(0).unwrap_err(),
        EResultError::Invalid
    );
}

#[test]
fn known_error_codes() {
    let cases: &[(i32, EResultError)] = &[
        (2, EResultError::Fail),
        (3, EResultError::NoConnection),
        (5, EResultError::InvalidPassword),
        (6, EResultError::LoggedInElsewhere),
        (15, EResultError::AccessDenied),
        (27, EResultError::Expired),
        (84, EResultError::RateLimitExceeded),
        (85, EResultError::TwoFactorRequired),
        (87, EResultError::LoginDeniedThrottle),
        (88, EResultError::TwoFactorCodeMismatch),
    ];

    for &(code, ref expected) in cases {
        let err = EResultError::from_i32(code).unwrap_err();
        assert_eq!(&err, expected, "EResult code {code}");
    }
}

#[test]
fn unknown_code_preserved() {
    let err = EResultError::from_i32(9999).unwrap_err();
    assert_eq!(err, EResultError::Unknown(9999));
}

#[test]
fn negative_code_is_unknown() {
    let err = EResultError::from_i32(-1).unwrap_err();
    assert_eq!(err, EResultError::Unknown(-1));
}
