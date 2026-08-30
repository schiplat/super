use clap::{Parser, ValueEnum};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::io::{self, Write};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Parser)]
#[command(
    version,
    about = "Deterministic bench payload (idle / log / crash / mem-eat / cpu-burn)"
)]
struct Args {
    #[arg(short, long)]
    mode: Mode,

    /// mem-eat: stop growing after this many MiB and hold (safety cap).
    /// Default 64 — suite targets ordinary 4–8 GiB labs, not half-machine heaps.
    #[arg(long, default_value_t = 64)]
    cap_mb: usize,

    /// crash-random: RNG seed so all arms share the same crash schedule when configs are shared.
    #[arg(long, default_value_t = 1)]
    seed: u64,

    /// log-throughput: milliseconds between BENCH_RESULT lines on stderr.
    #[arg(long, default_value_t = 2000)]
    report_interval_ms: u64,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum Mode {
    Idle,
    LogThroughput,
    CrashRandom,
    MemEat,
    CpuBurn,
}

fn main() {
    let args = Args::parse();

    match args.mode {
        Mode::Idle => loop {
            thread::sleep(Duration::from_secs(3600));
        },

        Mode::LogThroughput => {
            // Sustained write until SIGTERM. Finite 500k-then-exit would turn
            // long scenarios into restart storms instead of log backpressure.
            let payload = "INFO: 2026-01-01 bench log line 1234567890";
            let stdout = io::stdout();
            let mut handle = io::BufWriter::with_capacity(64 * 1024, stdout.lock());
            let mut window_start = Instant::now();
            let mut window_lines: u64 = 0;
            let mut total: u64 = 0;
            let report = Duration::from_millis(args.report_interval_ms.max(200));

            loop {
                if writeln!(handle, "{total} - {payload}").is_err() {
                    break;
                }
                window_lines += 1;
                total += 1;
                if window_start.elapsed() >= report {
                    let _ = handle.flush();
                    let secs = window_start.elapsed().as_secs_f64().max(1e-6);
                    let lps = window_lines as f64 / secs;
                    eprintln!("BENCH_RESULT:{lps:.2}");
                    window_start = Instant::now();
                    window_lines = 0;
                }
            }
        }

        Mode::CrashRandom => {
            let mut rng = StdRng::seed_from_u64(args.seed);
            let sleep_ms = rng.gen_range(100..2000);
            thread::sleep(Duration::from_millis(sleep_ms));
            std::process::exit(1);
        }

        Mode::MemEat => {
            // Touch every page so Linux actually accounts the RSS (zero-fill is lazy).
            let cap_bytes = args.cap_mb.saturating_mul(1024 * 1024);
            let chunk_size = 5 * 1024 * 1024;
            let mut held: Vec<Vec<u8>> = Vec::new();
            let mut allocated = 0usize;
            eprintln!("Starting mem-eat cap_mb={}", args.cap_mb);
            while allocated + chunk_size <= cap_bytes {
                let mut chunk = vec![0u8; chunk_size];
                for i in (0..chunk_size).step_by(4096) {
                    chunk[i] = 1;
                }
                allocated += chunk_size;
                held.push(chunk);
                if held.len().is_multiple_of(8) {
                    println!("Allocated: {} MB", allocated / (1024 * 1024));
                }
                thread::sleep(Duration::from_millis(100));
            }
            println!("Holding {} MB (cap reached)", allocated / (1024 * 1024));
            loop {
                thread::sleep(Duration::from_secs(3600));
                let _ = held.len();
            }
        }

        Mode::CpuBurn => {
            println!("Starting CPU Burn...");
            let mut x: f64 = 0.0;
            loop {
                x = (x + 1.0).sin().cos().tan();
                if x > 1000.0 {
                    x = 0.0;
                }
            }
        }
    }
}
