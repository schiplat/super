use clap::{Parser, ValueEnum};
use rand::Rng;
use std::io::{self, Write};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Parser)]
#[command(version)]
struct Args {
    #[arg(short, long)]
    mode: Mode,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum Mode {
    Idle,          // Idle: measure Super baseline overhead
    LogThroughput, // Log throughput: measure Super pipe consumption (critical)
    CrashRandom,   // Random crash: measure Super recovery logic
    MemEat,        // Memory growth: measure Super OOM handling
    CpuBurn,       // CPU burn mode
}

fn main() {
    let args = Args::parse();

    match args.mode {
        Mode::Idle => {
            // Just stay alive with minimal resource use
            loop {
                thread::sleep(Duration::from_secs(3600));
            }
        }

        Mode::LogThroughput => {
            // [Critical test]
            // Write stdout as fast as possible. Blocks if Super reads slowly.
            // Measure Super overhead via actual write throughput.
            let total_lines = 500_000;
            let payload = "INFO: 2024-01-01 This is a benchmark log line to test throughput performance 1234567890";

            // BufWriter reduces syscalls so the payload is not the bottleneck
            let stdout = io::stdout();
            let mut handle = io::BufWriter::new(stdout.lock());

            let start = Instant::now();

            for i in 0..total_lines {
                if writeln!(handle, "{} - {}", i, payload).is_err() {
                    break; // Pipe broken
                }
            }
            let _ = handle.flush();

            let duration = start.elapsed();
            let speed = total_lines as f64 / duration.as_secs_f64();

            // Print result to stderr (Super may capture stderr; scripts only need this number)
            // Format: BENCH_RESULT:<LINES_PER_SEC>
            eprintln!("BENCH_RESULT:{:.2}", speed);
        }

        Mode::CrashRandom => {
            let mut rng = rand::thread_rng();
            let sleep_ms = rng.gen_range(100..2000);
            thread::sleep(Duration::from_millis(sleep_ms));
            std::process::exit(1);
        }

        Mode::MemEat => {
            // Allocate 5MB every 100ms to simulate rapid memory leak
            let mut container = Vec::new();
            println!("Starting Memory Leak...");
            loop {
                // 5MB per chunk
                let chunk = vec![0u8; 5 * 1024 * 1024];
                container.push(chunk);
                // Prevent optimization; log current size
                if container.len() % 10 == 0 {
                    println!("Allocated: {} MB", container.len() * 5);
                }
                thread::sleep(Duration::from_millis(100));
            }
        }

        Mode::CpuBurn => {
            // CPU-bound loop targeting ~100% of one core
            println!("Starting CPU Burn...");
            let mut x: f64 = 0.0;
            loop {
                // Simple float ops to prevent compiler from optimizing away the loop
                x = (x + 1.0).sin().cos().tan();
                if x > 1000.0 {
                    x = 0.0;
                }
            }
        }
    }
}
