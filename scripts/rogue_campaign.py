"""Seeded deep-descent campaigns of the Rogue-only scenario through real processes.

Layer 2 of the tuning plan (docs/plans/rogue-tuning.md). Each seed starts a real
`tor-server` on `scenarios/rogue` and drives the locked adventurer with the
fixed earned-route policy (rogue_earned_route.Controller) through the headless
client, using only ordinary disclosures. There are no purchase decisions: the
scenario applies the authored build. Each run writes one run summary; the
campaign aggregates them for comparison with rogue_reference_model.py.

This is a fixed controller, not a model of human play. Its results are evidence
for the ratified run-shape tolerances, never a balance proof on their own.

    python scripts/rogue_campaign.py --output .local/campaign --seeds 0-7
"""
import argparse
from collections import Counter
import json
from pathlib import Path
import time
import unittest

from process_harness import ProcessTestCase, ROOT, settle_rogue_action
from rogue_earned_route import Controller, ControllerError
from scalar_wire import u64

SUMMARY = "tor-rogue-run-summary-1"
CAMPAIGN = "tor-rogue-campaign-1"
PACKAGE = ROOT / "scenarios/rogue"
HUNGER_BANDS = ("hungry", "weak", "empty")


def observation(frame):
    return frame["state"]["observation"]


def summarize(controller, samples, outcome, *, seed, wall_seconds):
    """One versioned run summary from the controller's disclosed history.

    Fields the ordinary disclosures cannot support (wanderer arrivals, kills by
    species, traps triggered) are reported explicitly as unavailable.
    """
    entries = {int(depth): value for depth, value in controller.first_entries.items()}
    levels = {}
    for witness in controller.growth_history:
        level = u64(witness["hit_dice"]["total"]) - 1  # body racial HD is not a Rogue level
        for depth, entry in entries.items():
            if entry["tick"] >= witness["tick"]:
                levels[depth] = max(levels.get(depth, 0), level)
    deepest = max(entries)
    floor_ticks = {}
    ordered = sorted(entries)
    for index, depth in enumerate(ordered):
        following = entries[ordered[index + 1]]["tick"] if index + 1 < len(ordered) else samples[-1]["tick"]
        floor_ticks[depth] = u64(following) - u64(entries[depth]["tick"])
    hunger = Counter(sample["hunger"] for sample in samples)
    final = samples[-1]
    return {
        "summary": SUMMARY, "seed": seed, "outcome": outcome,
        "died": outcome == "death", "deepest_depth": deepest,
        "death_depth": deepest if outcome == "death" else None,
        "final_tick": final["tick"], "commands": controller.commands,
        "level_on_arrival": {str(depth): level for depth, level in sorted(levels.items())},
        "ticks_per_floor": {str(depth): ticks for depth, ticks in sorted(floor_ticks.items())},
        "commands_per_floor": {str(depth): count for depth, count in sorted(controller.floor_commands.items())},
        "samples_by_hunger": dict(hunger),
        "hungry_share": sum(hunger[band] for band in HUNGER_BANDS) / len(samples),
        "meals_eaten": sum(1 for sample in samples if sample["phase"] == "eat"),
        "rest_episodes": controller.rest_episodes,
        "final_level": u64(final["hit_dice"]) - 1,
        "final_health": [final["hp"], final["max_hp"]],
        "earned_xp": final["earned_xp"],
        "wall_seconds": wall_seconds,
        "unavailable": ["wanderer_arrivals", "kills_by_species", "traps_triggered", "food_found"],
    }


class CampaignRun(ProcessTestCase):
    """One seeded descent. Constructed by run_seed; never collected as a test."""
    seed = 0
    floor_limit = 2500
    run_limit = 65000
    result = None

    def advancement(self, player):
        frame = self.command(player, {"type": "advancement", "command": "overview"})
        if frame["error"]:
            raise ControllerError(f"Overview refused: {frame['error']}")
        return frame["advancement_page"]

    def settled(self, player):
        """A state where the adventurer can act or the run is terminal."""
        for _ in range(200):
            frame = self.request(player, {"type": "snapshot"})
            combat = observation(frame)["combat"]
            if combat["terminal"] or combat["dead"] or (
                    observation(frame)["ready"] and combat["preparation_remaining"] is None):
                return frame
            time.sleep(0.02)
        raise ControllerError("Adventurer never became ready")

    def runTest(self):
        started = time.monotonic()
        self.server(scenario=PACKAGE, seed=self.seed)
        player, _ = self.client(omit_memory=True)
        controller = Controller(1, floor_limit=self.floor_limit, run_limit=self.run_limit)
        # The scenario grows the locked build; there is no first-HD purchase phase.
        controller.segment = "continuation"
        samples, outcome, refusal = [], None, None
        while outcome is None:
            frame = self.settled(player)
            state = frame["state"]
            controller.observe(state, self.advancement(player))
            combat = observation(frame)["combat"]
            decision = controller.choose(state)
            samples.append({"tick": state["observation"]["tick"], "phase": decision.phase,
                            "hp": u64(combat["hp"]), "max_hp": u64(combat["max_hp"]),
                            "hit_dice": combat["own_stats"]["hit_dice"]["total"],
                            "earned_xp": controller._advancement["growth"]["earned_xp"],
                            "hunger": combat["own_stats"]["nutrition"].get("hunger") or "nourished"})
            if decision.outcome is not None:
                outcome = decision.outcome
                break
            controller.consume_command("action", action=decision.action)
            reply = self.act(player, decision.action)
            if (reply.get("error") or "").startswith("ActorBusy") and not controller._at_budget():
                # A movement restriction (bear trap, hold) refuses the step; wait
                # out the restriction as a Rogue player would, then replan.
                controller._pending_stair = None
                controller.consume_command("action", action={"type": "wait"})
                reply = self.act(player, {"type": "wait"})
            if reply.get("error"):
                outcome = "refused_action"
                refusal = {"action": decision.action, "phase": decision.phase,
                           "error": reply["error"]}
                break
            # Paid work paused by danger resumes only with disclosed permission.
            settle_rogue_action(self, player, reply, survive=False)
        CampaignRun.result = summarize(controller, samples, outcome, seed=self.seed,
                                       wall_seconds=time.monotonic() - started)
        CampaignRun.result["refusal"] = refusal


def run_seed(seed, floor_limit, run_limit):
    case = type("SeededCampaignRun", (CampaignRun,), {
        "seed": seed, "floor_limit": floor_limit, "run_limit": run_limit})("runTest")
    CampaignRun.result = None
    if not hasattr(CampaignRun, "bin"):
        CampaignRun.setUpClass()  # unittest only does this for collected suites
    outcome = unittest.TestResult()
    case.run(outcome)
    if not outcome.wasSuccessful():
        problems = [text for _, text in outcome.errors + outcome.failures]
        return {"summary": SUMMARY, "seed": seed, "outcome": "harness_error",
                "error": problems[-1][-2000:] if problems else "unknown"}
    return CampaignRun.result


def aggregate(runs):
    """Campaign-level shape: death depths, levels at depth and hunger share."""
    completed = [run for run in runs if run["outcome"] != "harness_error"]
    deaths = Counter(run["death_depth"] for run in completed if run["died"])
    levels = {}
    for run in completed:
        for depth, level in run["level_on_arrival"].items():
            levels.setdefault(int(depth), []).append(level)
    return {
        "campaign": CAMPAIGN, "runs": len(runs), "completed": len(completed),
        "outcomes": dict(Counter(run["outcome"] for run in runs)),
        "death_depths": {str(depth): count for depth, count in sorted(deaths.items())},
        "deepest_depths": sorted(run["deepest_depth"] for run in completed),
        "mean_level_on_arrival": {str(depth): sum(values) / len(values)
                                  for depth, values in sorted(levels.items())},
        "mean_hungry_share": (sum(run["hungry_share"] for run in completed) / len(completed)
                              if completed else None),
    }


def parse_seeds(text):
    first, _, last = text.partition("-")
    return list(range(int(first), int(last or first) + 1))


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--seeds", default="0-3", type=parse_seeds)
    parser.add_argument("--floor-limit", default=2500, type=int)
    parser.add_argument("--run-limit", default=65000, type=int)
    args = parser.parse_args(argv)
    args.output.mkdir(parents=True, exist_ok=True)
    runs = []
    for seed in args.seeds:
        run = run_seed(seed, args.floor_limit, args.run_limit)
        (args.output / f"run-{seed}.json").write_text(json.dumps(run, indent=2), encoding="utf-8")
        print(seed, run["outcome"], run.get("deepest_depth"), run.get("final_level"), flush=True)
        runs.append(run)
    report = aggregate(runs)
    (args.output / "campaign.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
