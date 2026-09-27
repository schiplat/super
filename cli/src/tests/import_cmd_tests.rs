use super::*;
use std::collections::HashMap;

fn sample() -> StackDraft {
    let ini = "[program:web]\ncommand=/usr/bin/node server.js\nautostart=true\n";
    let ctx = ParseCtx::default();
    use common::import::StackFormat as _;
    common::import::SupervisorFormat.parse(ini, &ctx).unwrap()
}

#[test]
fn no_start_forces_autostart_false() {
    let d = stop_autostart(sample());
    assert!(!d.services[0].autostart);
}

#[test]
fn preview_partitions_services_and_collisions() {
    let d = sample();
    let current = ["web".to_string()];
    let (fresh, renamed, skipped) = resolve_collisions(&d, &current, CollisionMode::Skip, None);
    assert_eq!(skipped, vec!["web".to_string()]);
    assert!(fresh.is_empty());
    assert!(renamed.is_empty());
}

#[test]
fn rename_mode_generates_unique_suffixed_names() {
    let d = sample(); // program:web
    let current = ["web".to_string()];
    let (fresh, renamed, skipped) = resolve_collisions(&d, &current, CollisionMode::Rename, None);
    assert!(skipped.is_empty());
    assert_eq!(renamed.len(), 1);
    let (from, to) = &renamed[0];
    assert_eq!(from, "web");
    assert!(to.starts_with("web-") && to.len() == "web-XXXXXX".len());
    assert_ne!(fresh[0].name.as_deref(), Some("web"));
    assert_eq!(fresh[0].name.as_deref(), Some(to.as_str()));
}

#[test]
fn rename_with_fixed_suffix_and_fallback() {
    let d = sample();
    let current = ["web".to_string()];
    let (_, renamed, _) = resolve_collisions(&d, &current, CollisionMode::Rename, Some("staging"));
    assert_eq!(renamed[0].1, "web-staging");
    // Fixed suffix already taken on the daemon -> falls back to random.
    let current2 = ["web".to_string(), "web-staging".to_string()];
    let (_, renamed2, _) =
        resolve_collisions(&d, &current2, CollisionMode::Rename, Some("staging"));
    assert_eq!(renamed2[0].0, "web");
    assert_ne!(renamed2[0].1, "web-staging");
    assert!(renamed2[0].1.starts_with("web-"));
}

#[test]
fn override_mode_keeps_name_and_includes_service() {
    // The override bug: collided services must stay IN the apply set
    // AND be reported as the names being overridden.
    let d = sample(); // program:web collides
    let current = ["web".to_string()];
    let (fresh, renamed, skipped) = resolve_collisions(&d, &current, CollisionMode::Override, None);
    assert!(renamed.is_empty());
    assert_eq!(skipped, vec!["web".to_string()], "override names reported");
    assert_eq!(fresh.len(), 1, "override keeps service in apply set");
    assert_eq!(fresh[0].name.as_deref(), Some("web"));
}

#[test]
fn non_colliding_services_pass_through_all_modes() {
    let d = sample(); // web
    let current = ["other".to_string()];
    for mode in [
        CollisionMode::Skip,
        CollisionMode::Rename,
        CollisionMode::Override,
    ] {
        let (fresh, renamed, skipped) = resolve_collisions(&d, &current, mode, None);
        assert_eq!(fresh.len(), 1, "mode {mode:?}");
        assert_eq!(fresh[0].name.as_deref(), Some("web"));
        assert!(renamed.is_empty());
        assert!(skipped.is_empty());
    }
}

#[test]
fn duplicate_names_within_batch_rename_consistently() {
    let d = StackDraft {
        services: vec![svc_named("web"), svc_named("web")],
        warnings: Vec::new(),
    };
    let current: Vec<String> = vec![];
    // No daemon collision: both pass through untouched.
    let (fresh, _, _) = resolve_collisions(&d, &current, CollisionMode::Skip, None);
    assert_eq!(fresh.len(), 2);
    // Rename with a daemon collision: batch-internal names stay unique.
    let current2 = ["web".to_string()];
    let (fresh2, renamed2, _) = resolve_collisions(&d, &current2, CollisionMode::Rename, Some("x"));
    assert_eq!(renamed2.len(), 2);
    assert_ne!(renamed2[0].1, renamed2[1].1, "renamed targets must differ");
    let names: Vec<_> = fresh2.iter().filter_map(|s| s.name.clone()).collect();
    assert_ne!(names[0], names[1]);
}

#[test]
fn rename_target_avoids_later_batch_sibling() {
    // Regression: file = [web, web-staging], daemon = [web]. With a fixed
    // suffix "staging" the renamed target for `web` would have been
    // `web-staging` — clobbering the sibling `web-staging` service that
    // passes through untouched. The target must fall back to random.
    let d = StackDraft {
        services: vec![svc_named("web"), svc_named("web-staging")],
        warnings: Vec::new(),
    };
    let current = ["web".to_string()];
    let (fresh, renamed, skipped) =
        resolve_collisions(&d, &current, CollisionMode::Rename, Some("staging"));
    assert_eq!(skipped, Vec::<String>::new());
    assert_eq!(renamed.len(), 1, "only `web` collides with the daemon");
    assert_eq!(renamed[0].0, "web");
    assert_ne!(
        renamed[0].1, "web-staging",
        "rename target must not equal the batch sibling name"
    );
    let names: Vec<_> = fresh.iter().filter_map(|s| s.name.clone()).collect();
    assert_eq!(names.len(), 2);
    assert!(names.contains(&"web-staging".to_string()));
}

fn svc_named(name: &str) -> CreateProgramRequest {
    CreateProgramRequest {
        name: Some(name.to_string()),
        command: "/bin/sleep".to_string(),
        args: vec!["1".to_string()],
        retry_limit: 3,
        startsecs: 10,
        exitcodes: vec![0],
        numprocs: 1,
        ..Default::default()
    }
}

// The `ApiClient` type alias is re-exported for signature clarity only.
#[allow(unused)]
fn _t(_: HashMap<String, String>) {}
