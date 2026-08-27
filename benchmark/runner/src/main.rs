use clap::{Parser, ValueEnum};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use sysinfo::{Pid, System};

#[derive(Parser)]
#[command(version)]
struct Args {
    #[arg(long)]
    target: Target,

    #[arg(long)]
    config_dir: PathBuf,

    #[arg(long)]
    duration: u64, // Test duration (seconds)

    #[arg(long)]
    output_csv: PathBuf,
}

#[derive(Clone, ValueEnum)]
enum Target {
    Super,
    Supervisor,
    Pm2,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let mut system = System::new_all();

    // 1. Cleanup & Start Daemon
    let (daemon_pid, _child_handle) = start_target(&args.target, &args.config_dir)?;

    println!(
        ">>> Monitoring PID: {} for {} seconds...",
        daemon_pid, args.duration
    );

    // 2. Monitoring Loop
    let start_time = Instant::now();
    let mut wtr = csv::Writer::from_path(&args.output_csv)?;
    wtr.write_record(["time_ms", "cpu_usage", "memory_mb"])?;

    while start_time.elapsed().as_secs() < args.duration {
        // Refresh metrics for the target PID
        let pid = Pid::from(daemon_pid as usize);
        system.refresh_processes();
        if let Some(process) = system.process(pid) {
            let cpu = process.cpu_usage(); // %
            let mem = process.memory() as f64 / 1024.0 / 1024.0; // MB
            let elapsed = start_time.elapsed().as_millis();

            wtr.write_record(&[
                elapsed.to_string(),
                format!("{:.2}", cpu),
                format!("{:.2}", mem),
            ])?;
        } else {
            eprintln!("Process {} died!", daemon_pid);
            break;
        }
        wtr.flush()?;
        thread::sleep(Duration::from_millis(500)); // 0.5s sample interval
    }

    // 3. Teardown
    stop_target(&args.target, &args.config_dir);
    println!("Benchmark finished. Data saved to {:?}", args.output_csv);
    Ok(())
}

fn start_target(
    target: &Target,
    config_dir: &Path,
) -> anyhow::Result<(u32, Option<std::process::Child>)> {
    match target {
        Target::Super => {
            let config_path = config_dir.join("super.toml");
            // Assume superd is on PATH
            let child = Command::new("superd")
                .env("SUPER_CONFIG", config_path) // Assume superd reads config from this env var
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            let pid = child.id();
            // Wait for initialization
            thread::sleep(Duration::from_secs(2));
            Ok((pid, Some(child)))
        }
        Target::Supervisor => {
            let config_path = config_dir.join("supervisord.conf");
            Command::new("supervisord")
                .arg("-c")
                .arg(config_path)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            thread::sleep(Duration::from_secs(2));

            // Supervisor runs as a daemon; find PID via pidfile or pgrep
            // Use pgrep for simplicity
            let output = Command::new("pgrep")
                .arg("-n")
                .arg("supervisord")
                .output()?;
            let pid_str = String::from_utf8(output.stdout)?.trim().to_string();
            let pid = pid_str.parse::<u32>()?;
            Ok((pid, None))
        }
        Target::Pm2 => {
            let config_path = config_dir.join("ecosystem.config.js");
            // PM2 start
            Command::new("pm2").arg("kill").output()?; // Cleanup first
            Command::new("pm2")
                .arg("start")
                .arg(config_path)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .output()?;

            thread::sleep(Duration::from_secs(3));

            // Find PM2 God Daemon
            // PM2 client exits; load runs on the God Daemon
            let output = Command::new("pgrep").arg("-f").arg("PM2").output()?;
            let pids = String::from_utf8(output.stdout)?;
            // For simplicity, take the first matching God Daemon PID
            let pid = pids
                .lines()
                .next()
                .ok_or(anyhow::anyhow!("PM2 God Daemon not found"))?
                .trim()
                .parse::<u32>()?;
            Ok((pid, None))
        }
    }
}

fn stop_target(target: &Target, _config_dir: &Path) {
    match target {
        Target::Super => {
            let _ = Command::new("pkill").arg("superd").output();
        }
        Target::Supervisor => {
            let _ = Command::new("pkill").arg("supervisord").output();
        }
        Target::Pm2 => {
            let _ = Command::new("pm2").arg("kill").output();
        }
    }
}
