#!/usr/bin/env python3
"""
Plot the separation between any two catalog bodies using local SPICE ephemerides.

Bodies are resolved via `configs/bodies`; ensure kernels are present
by running `cargo run -p solar_cli --bin fetch_spice` beforehand.
"""

from __future__ import annotations

import argparse
from datetime import datetime
from pathlib import Path
from typing import Iterable, Tuple

import matplotlib.pyplot as plt
import numpy as np
import yaml

try:
    import spiceypy as spice
except ImportError as exc:  # pragma: no cover - depends on environment
    raise SystemExit(
        "spiceypy is required for this script. Install it with `pip install spiceypy`."
    ) from exc


PROJECT_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SPICE_DIR = PROJECT_ROOT / "data" / "spice"
DEFAULT_SCENARIO_PATH = PROJECT_ROOT / "configs" / "bodies"


def load_kernels(spice_dir: Path) -> None:
    """Load every kernel in the given directory into the SPICE kernel pool."""
    if not spice_dir.exists():
        raise SystemExit(
            f"SPICE directory {spice_dir} is missing. Run `cargo run -p solar_cli --bin fetch_spice` first."
        )

    loaded = 0
    for path in sorted(spice_dir.glob("*")):
        if path.suffix.lower() in {".bsp", ".bc", ".tls", ".tpc", ".tf"}:
            spice.furnsh(str(path))
            loaded += 1

    if loaded == 0:
        raise SystemExit(
            f"No SPICE kernels were loaded from {spice_dir}. "
            "Ensure `fetch_spice` downloaded the kernel set."
        )


def load_planet_catalog(path: Path) -> dict:
    """Load planet/moon definitions and build a case-insensitive lookup."""
    if not path.exists():
        raise SystemExit(f"Scenario catalog not found at {path}")

    with path.open("r", encoding="utf-8") as handle:
        data = yaml.safe_load(handle)

    if not isinstance(data, list):
        raise SystemExit(f"Unexpected catalog format in {path}: expected a list")

    catalog: dict[str, dict] = {}
    for entry in data:
        if not isinstance(entry, dict):
            continue
        name = entry.get("name")
        spice_name = entry.get("spice_name")
        if not name or not spice_name:
            continue
        catalog[name.upper()] = entry
        catalog[spice_name.upper()] = entry
    if not catalog:
        raise SystemExit(f"No valid bodies found in {path}")
    return catalog


def resolve_body(identifier: str, catalog: dict[str, dict]) -> dict:
    """Resolve a body identifier (name or SPICE name) to catalog entry."""
    entry = catalog.get(identifier.upper())
    if entry is None:
        available = ", ".join(sorted({item["name"] for item in catalog.values()}))
        raise SystemExit(
            f"Body '{identifier}' not found in catalog. Available names: {available}"
        )
    return entry


def compute_distances(
    start_epoch: str,
    end_epoch: str,
    step_days: float,
    target: str,
    observer: str,
    frame: str = "ECLIPJ2000",
    correction: str = "NONE",
) -> Tuple[np.ndarray, np.ndarray]:
    """Return (timestamps, distances_km) sampled between two epochs."""
    et_start = spice.str2et(start_epoch)
    et_end = spice.str2et(end_epoch)
    if et_end <= et_start:
        raise ValueError("end epoch must be after start epoch")

    step_seconds = max(step_days, 0.01) * 86_400.0
    ets = np.arange(et_start, et_end + step_seconds, step_seconds)

    distances = np.empty_like(ets)
    timestamps = np.empty(ets.shape[0], dtype=object)

    for idx, et in enumerate(ets):
        target_pos, _ = spice.spkpos(target, et, frame, correction, "SUN")
        observer_pos, _ = spice.spkpos(observer, et, frame, correction, "SUN")
        delta = np.subtract(target_pos, observer_pos)
        distances[idx] = np.linalg.norm(delta)
        utc = spice.et2utc(et, "C", 3)
        timestamps[idx] = datetime.strptime(utc, "%Y %b %d %H:%M:%S.%f")

    return timestamps, distances


def plot_distance(
    times: Iterable[datetime],
    distances_km: Iterable[float],
    output_path: Path,
    label_target: str,
    label_observer: str,
) -> None:
    """Generate and save the Earth–Mars distance plot."""
    fig, ax = plt.subplots(figsize=(10, 5))
    distances_mkm = np.asarray(distances_km) / 1.0e6
    ax.plot(times, distances_mkm, lw=1.5, color="tab:red")
    ax.set_xlabel("Date (UTC)")
    ax.set_ylabel("Distance (million km)")
    ax.set_title(f"{label_target} – {label_observer} Separation")
    ax.grid(True, linestyle="--", alpha=0.4)

    # Highlight extrema to visualize minimum and maximum separation.
    min_idx = int(np.argmin(distances_mkm))
    max_idx = int(np.argmax(distances_mkm))
    extrema = [
        ("Min", min_idx, "tab:green"),
        ("Max", max_idx, "tab:blue"),
    ]
    for label, idx, color in extrema:
        ax.scatter(times[idx], distances_mkm[idx], color=color, s=60, zorder=3)
        ax.annotate(
            f"{label}: {distances_mkm[idx]:.1f} Mkm\n{times[idx]:%Y-%m-%d}",
            xy=(times[idx], distances_mkm[idx]),
            xytext=(10, 10),
            textcoords="offset points",
            fontsize=9,
            color=color,
            bbox=dict(boxstyle="round,pad=0.3", fc="white", ec=color, lw=0.8),
            arrowprops=dict(arrowstyle="->", color=color, lw=0.8),
        )

    fig.autofmt_xdate()
    fig.tight_layout()
    fig.savefig(output_path, dpi=200)
    plt.close(fig)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Plot Earth–Mars distance using local SPICE ephemerides."
    )
    parser.add_argument(
        "--start",
        default="2025 JAN 01 00:00:00 TDB",
        help="Start epoch (SPICE string, default: 2025 JAN 01 00:00:00 TDB)",
    )
    parser.add_argument(
        "--end",
        default="2027 JAN 01 00:00:00 TDB",
        help="End epoch (SPICE string, default: 2027 JAN 01 00:00:00 TDB)",
    )
    parser.add_argument(
        "--step-days",
        type=float,
        default=5.0,
        help="Sampling cadence in days (default: 5)",
    )
    parser.add_argument(
        "--output",
        default=str(PROJECT_ROOT / "artifacts" / "earth_mars_distance.png"),
        help="Path to save the PNG plot (default: artifacts/earth_mars_distance.png)",
    )
    parser.add_argument(
        "--scenario",
        default=str(DEFAULT_SCENARIO_PATH),
        help="Path to bodies catalog (TOML directory or legacy YAML)",
    )
    parser.add_argument(
        "--body-a",
        default="EARTH",
        help="First body name/identifier from the catalog (default: EARTH)",
    )
    parser.add_argument(
        "--body-b",
        default="MARS",
        help="Second body name/identifier from the catalog (default: MARS)",
    )
    parser.add_argument(
        "--spice-dir",
        default=str(DEFAULT_SPICE_DIR),
        help="Directory containing SPICE kernels (default: data/spice)",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    spice_dir = Path(args.spice_dir)
    output_path = Path(args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)

    catalog = load_planet_catalog(Path(args.scenario))
    body_a = resolve_body(args.body_a, catalog)
    body_b = resolve_body(args.body_b, catalog)

    label_a = body_a["name"]
    label_b = body_b["name"]
    target_spice = body_a["spice_name"]
    observer_spice = body_b["spice_name"]

    load_kernels(spice_dir)
    try:
        times, distances = compute_distances(
            args.start, args.end, args.step_days, target_spice, observer_spice
        )
        plot_distance(times, distances, output_path, label_a, label_b)
    finally:
        spice.kclear()

    print(
        f"Saved {label_a}–{label_b} distance plot to {output_path} "
        f"(bodies: {target_spice} vs {observer_spice})"
    )


if __name__ == "__main__":
    main()
