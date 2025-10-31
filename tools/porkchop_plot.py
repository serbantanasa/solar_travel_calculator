#!/usr/bin/env python3
import argparse
from datetime import datetime, timedelta

import numpy as np
import pandas as pd
import matplotlib.pyplot as plt
import matplotlib.dates as mdates


def load_grid(csv_path: str) -> pd.DataFrame:
    df = pd.read_csv(csv_path)
    df["feasible"] = df["feasible"].astype(str).str.lower()
    df = df[df["feasible"] == "true"].copy()
    df["depart_dt"] = pd.to_datetime(
        df["depart_utc"], format="%Y %b %d %H:%M:%S.%f", errors="coerce"
    )
    df["arrive_dt"] = pd.to_datetime(
        df["arrive_utc"], format="%Y %b %d %H:%M:%S.%f", errors="coerce"
    )
    df = df.dropna(subset=["depart_dt", "arrive_dt"])
    return df


def make_plot(
    df: pd.DataFrame,
    output: str,
    metric: str = "dv_total_km_s",
    high_clip_factor: float = 4.0,
) -> None:
    # Build a rectangular grid (rows = arrivals, columns = departures)
    pivot = df.pivot_table(
        index="arrive_dt",
        columns="depart_dt",
        values=metric,
        aggfunc="min",
    )
    pivot = pivot.dropna(axis=0, how="all").dropna(axis=1, how="all")
    if pivot.empty:
        raise ValueError("No feasible Lambert solutions in the provided CSV")

    dep_dates = pivot.columns.to_pydatetime()
    arr_dates = pivot.index.to_pydatetime()
    dep_ord = mdates.date2num(dep_dates)
    arr_ord = mdates.date2num(arr_dates)
    Z = np.ma.masked_invalid(pivot.values)
    if np.ma.is_masked(Z):
        Z = np.ma.masked_invalid(Z)
    zmin = float(Z.min())
    zmax = float(Z.max())
    limit = zmin * high_clip_factor
    if zmax > limit:
        Z = np.ma.clip(Z, zmin, limit)
        zmax = limit
    if not np.isfinite(zmin) or not np.isfinite(zmax) or zmin >= zmax:
        raise ValueError("Unable to determine contour levels (check CSV contents)")

    fig, ax = plt.subplots(figsize=(8, 6))
    levels = np.linspace(zmin, zmax, 30)

    cmap = plt.get_cmap("jet")
    cf = ax.contourf(dep_ord, arr_ord, Z, levels=levels, cmap=cmap)
    cs = ax.contour(dep_ord, arr_ord, Z, levels=levels, colors="k", linewidths=0.4)

    cbar = fig.colorbar(cf, ax=ax, pad=0.02)
    cbar.set_label("Total Δv (km/s)", rotation=270, labelpad=15)

    ax.clabel(cs, cs.levels[::4], inline=True, fmt="%.2f", fontsize=8)
    ax.set_xlabel("Departure Date")
    ax.set_ylabel("Arrival Date")
    ax.set_title("Earth → Mars Launch Window")
    ax.xaxis.set_major_formatter(mdates.DateFormatter("%m/%d/%y"))
    ax.yaxis.set_major_formatter(mdates.DateFormatter("%m/%d/%y"))
    fig.autofmt_xdate()

    # mark minimum Δv point
    min_row = df.loc[df[metric].idxmin()]
    ax.axvline(min_row["depart_dt"], color="k", linestyle="--", linewidth=0.8)
    ax.axhline(min_row["arrive_dt"], color="k", linestyle="--", linewidth=0.8)
    ax.plot(min_row["depart_dt"], min_row["arrive_dt"], "k+", markersize=10, mew=2)
    label = f"Δv = {min_row[metric]:.2f} km/s"
    ax.text(
        min_row["depart_dt"],
        min_row["arrive_dt"],
        f" {label}",
        color="k",
        fontsize=10,
        verticalalignment="bottom",
        horizontalalignment="left",
    )
    ax.legend(["Minimum Δv"], loc="upper left")

    fig.tight_layout()
    fig.savefig(output, dpi=300)
    plt.close(fig)


def main() -> None:
    parser = argparse.ArgumentParser(description="Matplotlib porkchop contour plotter")
    parser.add_argument("--input", required=True, help="CSV produced by `porkchop` binary")
    parser.add_argument("--output", required=True, help="Output PNG file")
    parser.add_argument("--metric", default="dv_total_km_s", help="Metric column to contour")
    parser.add_argument(
        "--high-clip-factor",
        type=float,
        default=4.0,
        help="Clip values above factor * min to highlight valleys",
    )

    args = parser.parse_args()
    df = load_grid(args.input)
    make_plot(df, args.output, args.metric, args.high_clip_factor)


if __name__ == "__main__":
    main()
