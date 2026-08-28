"""Aggregate run_*/scenario CSVs: median, IQR, CV. Does not invent a composite score."""
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


def plot_bars(summary: dict, results: Path) -> None:
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
    fig.savefig(results / "summary_rss.png", dpi=120)
    plt.close()


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("results")
    args = p.parse_args()
    results = Path(args.results)
    summary = collect(results)
    plot_bars(summary, results)
    for sc, arms in summary["scenarios"].items():
        for arm, runs in list(arms.items()):
            meds = [r["rss"]["median"] for r in runs if "rss" in r and "median" in r["rss"]]
            if meds:
                arms[arm] = {"runs": runs, "aggregate": cross_run_median(meds)}
    (results / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    plot_bars(summary, results)
    print(f"wrote {results / 'summary.json'} and summary_rss.png")


if __name__ == "__main__":
    main()
