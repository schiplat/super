use super::*;
use std::process::Command;

fn write_script(path: &Path, body: &str) {
    std::fs::write(path, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut p = std::fs::metadata(path).unwrap().permissions();
        p.set_mode(0o755);
        std::fs::set_permissions(path, p).unwrap();
    }
}

#[test]
fn extract_tar_gz_single_file() {
    let dir = tempfile::tempdir().unwrap();
    let payload = dir.path().join("my-app");
    write_script(&payload, "#!/bin/sh\necho V2\n");
    let archive = dir.path().join("app.tar.gz");
    let status = Command::new("tar")
        .args(["-czf"])
        .arg(&archive)
        .arg("-C")
        .arg(dir.path())
        .arg("my-app")
        .status()
        .unwrap();
    assert!(status.success());

    let staging = dir.path().join("my-app.new");
    extract_archive_to_staging(&archive, &staging, "my-app").unwrap();
    assert!(staging.exists());
    assert!(std::fs::read_to_string(&staging).unwrap().contains("V2"));
}

#[test]
fn extract_zip_basename_match_among_many() {
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("edge-agent");
    let extra = dir.path().join("README.txt");
    write_script(&bin, "#!/bin/sh\necho AGENT\n");
    std::fs::write(&extra, "notes").unwrap();
    let archive = dir.path().join("bundle.zip");
    let status = Command::new("zip")
        .args([
            "-j",
            archive.to_str().unwrap(),
            bin.to_str().unwrap(),
            extra.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let staging = dir.path().join("edge-agent.new");
    extract_archive_to_staging(&archive, &staging, "edge-agent").unwrap();
    assert!(std::fs::read_to_string(&staging).unwrap().contains("AGENT"));
}

#[test]
fn extract_rejects_path_traversal_tar() {
    let dir = tempfile::tempdir().unwrap();
    // Craft a tar with a `../evil` member via Python for portability.
    let archive = dir.path().join("evil.tar");
    let py = r#"
import tarfile, sys
with tarfile.open(sys.argv[1], "w") as t:
    info = tarfile.TarInfo("../evil")
    data = b"x"
    info.size = len(data)
    import io
    t.addfile(info, io.BytesIO(data))
"#;
    let status = Command::new("python3")
        .args(["-c", py, archive.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(status.success());
    let staging = dir.path().join("app.new");
    let err = extract_archive_to_staging(&archive, &staging, "app").unwrap_err();
    assert!(
        err.to_string().contains("rejected") || err.to_string().contains("relative"),
        "{err}"
    );
}

#[test]
fn extract_multi_file_without_basename_errors() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.bin");
    let b = dir.path().join("b.bin");
    write_script(&a, "a");
    write_script(&b, "b");
    let archive = dir.path().join("multi.tar");
    let status = Command::new("tar")
        .args(["-cf"])
        .arg(&archive)
        .arg("-C")
        .arg(dir.path())
        .args(["a.bin", "b.bin"])
        .status()
        .unwrap();
    assert!(status.success());
    let staging = dir.path().join("app.new");
    let err = extract_archive_to_staging(&archive, &staging, "app").unwrap_err();
    assert!(err.to_string().contains("none named"), "{err}");
}
