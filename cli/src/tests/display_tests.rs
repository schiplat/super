use super::*;

fn names(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn confirm_prune_skips_prompt_when_empty() {
    let plan = ApplyPlan {
        create: vec![],
        keep: names(&["web"]),
        remove: vec![],
    };
    assert!(confirm_prune("stack.toml", &plan));
}

#[test]
fn build_apply_plan_classifies_create_keep_remove() {
    let plan = build_apply_plan(
        names(&["web", "worker", "beat"]),
        names(&["web", "legacy", "old-nginx"]),
        true,
    );
    assert_eq!(plan.create, names(&["beat", "worker"]));
    assert_eq!(plan.keep, names(&["web"]));
    assert_eq!(plan.remove, names(&["legacy", "old-nginx"]));

    let no_prune = build_apply_plan(names(&["web"]), names(&["web", "legacy"]), false);
    assert!(no_prune.remove.is_empty());
    assert_eq!(no_prune.keep, names(&["web"]));
}

#[test]
fn prune_confirmation_requires_exact_token() {
    assert!(prune_confirmation_accepted("confirmed"));
    assert!(prune_confirmation_accepted("  confirmed\n"));
    assert!(!prune_confirmation_accepted("y"));
    assert!(!prune_confirmation_accepted("yes"));
    assert!(!prune_confirmation_accepted("prune"));
    assert!(!prune_confirmation_accepted("CONFIRMED"));
    assert!(!prune_confirmation_accepted("confirmed please"));
}

#[test]
fn confirm_batch_never_prompts_for_single() {
    assert!(confirm_batch(0, "STOP", &[]));
    let one = vec!["web".to_string()];
    assert!(confirm_batch(1, "STOP", &one));
}

#[test]
fn confirm_batch_aborts_without_stdin() {
    // In a test binary stdin is not a TTY and there is no input; the
    // fail-closed path returns false.
    let names: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
    assert!(!confirm_batch(3, "STOP", &names));
}

#[test]
fn confirm_batch_accepts_empty_preview() {
    // count > 1 with no preview still prompts (old behaviour).
    assert!(!confirm_batch(3, "RESTART", &[]));
}

#[test]
fn preview_split_shows_everything_under_limit() {
    let src = names(&["a", "b"]);
    let (lines, hidden) = preview_split(&src);
    assert_eq!(lines, vec!["a", "b"]);
    assert_eq!(hidden, 0);
}

#[test]
fn preview_split_truncates_at_limit() {
    let many: Vec<String> = (0..PREVIEW_LIMIT + 7).map(|i| format!("p{i}")).collect();
    let (lines, hidden) = preview_split(&many);
    assert_eq!(lines.len(), PREVIEW_LIMIT);
    assert_eq!(hidden, 7);
    assert_eq!(lines[0], many[0]);
    assert_eq!(lines[PREVIEW_LIMIT - 1], many[PREVIEW_LIMIT - 1]);
}

#[test]
fn preview_split_empty() {
    let (lines, hidden) = preview_split(&[]);
    assert!(lines.is_empty());
    assert_eq!(hidden, 0);
}
