"""Aggregate run_*/scenario CSVs: median, IQR, CV. Does not invent a composite score.

Supports the formal topology: four like-for-like hosts, one arm per host.
  python3 analysis/summarize.py --merge oss/ pro/ sv/ pm2/ -o report/
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

import matplotlib.pyplot as plt
import pandas as pd

ARMS = ["super-oss", "super-pro", "supervisord", "pm2"]
COLORS = {
    "super-oss": "#b45309",
    "super-pro": "#f59e0b",
    "supervisord": "#2563eb",
    "pm2": "#16a34a",
}
HATCH = {"super-oss": "", "super-pro": "//", "supervisord": "", "pm2": ""}


def steady_median(series: pd.Series, cv_pct: float = 10.0) -> dict:
    s = series.dropna()
    if s.empty:
        return {"n": 0}
    # Walk windows of 20% of samples from the end until CV < threshold.
    n = len(s)
    w = max(n // 5, 8)
    window = s.iloc[-w:]
    mean = float(window.mean())
    std = float(window.std(ddof=0))
    cv = (std / mean * 100) if mean else 0.0
    q1, med, q3 = (float(window.quantile(x)) for x in (0.25, 0.5, 0.75))
    return {
        "n": int(len(window)),
        "median": med,
        "p95": float(window.quantile(0.95)),
        "iqr": q3 - q1,
        "cv_pct": cv,
        "steady_cv_ok": cv < cv_pct,
    }


def collect(results: Path) -> dict:
    out: dict = {"scenarios": {}}
    for csv in results.glob("run_*/**/data/*.csv"):
        arm = csv.stem
        scenario = csv.parent.parent.name
        run = csv.parts[-4] if len(csv.parts) >= 4 else "run"
        df = pd.read_csv(csv)
        rec = out["scenarios"].setdefault(scenario, {}).setdefault(arm, [])
        row = {"run": run}
        if "memory_mb" in df.columns:
            row["rss"] = steady_median(df["memory_mb"])
        if "cpu_usage" in df.columns:
            row["cpu"] = {
                "median": float(df["cpu_usage"].median()),
                "max": float(df["cpu_usage"].max()),
            }
        rec.append(row)
    return out


def collect_gradient(results: Path) -> dict:
    """RES-1 scalability gradient: daemon-set RSS median vs managed-process count.

    Reads results/gradient/RES-1/n*/data/{arm}.csv and returns
    {arm: {n: steady_median(memory_mb)}}.
    """
    out: dict = {}
    for csv in results.glob("gradient/RES-1/n*/data/*.csv"):
        arm = csv.stem
        n = int(csv.parent.parent.name.removeprefix("n"))
        df = pd.read_csv(csv)
        if "memory_mb" not in df.columns:
            continue
        out.setdefault(arm, {})[n] = steady_median(df["memory_mb"])
    return out


def cross_run_median(points: list[float]) -> dict:
    s = pd.Series(points)
    if s.empty:
        return {}
    return {
        "median_of_medians": float(s.median()),
        "iqr": float(s.quantile(0.75) - s.quantile(0.25)),
        "range": float(s.max() - s.min()),
        "n_runs": int(len(s)),
    }


def plot_bars(summary: dict, out: Path) -> None:
    # One figure per scenario: grouped bars of RSS median-of-medians with IQR whiskers.
    scenarios = sorted(summary["scenarios"])
    if not scenarios:
        return
    fig, axes = plt.subplots(max(len(scenarios), 1), 1, figsize=(10, 3.2 * max(len(scenarios), 1)))
    if len(scenarios) == 1:
        axes = [axes]
    for ax, sc in zip(axes, scenarios):
        xs, ys, yerr, colors, hatches, labels = [], [], [], [], [], []
        for i, arm in enumerate(ARMS):
            runs = summary["scenarios"].get(sc, {}).get(arm, [])
            meds = [r["rss"]["median"] for r in runs if "rss" in r and "median" in r["rss"]]
            if not meds:
                continue
            agg = cross_run_median(meds)
            xs.append(i)
            ys.append(agg["median_of_medians"])
            yerr.append(agg["iqr"] / 2 if agg["n_runs"] > 1 else 0)
            colors.append(COLORS[arm])
            hatches.append(HATCH[arm])
            labels.append(arm)
        if not xs:
            ax.set_title(f"{sc} (no data)")
            continue
        bars = ax.bar(xs, ys, color=colors, yerr=yerr, capsize=4, edgecolor="#111")
        for bar, h in zip(bars, hatches):
            bar.set_hatch(h)
        ax.set_xticks(xs, labels, rotation=15)
        ax.set_ylabel("RSS MiB (median of run medians)")
        ax.set_title(f"{sc} — daemon-set RSS (IQR whiskers; not a score)")
        ax.grid(True, axis="y", linestyle="--", alpha=0.4)
    fig.tight_layout()
    fig.savefig(out / "summary_rss.png", dpi=120)
    plt.close()


def plot_gradient(gradient: dict, out: Path) -> None:
    """Daemon-set RSS median vs managed-process count (RES-1 scalability)."""
    if not gradient:
        return
    fig, ax = plt.subplots(figsize=(8, 5))
    for arm in ARMS:
        data = gradient.get(arm)
        if not data:
            continue
        ns = sorted(data)
        meds = [data[n].get("median") for n in ns]
        if any(m is None for m in meds):
            continue
        ax.plot(ns, meds, marker="o", color=COLORS[arm], linestyle="-" if arm != "super-pro" else "--",
                label=arm)
        # annotate one point with IQR to show spread
        mid = ns[len(ns) // 2]
        ax.annotate(f"{data[mid]['median']:.1f}±{data[mid]['iqr']:.1f}", (mid, data[mid]["median"]),
                    textcoords="offset points", xytext=(4, 4), fontsize=8)
    ax.set_xlabel("managed processes (N)")
    ax.set_ylabel("daemon-set RSS median (MiB)")
    ax.set_title("RES-1 scalability — daemon-set RSS vs N (not a score)")
    ax.grid(True, linestyle="--", alpha=0.4)
    ax.legend(loc="best")
    fig.tight_layout()
    fig.savefig(out / "res1_gradient.png", dpi=120)
    plt.close()


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("dirs", nargs="+", help="one or more result dirs (one per arm)")
    p.add_argument("--out", default=".", help="output directory (default cwd)")
    args = p.parse_args()

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)

    merged: dict = {"scenarios": {}}
    gradient: dict = {}
    manifests = {}
    for d in args.dirs:
        results = Path(d)
        one = collect(results)
        for sc, arms in one["scenarios"].items():
            for arm, runs in arms.items():
                merged["scenarios"].setdefault(sc, {}).setdefault(arm, []).extend(runs)
        for arm, ns in collect_gradient(results).items():
            gradient.setdefault(arm, {}).update(ns)
        if (results / "manifest.json").exists():
            manifests[d] = (results / "manifest.json").read_text()

    for sc, arms in merged["scenarios"].items():
        for arm, runs in list(arms.items()):
            meds = [r["rss"]["median"] for r in runs if "rss" in r and "median" in r["rss"]]
            if meds:
                arms[arm] = {"runs": runs, "aggregate": cross_run_median(meds)}
    merged["gradient"] = gradient

    (out / "summary.json").write_text(json.dumps(merged, indent=2) + "\n")
    if manifests:
        (out / "manifests.json").write_text(json.dumps(manifests, indent=2) + "\n")
    plot_bars(merged, out)
    plot_gradient(gradient, out)
    print(f"wrote {out / 'summary.json'}, summary_rss.png, res1_gradient.png")


if __name__ == "__main__":
    main()
