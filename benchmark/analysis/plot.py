"""Plot one scenario directory: CSVs named {super-oss,super-pro,supervisord,pm2}.csv."""
from __future__ import annotations

import argparse
import os

import matplotlib.pyplot as plt
import pandas as pd

# Same hue for Super editions; dash vs solid + hatch in bars (summarize.py).
SERIES = [
    ("super-oss", "#b45309", "solid", "o"),
    ("super-pro", "#f59e0b", "dashed", "s"),
    ("supervisord", "#2563eb", "solid", "^"),
    ("pm2", "#16a34a", "solid", "D"),
]


def plot(data_dir: str, title: str) -> None:
    fig, axes = plt.subplots(2, 1, figsize=(11, 8), sharex=True)
    has = False
    for name, color, ls, marker in SERIES:
        path = os.path.join(data_dir, f"{name}.csv")
        if not os.path.exists(path):
            continue
        df = pd.read_csv(path)
        if "time_ms" not in df.columns:
            continue
        t = df["time_ms"] / 1000.0
        mem = df["memory_mb"] if "memory_mb" in df.columns else None
        cpu = df["cpu_usage"] if "cpu_usage" in df.columns else None
        if mem is not None:
            axes[0].plot(t, mem, label=name, color=color, linestyle=ls, linewidth=2)
            has = True
        if cpu is not None:
            axes[1].plot(
                t,
                cpu.rolling(3, min_periods=1).mean(),
                label=name,
                color=color,
                linestyle=ls,
                linewidth=1.6,
                marker=marker,
                markevery=max(len(df) // 12, 1),
                markersize=4,
            )

    axes[0].set_ylabel("Daemon-set RSS (MiB)")
    axes[0].set_title(title or "Daemon RSS (not whole-host; RSS not PSS)")
    axes[0].grid(True, linestyle="--", alpha=0.4)
    axes[0].legend(loc="best", fontsize=9)
    axes[1].set_ylabel("CPU % (one core = 100)")
    axes[1].set_xlabel("Time (s)")
    axes[1].grid(True, linestyle="--", alpha=0.4)
    if not has:
        axes[0].text(0.5, 0.5, "no CSV", ha="center", transform=axes[0].transAxes)
    fig.tight_layout()
    out = os.path.join(data_dir, "report.png")
    fig.savefig(out, dpi=120)
    plt.close()
    print(f"Graph saved to {out}")


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("dir")
    p.add_argument("--title", default="")
    p.add_argument("--mode", choices=["compare", "self"], default="compare")
    args = p.parse_args()
    plot(args.dir, args.title)
