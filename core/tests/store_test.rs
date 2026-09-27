use common::ProgramConfig;
use std::collections::{HashMap, HashSet};
use super_core::store;
use tempfile::TempDir;
use uuid::Uuid;

#[tokio::test]
async fn test_persistence() {
    // 1. Set up environment
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("snapshot.json");

    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();

    let mut programs = HashMap::new();

    // 2. Build test data
    let mut config1 = ProgramConfig {
        name: "service-a".to_string(),
        command: "sleep".to_string(),
        args: vec![],
        env: HashMap::new(),
        cwd: None,
        user: None,
        group: None,
        autostart: true,
        retry_limit: 3,
        depends_on: vec!["db".to_string()],
        health_check: None,
        hooks: Default::default(),
        artifact: None,
        created_at: 100,
        updated_at: 200,

        cron: None,
        restore_path: None,

        // Use Default for remaining fields (e.g. resource_limits)
        ..Default::default()
    };
    // Simulate Fatal state (via autostart=false)
    config1.autostart = false;

    programs.insert(id1, config1);
    programs.insert(
        id2,
        ProgramConfig {
            name: "service-b".to_string(),
            command: "echo".to_string(),
            ..programs.get(&id1).unwrap().clone() // Reuse config from id1
        },
    );

    // 3. Write
    store::save(
        &file_path,
        &store::Snapshot {
            programs: programs.clone(),
            stopped_by_user: HashSet::new(),
        },
    )
    .await
    .expect("Save failed");
    assert!(file_path.exists());

    // 4. Read
    let loaded = store::load_with_recovery(&file_path)
        .await
        .expect("Load failed");

    // 5. Verify consistency
    assert_eq!(loaded.programs.len(), 2);
    assert!(loaded.stopped_by_user.is_empty());
    let loaded_cfg1 = loaded.programs.get(&id1).unwrap();

    assert_eq!(loaded_cfg1.name, "service-a");
    assert!(!loaded_cfg1.autostart); // State should be preserved
    assert_eq!(loaded_cfg1.depends_on, vec!["db".to_string()]);
}

#[tokio::test]
async fn test_store_resource_limits_roundtrip() {
    use common::ResourceLimits;

    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("snapshot.json");
    let id = Uuid::new_v4();

    let mut programs = HashMap::new();
    programs.insert(
        id,
        ProgramConfig {
            name: "limited".to_string(),
            command: "sleep".to_string(),
            resource_limits: Some(ResourceLimits {
                cpu_quota: Some(0.5),
                memory_limit: Some(100),
                memory_warn_percent: Some(80),
                memory_warn_headroom: None,
                memory_high: Some(88),
            }),
            ..Default::default()
        },
    );

    store::save(
        &file_path,
        &store::Snapshot {
            programs: programs.clone(),
            stopped_by_user: HashSet::new(),
        },
    )
    .await
    .expect("Save failed");

    let loaded = store::load_with_recovery(&file_path)
        .await
        .expect("Load failed");
    let cfg = loaded.programs.get(&id).unwrap();
    let limits = cfg.resource_limits.as_ref().expect("limits missing");
    assert_eq!(limits.cpu_quota, Some(0.5));
    assert_eq!(limits.memory_limit, Some(100));
    assert_eq!(limits.memory_warn_percent, Some(80));
    assert_eq!(limits.memory_high, Some(88));
}

/// Legacy snapshots (pre-1.7: a bare `HashMap<Uuid, ProgramConfig>`) still
/// load, with an empty held-stopped set.
#[tokio::test]
async fn test_legacy_snapshot_loads() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("snapshot.json");
    let id = Uuid::new_v4();
    let mut programs = HashMap::new();
    programs.insert(
        id,
        ProgramConfig {
            name: "legacy".to_string(),
            command: "sleep".to_string(),
            ..Default::default()
        },
    );
    // Write in the legacy format: a bare program map, no wrapper object.
    let content = serde_json::to_string(&programs).unwrap();
    tokio::fs::write(&file_path, content).await.unwrap();

    let loaded = store::load_with_recovery(&file_path)
        .await
        .expect("Load failed");
    assert_eq!(loaded.programs.len(), 1);
    assert!(loaded.programs.contains_key(&id));
    assert!(loaded.stopped_by_user.is_empty());
}

/// `stopped_by_user` round-trips so operator stop intent survives restarts.
#[tokio::test]
async fn test_stopped_by_user_roundtrip() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("snapshot.json");
    let id = Uuid::new_v4();
    let mut held: HashSet<Uuid> = HashSet::new();
    held.insert(id);

    store::save(
        &file_path,
        &store::Snapshot {
            programs: HashMap::new(),
            stopped_by_user: held,
        },
    )
    .await
    .expect("Save failed");

    let loaded = store::load_with_recovery(&file_path)
        .await
        .expect("Load failed");
    assert!(
        loaded.stopped_by_user.contains(&id),
        "held-stopped set must survive save/load"
    );
}
