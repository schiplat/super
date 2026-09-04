use clap::{Parser, ValueEnum};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use sysinfo::{Pid, System};

#[derive(Parser)]
#[command(
    version,
    about = "Sample daemon-set RSS/CPU and process-tree RSS for one bench arm"
)]
struct Args {
    #[arg(long)]
    target: Target,

    /// Directory produced by generator for this arm (super-oss/, supervisord/, …).
    #[arg(long)]
    instance_dir: PathBuf,

    #[arg(long)]
    duration: u64,

    #[arg(long)]
    output_csv: PathBuf,

    /// Expected managed payload processes (B3 check).
    #[arg(long, default_value_t = 0)]
    expected_n: usize,

    #[arg(long, default_value_t = 500)]
    sample_ms: u64,

    /// Optional Bearer token for super-pro control/metrics.
    #[arg(long)]
    auth_token: Option<String>,

    /// superd binary (default: PATH).
    #[arg(long)]
    superd: Option<PathBuf>,

    /// Directory of licensed plugins to copy into SUPER_ROOT/plugins (super-pro only).
    #[arg(long)]
    plugins_dir: Option<PathBuf>,
}

#[derive(Clone, Copy, ValueEnum, PartialEq, Eq)]
enum Target {
    #[clap(name = "super-oss")]
    SuperOss,
    #[clap(name = "super-pro")]
    SuperPro,
    Supervisord,
    Pm2,
}

impl Target {
    fn as_str(self) -> &'static str {
        match self {
            Target::SuperOss => "super-oss",
            Target::SuperPro => "super-pro",
            Target::Supervisord => "supervisord",
            Target::Pm2 => "pm2",
        }
    }
}

struct Handle {
    daemon_pids: Vec<u32>,
    child: Option<Child>,
}

fn pid_of(p: u32) -> Pid {
    Pid::from(p as usize)
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    fs::create_dir_all(args.output_csv.parent().unwrap_or(Path::new(".")))?;

    prepare_instance(&args)?;
    let mut handle = start_target(&args)?;

    let mut system = System::new_all();
    // Two refreshes so cpu_usage() is a real delta (first sysinfo sample is 0).
    system.refresh_processes();
    thread::sleep(Duration::from_millis(200));
    system.refresh_processes();

    let start_time = Instant::now();
    let mut wtr = csv::Writer::from_path(&args.output_csv)?;
    wtr.write_record([
        "time_ms",
        "cpu_usage",
        "memory_mb",
        "total_tree_rss_mb",
        "managed_process_count",
    ])?;

    let mut first_row = true;
    let deadline = Duration::from_secs(args.duration);
    while start_time.elapsed() < deadline {
        thread::sleep(Duration::from_millis(args.sample_ms));
        system.refresh_processes();

        let live: Vec<u32> = handle
            .daemon_pids
            .iter()
            .copied()
            .filter(|p| system.process(pid_of(*p)).is_some())
            .collect();
        if live.is_empty() {
            eprintln!("All daemon PIDs exited");
            break;
        }
        handle.daemon_pids = live;

        let (cpu, mem_mb) = daemon_cpu_mem(&system, &handle.daemon_pids);
        let (tree_mb, managed) = tree_and_managed(&system, &handle.daemon_pids);
        if args.expected_n > 0 && managed < args.expected_n {
            eprintln!(
                "warn: managed_process_count={managed} expected={}",
                args.expected_n
            );
        }

        if first_row {
            first_row = false;
            continue;
        }

        wtr.write_record(&[
            start_time.elapsed().as_millis().to_string(),
            format!("{cpu:.2}"),
            format!("{mem_mb:.2}"),
            format!("{tree_mb:.2}"),
            managed.to_string(),
        ])?;
        wtr.flush()?;
    }

    let _ = collect_restarts(&args);
    stop_target(&args, &mut handle);
    thread::sleep(Duration::from_millis(300));
    if leftovers(&handle.daemon_pids) {
        anyhow::bail!(
            "teardown left daemon PIDs running: {:?}",
            handle.daemon_pids
        );
    }
    println!("Benchmark finished. Data saved to {:?}", args.output_csv);
    Ok(())
}

fn prepare_instance(args: &Args) -> anyhow::Result<()> {
    match args.target {
        Target::SuperOss | Target::SuperPro => {
            let root = &args.instance_dir;
            for d in ["conf", "data", "logs", "run", "plugins"] {
                fs::create_dir_all(root.join(d))?;
            }
            if args.target == Target::SuperPro {
                inject_pro_license(root)?;
                if let Some(src) = &args.plugins_dir {
                    copy_plugins(src, &root.join("plugins"))?;
                } else if let Ok(src) = std::env::var("SUPER_BENCH_PLUGINS_DIR") {
                    copy_plugins(Path::new(&src), &root.join("plugins"))?;
                } else {
                    anyhow::bail!(
                        "super-pro requires --plugins-dir or SUPER_BENCH_PLUGINS_DIR (security + isolation)"
                    );
                }
            }
        }
        Target::Supervisord => {
            fs::create_dir_all(args.instance_dir.join("logs"))?;
            fs::create_dir_all(args.instance_dir.join("run"))?;
        }
        Target::Pm2 => {
            fs::create_dir_all(args.instance_dir.join("logs"))?;
            fs::create_dir_all(args.instance_dir.join("pm2-home"))?;
        }
    }
    Ok(())
}

fn inject_pro_license(root: &Path) -> anyhow::Result<()> {
    let toml_path = root.join("conf/super.toml");
    let mut toml = fs::read_to_string(&toml_path)?;
    let secret = std::env::var("SUPER_BENCH_AUTH_SECRET")
        .map_err(|_| anyhow::anyhow!("super-pro requires SUPER_BENCH_AUTH_SECRET"))?;
    let key = if let Ok(k) = std::env::var("SUPER_BENCH_LICENSE_KEY") {
        k
    } else if let Ok(p) = std::env::var("SUPER_BENCH_LICENSE_FILE") {
        fs::read_to_string(p)?.trim().to_string()
    } else {
        anyhow::bail!("super-pro requires SUPER_BENCH_LICENSE_KEY or SUPER_BENCH_LICENSE_FILE");
    };
    let secret_esc = secret.replace('\\', "\\\\").replace('"', "\\\"");
    let key_esc = key.replace('\\', "\\\\").replace('"', "\\\"");
    toml.push_str(&format!(
        "\nauth_secret = \"{secret_esc}\"\n[license]\nkey = \"{key_esc}\"\nstrict = true\n"
    ));
    fs::write(toml_path, toml)?;
    Ok(())
}

fn copy_plugins(src: &Path, dst: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(dst)?;
    let mut copied = 0usize;
    for ent in fs::read_dir(src)? {
        let ent = ent?;
        let name = ent.file_name();
        let n = name.to_string_lossy();
        if n.starts_with("security.") || n.starts_with("isolation.") {
            fs::copy(ent.path(), dst.join(&name))?;
            copied += 1;
        }
    }
    if copied < 2 {
        anyhow::bail!(
            "expected security.* and isolation.* plugins in {} (found {copied})",
            src.display()
        );
    }
    Ok(())
}

fn start_target(args: &Args) -> anyhow::Result<Handle> {
    match args.target {
        Target::SuperOss | Target::SuperPro => {
            let superd = args
                .superd
                .clone()
                .unwrap_or_else(|| PathBuf::from("superd"));
            let child = Command::new(&superd)
                .env("SUPER_ROOT", args.instance_dir.canonicalize()?)
                .arg("--foreground")
                .current_dir(&args.instance_dir)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            let pid = child.id();
            wait_http(
                "http://127.0.0.1:9002/health",
                args.auth_token.as_deref(),
                40,
            )?;
            Ok(Handle {
                daemon_pids: vec![pid],
                child: Some(child),
            })
        }
        Target::Supervisord => {
            let conf = args.instance_dir.join("supervisord.conf");
            Command::new("supervisord")
                .arg("-c")
                .arg(&conf)
                .current_dir(&args.instance_dir)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()?;
            thread::sleep(Duration::from_secs(2));
            let pidfile = args.instance_dir.join("run/supervisord.pid");
            let pid: u32 = fs::read_to_string(&pidfile)?.trim().parse()?;
            Ok(Handle {
                daemon_pids: vec![pid],
                child: None,
            })
        }
        Target::Pm2 => {
            let home = args.instance_dir.join("pm2-home");
            let eco = args.instance_dir.join("ecosystem.config.js");
            let _ = Command::new("pm2")
                .env("PM2_HOME", &home)
                .arg("kill")
                .output();
            Command::new("pm2")
                .env("PM2_HOME", &home)
                .arg("start")
                .arg(&eco)
                .current_dir(&args.instance_dir)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()?;
            thread::sleep(Duration::from_secs(3));
            let mut pids = Vec::new();
            let pidfile = home.join("pm2.pid");
            if pidfile.exists() {
                if let Ok(p) = fs::read_to_string(&pidfile)?.trim().parse::<u32>() {
                    pids.push(p);
                }
            }
            if let Ok(out) = Command::new("pgrep").args(["-f", "pm2-agent"]).output() {
                for line in String::from_utf8_lossy(&out.stdout).lines() {
                    if let Ok(p) = line.trim().parse::<u32>() {
                        if !pids.contains(&p) {
                            pids.push(p);
                        }
                    }
                }
            }
            if pids.is_empty() {
                anyhow::bail!("PM2 daemon PID not found (PM2_HOME={})", home.display());
            }
            Ok(Handle {
                daemon_pids: pids,
                child: None,
            })
        }
    }
}

fn wait_http(url: &str, token: Option<&str>, tries: u32) -> anyhow::Result<()> {
    for i in 0..tries {
        let mut cmd = Command::new("curl");
        cmd.args(["-sf", "--max-time", "1", url]);
        if let Some(t) = token {
            cmd.arg("-H").arg(format!("Authorization: Bearer {t}"));
        }
        if cmd.status().map(|s| s.success()).unwrap_or(false) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(250 + u64::from(i) * 50));
    }
    anyhow::bail!("timed out waiting for {url}")
}

fn daemon_cpu_mem(system: &System, pids: &[u32]) -> (f32, f64) {
    let mut cpu = 0.0f32;
    let mut mem = 0u64;
    for p in pids {
        if let Some(proc_) = system.process(pid_of(*p)) {
            cpu += proc_.cpu_usage();
            mem += proc_.memory();
        }
    }
    (cpu, mem as f64 / 1024.0 / 1024.0)
}

fn tree_and_managed(system: &System, daemon_pids: &[u32]) -> (f64, usize) {
    let daemon: HashSet<u32> = daemon_pids.iter().copied().collect();
    let mut descendants: HashSet<u32> = HashSet::new();
    for (pid, proc_) in system.processes() {
        let id = pid.as_u32();
        if daemon.contains(&id) {
            continue;
        }
        let mut cur = proc_.parent();
        let mut hops = 0;
        while let Some(pp) = cur {
            if daemon.contains(&pp.as_u32()) {
                descendants.insert(id);
                break;
            }
            cur = system.process(pp).and_then(|p| p.parent());
            hops += 1;
            if hops > 16 {
                break;
            }
        }
    }

    let mut tree_bytes = 0u64;
    for p in daemon_pids {
        if let Some(proc_) = system.process(pid_of(*p)) {
            tree_bytes += proc_.memory();
        }
    }
    for id in &descendants {
        if let Some(proc_) = system.process(pid_of(*id)) {
            tree_bytes += proc_.memory();
        }
    }

    let managed = descendants
        .iter()
        .filter(|id| {
            system.process(pid_of(**id)).is_some_and(|p| {
                let n = p.name().to_lowercase();
                n.contains("payloads")
            })
        })
        .count();

    (tree_bytes as f64 / 1024.0 / 1024.0, managed)
}

fn collect_restarts(args: &Args) -> anyhow::Result<()> {
    let dest = args.output_csv.with_file_name("restarts.json");
    match args.target {
        Target::SuperOss | Target::SuperPro => {
            let mut cmd = Command::new("curl");
            cmd.args(["-sf", "http://127.0.0.1:9002/metrics"]);
            if let Some(t) = &args.auth_token {
                cmd.arg("-H").arg(format!("Authorization: Bearer {t}"));
            }
            let raw = String::from_utf8_lossy(&cmd.output()?.stdout).into_owned();
            let sum: f64 = raw
                .lines()
                .filter(|l| l.starts_with("super_process_restart_count"))
                .filter_map(|l| l.split_whitespace().last()?.parse::<f64>().ok())
                .sum();
            let body = format!(
                "{{\"arm\":\"{}\",\"restart_sum\":{sum}}}",
                args.target.as_str()
            );
            fs::write(dest, body)?;
        }
        Target::Supervisord => {
            let conf = args.instance_dir.join("supervisord.conf");
            let out = Command::new("supervisorctl")
                .args(["-c", &conf.to_string_lossy(), "status"])
                .output()?;
            let text = String::from_utf8_lossy(&out.stdout);
            let body = serde_json::json!({
                "arm": "supervisord",
                "status": text.trim(),
            });
            fs::write(dest, serde_json::to_string_pretty(&body)?)?;
        }
        Target::Pm2 => {
            let home = args.instance_dir.join("pm2-home");
            let out = Command::new("pm2")
                .env("PM2_HOME", home)
                .arg("jlist")
                .output()?;
            fs::write(dest, &out.stdout)?;
        }
    }
    Ok(())
}

fn stop_target(args: &Args, handle: &mut Handle) {
    match args.target {
        Target::SuperOss | Target::SuperPro => {
            if let Some(mut c) = handle.child.take() {
                let _ = c.kill();
                let _ = c.wait();
            }
            for p in &handle.daemon_pids {
                let _ = Command::new("kill")
                    .args(["-TERM", &p.to_string()])
                    .status();
            }
            thread::sleep(Duration::from_millis(500));
            for p in &handle.daemon_pids {
                let _ = Command::new("kill")
                    .args(["-KILL", &p.to_string()])
                    .status();
            }
        }
        Target::Supervisord => {
            let conf = args.instance_dir.join("supervisord.conf");
            let _ = Command::new("supervisorctl")
                .args(["-c", &conf.to_string_lossy(), "shutdown"])
                .status();
            thread::sleep(Duration::from_millis(800));
            for p in &handle.daemon_pids {
                let _ = Command::new("kill")
                    .args(["-TERM", &p.to_string()])
                    .status();
            }
        }
        Target::Pm2 => {
            let home = args.instance_dir.join("pm2-home");
            let _ = Command::new("pm2")
                .env("PM2_HOME", home)
                .arg("kill")
                .status();
        }
    }
    thread::sleep(Duration::from_millis(400));
}

fn leftovers(pids: &[u32]) -> bool {
    pids.iter()
        .any(|p| Path::new(&format!("/proc/{p}")).exists())
}
