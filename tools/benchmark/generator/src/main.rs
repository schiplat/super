use clap::Parser;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    version,
    about = "Generate equivalent configs for super-oss, super-pro, supervisord, pm2"
)]
struct Args {
    #[arg(long, default_value_t = 50)]
    count: usize,

    /// Semantic mode: idle | crash | log | mem-leak
    #[arg(long, default_value = "idle")]
    mode: String,

    #[arg(long)]
    payload_path: PathBuf,

    #[arg(long)]
    output_dir: PathBuf,

    /// mem-eat safety cap (MiB per process). Default 64 for ordinary Lab hosts.
    #[arg(long, default_value_t = 64)]
    cap_mb: usize,

    /// PRO-only: cgroup memory_limit in MB (STB-2-PRO). 0 = omit.
    #[arg(long, default_value_t = 0)]
    cgroup_memory_mb: u64,

    /// Shared crash RNG: program i uses seed = crash_seed_base + i
    #[arg(long, default_value_t = 1)]
    crash_seed_base: u64,
}

fn payload_mode(semantic: &str) -> anyhow::Result<&'static str> {
    Ok(match semantic {
        "idle" => "idle",
        "crash" => "crash-random",
        "log" => "log-throughput",
        "mem-leak" => "mem-eat",
        other => anyhow::bail!("unknown semantic mode {other} (idle|crash|log|mem-leak)"),
    })
}

fn payload_args(i: usize, semantic: &str, cap_mb: usize, seed_base: u64) -> Vec<String> {
    let mode = payload_mode(semantic).expect("mode");
    let mut args = vec!["--mode".into(), mode.into()];
    match semantic {
        "mem-leak" => {
            args.push("--cap-mb".into());
            args.push(cap_mb.to_string());
        }
        "crash" => {
            args.push("--seed".into());
            args.push((seed_base + i as u64).to_string());
        }
        _ => {}
    }
    args
}

fn super_toml(pro: bool) -> String {
    let mut s = String::from(
        "# Bench instance config. Programs load from [include] JSON (not [[program]] in this file).\n\
         # Control plane: loopback only (bench config, not a product-default audit).\n\n\
         [server]\n\
         host = \"127.0.0.1\"\n\
         port = 9002\n\
         allow_insecure_public_bind = false\n\
         enable_docs = false\n\
         daemon = false\n\
         shutdown_timeout = 10\n\
         flapping_window = 60\n\
         flapping_threshold = 10000\n\n\
         [logging]\n\
         log_level = \"warn\"\n\
         log_max_mb = 1024\n\
         log_backups = 0\n\n\
         [child_logging]\n\
         driver = \"file\"\n\
         max_size_mb = 4096\n\
         max_backups = 0\n\
         max_line_size_kb = 64\n\n\
         [storage]\n\
         log_dir = \"logs\"\n\
         data_file = \"data/snapshot.json\"\n\n\
         [include]\n\
         files = [\"conf/conf.d/*.json\"]\n",
    );
    if pro {
        s.push_str(
            "\n# Filled by the orchestrator from SUPER_BENCH_AUTH_SECRET / SUPER_BENCH_LICENSE_FILE.\n\
             # auth_secret = \"\"\n\
             # [license]\n\
             # key = \"\"\n\
             # strict = true\n",
        );
    }
    s
}

fn services_json(
    count: usize,
    semantic: &str,
    payload: &str,
    cap_mb: usize,
    seed_base: u64,
    cgroup_memory_mb: u64,
) -> Value {
    let services: Vec<Value> = (0..count)
        .map(|i| {
            let mut svc = json!({
                "name": format!("bench-{i}"),
                "command": payload,
                "args": payload_args(i, semantic, cap_mb, seed_base),
                "autostart": true,
                "retry_limit": 3,
                "autorestart": "true",
                "startsecs": 0,
            });
            if cgroup_memory_mb > 0 {
                svc["resource_limits"] = json!({ "memory_limit": cgroup_memory_mb });
            }
            svc
        })
        .collect();
    json!({ "prune": false, "services": services })
}

fn supervisor_conf(
    count: usize,
    semantic: &str,
    payload: &str,
    cap_mb: usize,
    seed_base: u64,
) -> String {
    // inet HTTP is BENCH control plane (needed for supervisorctl), not product default.
    let mut s = String::from(
        "[supervisord]\n\
         nodaemon=false\n\
         logfile=logs/supervisord.log\n\
         pidfile=run/supervisord.pid\n\
         childlogdir=logs\n\
         loglevel=warn\n\
         directory=.\n\n\
         [inet_http_server]\n\
         port=127.0.0.1:9001\n\n\
         [rpcinterface:supervisor]\n\
         supervisor.rpcinterface_factory = supervisor.rpcinterface:make_main_rpcinterface\n\n\
         [supervisorctl]\n\
         serverurl=http://127.0.0.1:9001\n\n",
    );
    let mode = payload_mode(semantic).unwrap();
    for i in 0..count {
        let extra = match semantic {
            "mem-leak" => format!(" --cap-mb {cap_mb}"),
            "crash" => format!(" --seed {}", seed_base + i as u64),
            _ => String::new(),
        };
        s.push_str(&format!(
            "[program:bench-{i}]\n\
             command={payload} --mode {mode}{extra}\n\
             autostart=true\n\
             autorestart=true\n\
             startretries=3\n\
             startsecs=0\n\
             stdout_logfile=logs/bench-{i}.out\n\
             stderr_logfile=logs/bench-{i}.err\n\
             stdout_logfile_maxbytes=0\n\
             stderr_logfile_maxbytes=0\n\
             directory=.\n\n"
        ));
    }
    s
}

fn pm2_ecosystem(
    count: usize,
    semantic: &str,
    payload: &str,
    cap_mb: usize,
    seed_base: u64,
) -> String {
    let mode = payload_mode(semantic).unwrap();
    let mut apps = Vec::new();
    for i in 0..count {
        let extra = match semantic {
            "mem-leak" => format!(" --cap-mb {cap_mb}"),
            "crash" => format!(" --seed {}", seed_base + i as u64),
            _ => String::new(),
        };
        apps.push(format!(
            "    {{\n\
               name: 'bench-{i}',\n\
               script: '{payload}',\n\
               args: '--mode {mode}{extra}',\n\
               instances: 1,\n\
               autorestart: true,\n\
               max_restarts: 3,\n\
               min_uptime: 0,\n\
               out_file: 'logs/bench-{i}.out',\n\
               error_file: 'logs/bench-{i}.err',\n\
               merge_logs: false\n\
             }}"
        ));
    }
    format!(
        "module.exports = {{\n  apps : [\n{}\n  ]\n}};\n",
        apps.join(",\n")
    )
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let _ = payload_mode(&args.mode)?;
    fs::create_dir_all(&args.output_dir)?;
    let payload = args
        .payload_path
        .canonicalize()?
        .to_string_lossy()
        .to_string();

    let oss_dir = args.output_dir.join("super-oss");
    let pro_dir = args.output_dir.join("super-pro");
    let sup_dir = args.output_dir.join("supervisord");
    let pm2_dir = args.output_dir.join("pm2");
    for d in [
        oss_dir.join("conf/conf.d"),
        pro_dir.join("conf/conf.d"),
        sup_dir.clone(),
        pm2_dir.clone(),
    ] {
        fs::create_dir_all(&d)?;
    }

    fs::write(oss_dir.join("conf/super.toml"), super_toml(false))?;
    fs::write(
        oss_dir.join("conf/conf.d/bench.json"),
        serde_json::to_string_pretty(&services_json(
            args.count,
            &args.mode,
            &payload,
            args.cap_mb,
            args.crash_seed_base,
            0,
        ))?,
    )?;

    fs::write(pro_dir.join("conf/super.toml"), super_toml(true))?;
    fs::write(
        pro_dir.join("conf/conf.d/bench.json"),
        serde_json::to_string_pretty(&services_json(
            args.count,
            &args.mode,
            &payload,
            args.cap_mb,
            args.crash_seed_base,
            args.cgroup_memory_mb,
        ))?,
    )?;

    fs::create_dir_all(sup_dir.join("logs"))?;
    fs::create_dir_all(sup_dir.join("run"))?;
    fs::write(
        sup_dir.join("supervisord.conf"),
        supervisor_conf(
            args.count,
            &args.mode,
            &payload,
            args.cap_mb,
            args.crash_seed_base,
        ),
    )?;

    fs::create_dir_all(pm2_dir.join("logs"))?;
    fs::write(
        pm2_dir.join("ecosystem.config.js"),
        pm2_ecosystem(
            args.count,
            &args.mode,
            &payload,
            args.cap_mb,
            args.crash_seed_base,
        ),
    )?;

    println!("Configs generated in {:?}", args.output_dir);
    Ok(())
}
