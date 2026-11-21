use super::*;

fn rule_sub(sub: &str, role: UserRole) -> RoleRule {
    RoleRule {
        r#match: RuleMatch {
            sub: Some(sub.into()),
            ..Default::default()
        },
        role,
    }
}

fn base() -> SsoConfig {
    SsoConfig {
        enabled: true,
        issuer: "https://idp.example.com".into(),
        client_id: "super".into(),
        public_base_url: "https://super.example.com".into(),
        scopes: DEFAULT_SCOPES.into(),
        groups_claim: DEFAULT_GROUPS_CLAIM.into(),
        token_auth_method: TokenAuthMethod::ClientSecretBasic,
        role_rules: vec![rule_sub("u1", UserRole::Admin)],
        session_ttl_secs: DEFAULT_SESSION_TTL_SECS,
        idle_timeout_secs: DEFAULT_IDLE_TIMEOUT_SECS,
        allow_insecure_issuer: false,
    }
}

fn assert_invalid(config: &SsoConfig, needle: &str) {
    let errors = config.validate();
    assert!(
        errors.iter().any(|e| e.contains(needle)),
        "expected an error containing {needle:?}, got {errors:?}"
    );
}

#[test]
fn valid_config_has_no_errors() {
    assert_eq!(base().validate(), Vec::<String>::new());
}

#[test]
fn omitted_optional_fields_get_defaults() {
    let config: SsoConfig = serde_json::from_value(serde_json::json!({
        "enabled": true,
        "issuer": "https://idp.example.com",
        "client_id": "super",
        "public_base_url": "https://super.example.com",
        "role_rules": [{ "match": { "sub": "u1" }, "role": "admin" }]
    }))
    .unwrap();
    assert_eq!(config.scopes, DEFAULT_SCOPES);
    assert_eq!(config.groups_claim, DEFAULT_GROUPS_CLAIM);
    assert_eq!(config.token_auth_method, TokenAuthMethod::ClientSecretBasic);
    assert_eq!(config.session_ttl_secs, DEFAULT_SESSION_TTL_SECS);
    assert_eq!(config.idle_timeout_secs, DEFAULT_IDLE_TIMEOUT_SECS);
    assert!(!config.allow_insecure_issuer);
    assert!(config.validate().is_empty());
}

#[test]
fn public_issuer_must_be_https() {
    let mut config = base();
    config.issuer = "http://idp.example.com".into();
    assert_invalid(&config, "must use https");

    // Even the opt-in flag cannot downgrade a public issuer.
    config.allow_insecure_issuer = true;
    assert_invalid(&config, "must use https");
}

#[test]
fn loopback_issuer_may_be_http_only_with_the_flag() {
    let mut config = base();
    config.issuer = "http://127.0.0.1:8080".into();
    assert_invalid(&config, "must use https");

    config.allow_insecure_issuer = true;
    assert!(config.validate().is_empty(), "{:?}", config.validate());
}

#[test]
fn issuer_with_query_or_credentials_is_rejected() {
    let mut config = base();
    config.issuer = "https://idp.example.com?x=1".into();
    assert_invalid(&config, "query or fragment");

    config.issuer = "https://user:pw@idp.example.com".into();
    assert_invalid(&config, "credentials");
}

#[test]
fn base_url_must_be_an_origin() {
    let mut config = base();
    config.public_base_url = "https://super.example.com/ui".into();
    assert_invalid(&config, "without a path");

    config.public_base_url = "http://super.example.com".into();
    assert_invalid(&config, "http is only allowed for loopback");

    config.public_base_url = "http://127.0.0.1:9002".into();
    assert!(config.validate().is_empty(), "{:?}", config.validate());
}

#[test]
fn scopes_must_include_openid_and_be_sane() {
    let mut config = base();
    config.scopes = "profile email".into();
    assert_invalid(&config, "openid");

    config.scopes = "   ".into();
    assert_invalid(&config, "must not be empty");

    config.scopes = "openid \"bad".into();
    assert_invalid(&config, "invalid scope");
}

#[test]
fn ttl_bounds_and_ordering() {
    let mut config = base();
    config.session_ttl_secs = 60;
    assert_invalid(&config, "session_ttl_secs must be between");

    config.session_ttl_secs = DEFAULT_SESSION_TTL_SECS;
    config.idle_timeout_secs = DEFAULT_SESSION_TTL_SECS + 1;
    assert_invalid(&config, "must not exceed");
}

#[test]
fn enabled_needs_at_least_one_rule() {
    let mut config = base();
    config.role_rules.clear();
    assert_invalid(&config, "at least one role rule");

    config.enabled = false;
    assert!(config.validate().is_empty(), "{:?}", config.validate());
}

#[test]
fn rule_without_selectors_is_rejected() {
    let mut config = base();
    config.role_rules = vec![RoleRule {
        r#match: RuleMatch::default(),
        role: UserRole::Admin,
    }];
    assert_invalid(&config, "at least one selector");
}

#[test]
fn email_selector_must_look_like_an_email() {
    let mut config = base();
    config.role_rules = vec![RoleRule {
        r#match: RuleMatch {
            email: Some("ops".into()),
            ..Default::default()
        },
        role: UserRole::Operator,
    }];
    assert_invalid(&config, "look like an email");
}

#[test]
fn rule_json_uses_the_match_key() {
    let json = serde_json::to_value(rule_sub("u1", UserRole::Admin)).unwrap();
    assert!(json.get("match").is_some(), "{json}");
    assert!(json.get("r#match").is_none(), "{json}");
    assert_eq!(json["role"], "admin");
}

#[test]
fn redirect_uri_is_origin_plus_callback() {
    let mut config = base();
    config.public_base_url = "https://super.example.com/".into();
    assert_eq!(
        config.redirect_uri(),
        "https://super.example.com/api/v1/auth/oidc/callback"
    );
}

#[test]
fn normalized_issuer_drops_the_trailing_slash() {
    let mut config = base();
    config.issuer = "https://idp.example.com/".into();
    assert_eq!(config.normalized_issuer(), "https://idp.example.com");
}

#[test]
fn loopback_url_detection() {
    assert!(is_loopback_url("http://127.0.0.1:8080"));
    assert!(is_loopback_url("http://localhost:8080"));
    assert!(is_loopback_url("http://[::1]:8080"));
    assert!(!is_loopback_url("https://127.0.0.1"));
    assert!(!is_loopback_url("http://idp.example.com"));
}

fn claims<'a>(groups: &'a [String], email: Option<&'a str>, verified: bool) -> IdentityClaims<'a> {
    IdentityClaims {
        sub: "u1",
        email,
        email_verified: verified,
        groups,
    }
}

#[test]
fn first_matching_rule_wins() {
    let rules = vec![
        RoleRule {
            r#match: RuleMatch {
                groups_any: Some(vec!["admins".into()]),
                ..Default::default()
            },
            role: UserRole::Admin,
        },
        rule_sub("u1", UserRole::Viewer),
    ];
    let groups = vec!["admins".to_string()];
    assert_eq!(
        resolve_role(&rules, &claims(&groups, None, false)),
        Some((0, UserRole::Admin))
    );
}

#[test]
fn sub_match_is_exact_and_default_is_deny() {
    let rules = vec![rule_sub("u1", UserRole::Operator)];
    let groups: Vec<String> = Vec::new();
    assert_eq!(
        resolve_role(&rules, &claims(&groups, None, false)),
        Some((0, UserRole::Operator))
    );

    let other = IdentityClaims {
        sub: "u2",
        ..claims(&groups, None, false)
    };
    assert_eq!(resolve_role(&rules, &other), None);
}

#[test]
fn email_rules_require_a_verified_email() {
    let rules = vec![RoleRule {
        r#match: RuleMatch {
            email: Some("Ops@Example.com".into()),
            ..Default::default()
        },
        role: UserRole::Operator,
    }];
    let groups: Vec<String> = Vec::new();

    assert_eq!(
        resolve_role(&rules, &claims(&groups, Some("ops@example.com"), true)),
        Some((0, UserRole::Operator)),
        "case-insensitive match"
    );
    assert_eq!(
        resolve_role(&rules, &claims(&groups, Some("ops@example.com"), false)),
        None,
        "unverified email must not grant a role"
    );
    assert_eq!(
        resolve_role(&rules, &claims(&groups, None, true)),
        None,
        "missing email must not grant a role"
    );
}

#[test]
fn every_selector_must_match() {
    let rules = vec![RoleRule {
        r#match: RuleMatch {
            sub: Some("u1".into()),
            groups_any: Some(vec!["admins".into()]),
            ..Default::default()
        },
        role: UserRole::Admin,
    }];
    let admins = vec!["admins".to_string()];
    let viewers = vec!["viewers".to_string()];

    assert!(resolve_role(&rules, &claims(&admins, None, false)).is_some());
    assert_eq!(resolve_role(&rules, &claims(&viewers, None, false)), None);
}

#[test]
fn selector_less_rule_never_matches_even_if_it_reaches_us() {
    let rules = vec![RoleRule {
        r#match: RuleMatch::default(),
        role: UserRole::Admin,
    }];
    let groups: Vec<String> = Vec::new();
    assert_eq!(resolve_role(&rules, &claims(&groups, None, false)), None);
}

#[test]
fn scopes_normalize_commas_and_whitespace() {
    use super::normalize_scopes;
    assert_eq!(
        normalize_scopes("openid, profile,email"),
        "openid profile email"
    );
    assert_eq!(normalize_scopes("openid ,  profile , "), "openid profile");
    assert_eq!(
        normalize_scopes("openid profile email"),
        "openid profile email"
    );
    assert_eq!(normalize_scopes("  "), "");
    assert_eq!(normalize_scopes(",,,"), "");
}
