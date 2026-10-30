"""Measure Rogue item generation on first floors across many seeds.

Each seed starts a real server on `scenarios/rogue`, connects the adventurer
(which generates depth 1) and reads the saved first-entry receipts. The pooled
items and food per floor are compared with Rogue's expectation, nine 36%
attempts with food at 16% of items, with a normal-approximation z score.

    python scripts/rogue_generation_probe.py --seeds 0-119 --work-root C:/fast
"""
import argparse
import json
import math
from pathlib import Path
import unittest

from process_harness import ProcessTestCase
import rogue_campaign

PACKAGE = rogue_campaign.PACKAGE
EXPECTED_FOOD = 9 * 0.36 * 0.16


class FirstFloor(ProcessTestCase):
    """One seed's depth 1. Constructed by probe(); never collected as a test."""
    seed = 0
    result = None

    def runTest(self):
        self.server(scenario=PACKAGE, seed=self.seed)
        self.client(omit_memory=True)
        FirstFloor.result = rogue_campaign.item_generation(self.private_receipts())


def probe(seed, work_root=None):
    case = type("SeededFirstFloor", (FirstFloor,), {"seed": seed, "process_root": work_root})("runTest")
    FirstFloor.result = None
    if not hasattr(FirstFloor, "bin"):
        FirstFloor.setUpClass()
    outcome = unittest.TestResult()
    case.run(outcome)
    if outcome.errors or outcome.failures:
        raise RuntimeError((outcome.errors + outcome.failures)[0][1])
    return FirstFloor.result


def pooled(results):
    floors = sum(row["floors"] for row in results)
    food = sum(row["food_per_floor"] * row["floors"] for row in results)
    items = sum(row["items_per_floor"] * row["floors"] for row in results)
    # Food per floor is a sum of independent attempts: variance about its mean.
    z = (food - EXPECTED_FOOD * floors) / math.sqrt(EXPECTED_FOOD * floors) if floors else None
    return {"floors": floors, "food": food, "food_per_floor": food / floors if floors else None,
            "expected_food_per_floor": EXPECTED_FOOD,
            "items_per_floor": items / floors if floors else None, "food_z": z}


def seeds(text):
    first, _, last = text.partition("-")
    return range(int(first), int(last or first) + 1)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--seeds", type=seeds, default=seeds("0-119"))
    parser.add_argument("--work-root", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args(argv)
    results = [probe(seed, args.work_root) for seed in args.seeds]
    report = {"per_seed": results, "pooled": pooled(results)}
    if args.output:
        args.output.write_text(json.dumps(report, indent=1), encoding="utf-8")
    print(json.dumps(report["pooled"], indent=1))


if __name__ == "__main__":
    main()
