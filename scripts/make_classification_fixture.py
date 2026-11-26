#!/usr/bin/env python3
"""Write the deterministic CSV fixture used by the classifier smoke test.

This is synthetic data, generated only to exercise the import/profile/train
path. It is not presented as a real measurement anywhere in ATOM's interface;
the README says so when it is used for a screenshot.
"""

from __future__ import annotations

import argparse
import math
import random
from pathlib import Path

ROWS = 600


def label_for(temp: float, humidity: float, wind: float, mode: str) -> str:
    score = temp * 0.55 + humidity * 0.25 - wind * 0.45
    if mode == "fast":
        score += 5
    elif mode == "eco":
        score -= 4
    if score > 24.0:
        return "heating"
    if score < 16.5:
        return "cooling"
    return "ventilation"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()

    rng = random.Random(20261126)
    modes = ["auto", "eco", "fast"]
    rooms = ["kitchen", "office", "bedroom", "garage"]
    lines = ["temperature_c,humidity_pct,wind_kmh,mode,room,hvac_action"]
    for index in range(ROWS):
        temp = round(rng.gauss(21, 6), 1)
        humidity = round(min(95, max(20, rng.gauss(55, 14))), 1)
        wind = round(abs(rng.gauss(12, 6)), 1)
        mode = modes[index % len(modes)]
        room = rooms[(index // 3) % len(rooms)]
        action = label_for(temp, humidity, wind, mode)
        # A little label noise keeps the task from being trivially separable.
        if index % 17 == 0:
            action = "ventilation" if action != "ventilation" else "heating"
        # Missing values and one malformed numeric cell appear on purpose so the
        # profile has something real to report.
        temp_cell = "" if index % 41 == 0 else f"{temp}"
        humidity_cell = "n/a" if index % 53 == 0 else f"{humidity}"
        wind_cell = "unknown" if index % 97 == 0 else f"{wind}"
        lines.append(f"{temp_cell},{humidity_cell},{wind_cell},{mode},{room},{action}")

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text("\n".join(lines) + "\n", encoding="utf-8")
    counts = {}
    for line in lines[1:]:
        counts[line.rsplit(",", 1)[1]] = counts.get(line.rsplit(",", 1)[1], 0) + 1
    print(f"wrote {args.output} ({len(lines) - 1} rows)")
    for label, count in sorted(counts.items()):
        print(f"  {label}: {count}")


if __name__ == "__main__":
    main()
