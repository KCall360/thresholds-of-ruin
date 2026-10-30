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
import shutil
import subprocess
import tomllib
from pathlib import Path
import time
import unittest

from arena_matrix import toml_source
from process_harness import OutputQueue, Process, ProcessTestCase, ROOT, settle_rogue_action
import rogue_earned_route
import rogue_reference_model
from rogue_earned_route import Controller, ControllerError
from rogue_test_support import DisclosedSearchSweep
from scalar_wire import u64

SUMMARY = "tor-rogue-run-summary-1"
CAMPAIGN = "tor-rogue-campaign-1"
PACKAGE = ROOT / "scenarios/rogue"
HUNGER_BANDS = ("hungry", "weak", "empty")
FOOD = {"rogue.ration", "rogue.fruit"}
HEALING = ("rogue.healing", "rogue.extra_healing")
# Rogue's armor table, best first: (archetype, armor class, draw weight).
ARMOR = [("rogue.plate_mail", 3, 5), ("rogue.banded_mail", 4, 10), ("rogue.splint_mail", 4, 10),
         ("rogue.chain_mail", 5, 12), ("rogue.scale_mail", 6, 13), ("rogue.ring_mail", 7, 15)]
DRINK_BELOW = 0.4
ITEM_STAGES = {"rogue.items", "rogue.items_other", "rogue.food_drought", "rogue.treasure_room"}


def healing_potion(state):
    """The id of a carried, identified healing potion, if any."""
    for row in state["observation"]["inventory"]:
        identity = (row.get("known_identity") or {}).get("id") if isinstance(
            row.get("known_identity"), dict) else row.get("known_identity")
        if row.get("identified") and int(row["quantity"]) > 0 and (
                identity in HEALING or row["name"].startswith("potion of") and "healing" in row["name"]):
            return row["id"]
    return None


def placements(result):
    """Placed item contents of one stage result, including room decisions."""
    rows = list(result.get("attempts", []))
    rows += [room["result"]["decision"] for room in result.get("rooms", [])
             if room["result"].get("kind") == "attempt"]
    selection = result.get("selection", {})
    rows += selection.get("items", [])
    return [row["content"] for row in rows
            if row.get("kind") == "placed" and row["entity"]["kind"] == "item"]


def item_generation(receipts):
    """Per generated floor: items and food placed by Rogue's item stages.

    Read from the private first-entry receipts after the run, so it measures
    what actually landed, including placements skipped for lack of a cell.
    """
    floors = {}
    for record in receipts:
        contents = [content for decision in record["decisions"] if decision["id"] in ITEM_STAGES
                    for content in placements(decision["result"])]
        drought = next((len(placements(decision["result"])) for decision in record["decisions"]
                        if decision["id"] == "rogue.food_drought"), 0)
        floors[record["scope"]] = {"items": len(contents),
                                   "food": sum(content in FOOD for content in contents),
                                   "drought": drought}
    count = len(floors)
    return {"floors": count,
            "items_per_floor": sum(row["items"] for row in floors.values()) / count if count else None,
            "food_per_floor": sum(row["food"] for row in floors.values()) / count if count else None,
            "drought_floors": sum(row["drought"] for row in floors.values())}
MAX_PATIENT_WAITS = 20
MAX_REFUSED_STEPS = 5
RESPONSE_SECONDS = 90


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
        # Where each floor's commands went, to tell a stalled bot from a slow floor.
        "phases_per_floor": {str(depth): dict(Counter(sample["phase"] for sample in samples
                                                      if sample.get("depth") == depth))
                             for depth in ordered},
        "rest_episodes": controller.rest_episodes,
        "final_level": u64(final["hit_dice"]) - 1,
        "final_health": [final["hp"], final["max_hp"]],
        "earned_xp": final["earned_xp"],
        "wall_seconds": wall_seconds,
        "unavailable": ["wanderer_arrivals", "kills_by_species", "traps_triggered"],
    }


class RollingOutputQueue(OutputQueue):
    """A disk spool that restarts once fully read and past `limit` bytes.

    The headless client prints several megabytes of frames per second, so an
    hour-long run would otherwise fill a volume. Frame waits read lines as they
    arrive and never read the transcript back; the bounded diagnostic tail is
    kept in memory by the transcript.
    """
    limit = 64 * 1024 * 1024

    def get(self, timeout):
        line = super().get(timeout)
        with self.condition:
            if not self.count and self.offset > self.limit:
                self.writer.flush()
                self.writer.seek(0)
                self.writer.truncate()
                self.reader.seek(0)
                self.offset = 0
        return line


class CampaignClient(Process):
    output_queue = RollingOutputQueue


class CampaignRun(ProcessTestCase):
    """One seeded descent. Constructed by run_seed; never collected as a test."""
    seed = 0
    start_depth = 1
    floor_limit = 2500
    run_limit = 65000
    result = None

    def launch(self, name, args, cls=Process, **kwargs):
        if name == "tor-client-headless" and cls is Process:
            cls = CampaignClient
        return super().launch(name, args, cls, **kwargs)

    def frame(self, client, predicate, seconds=15):
        # Long saved histories make some replies slower than the test default.
        return super().frame(client, predicate, max(seconds, RESPONSE_SECONDS))

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

    def deep_package(self):
        """A copy that starts at `start_depth`, grown to the model's level there.

        The adventurer arrives at that floor's up stairs, as after a descent,
        with the scheduled rows for the expected level already grown and one
        spare ration. Earlier floors are never generated.
        """
        package = self.directory / "rogue-deep"
        shutil.copytree(PACKAGE, package, ignore=shutil.ignore_patterns(".compiled"))
        manifest = tomllib.loads((package / "scenario.toml").read_text(encoding="utf-8"))
        level = deep_start_level(self.start_depth)
        character = manifest["characters"][0]
        character["anchor"] = f"dungeon/{self.start_depth}/up"
        armor = expected_armor(self.start_depth)
        for row in character["loadout"]:
            if row["archetype"] == "rogue.ration":
                row["quantity"] += 1
            if row["archetype"] == "rogue.ring_mail":
                row["archetype"] = armor
        potions = expected_healing(self.start_depth)
        if potions:
            character["loadout"].append({"archetype": "rogue.healing", "quantity": potions})
        character["known_identities"] += [armor, *HEALING]
        build = character["creature"]
        grown = min(level - 1, len(build["growth_schedule"]))
        build["hit_dice"] += build["growth_schedule"][:grown]
        build["growth_schedule"] = build["growth_schedule"][grown:]
        (package / "scenario.toml").write_text(toml_source(manifest), encoding="utf-8",
                                               newline="\n")
        compiler = self.bin / ("tor-scenario" + self.suffix)
        validated = subprocess.run([str(compiler), "validate", str(package)], capture_output=True,
                                   text=True, encoding="utf-8", timeout=300)
        if validated.returncode:
            raise ControllerError(f"Deep-start package rejected: {validated.stderr.strip()}")
        return package

    def runTest(self):
        started = time.monotonic()
        package = PACKAGE if self.start_depth == 1 else self.deep_package()
        self.server(scenario=package, seed=self.seed)
        player, _ = self.client(omit_memory=True)
        controller = Controller(1, floor_limit=self.floor_limit, run_limit=self.run_limit)
        # The scenario grows the locked build; there is no first-HD purchase phase.
        controller.segment = "continuation"
        controller.depth = self.start_depth
        if self.start_depth > 1:
            # The route's start check expects one class HD; a deep start has the
            # same one racial body HD plus its already grown scheduled rows.
            rogue_earned_route.starting_body_hd = lambda summary, character: 1
        samples, outcome, refusal = [], None, None
        sweeps, waits, searches, refused, deadlines = {}, 0, 0, 0, 0
        while outcome is None:
            frame = self.settled(player)
            state = frame["state"]
            controller.observe(state, self.advancement(player))
            combat = observation(frame)["combat"]
            decision = controller.choose(state)
            samples.append({"tick": state["observation"]["tick"], "phase": decision.phase,
                            "depth": controller.depth,
                            "hp": u64(combat["hp"]), "max_hp": u64(combat["max_hp"]),
                            "hit_dice": combat["own_stats"]["hit_dice"]["total"],
                            "earned_xp": controller._advancement["growth"]["earned_xp"],
                            "hunger": combat["own_stats"]["nutrition"].get("hunger") or "nourished"})
            action = decision.action
            potion = healing_potion(state) if decision.outcome is None else None
            if potion is not None and u64(combat["hp"]) < DRINK_BELOW * u64(combat["max_hp"]):
                # The one item use this fixed policy makes: a known healing potion
                # when Health is low, as a Rogue player would.
                action = {"type": "drink", "item": potion}
            if decision.outcome == "censored_route_exhausted" and not controller._at_budget():
                # Rogue hides stairs behind secret doors and passages: sweep the
                # disclosed floor with paid Search until a new frontier appears.
                sweep = sweeps.setdefault(controller.depth, DisclosedSearchSweep())
                sweep.observe(state["observation"])
                try:
                    action = sweep.choose(state, controller.floors[controller.depth].graph)
                except AssertionError:
                    action = None
            elif (decision.outcome == "censored_no_disclosed_combat_route"
                    and waits < MAX_PATIENT_WAITS and not controller._at_budget()):
                # A seen foe with no disclosed approach usually comes to us.
                waits += 1
                action = {"type": "wait"}
            elif decision.outcome is None:
                waits = 0
            if action is None:
                outcome = decision.outcome
                break
            searches += action.get("id") == "rogue.search"
            controller._planned = action
            controller.consume_command("action", action=action)
            decision = type(decision)(action, decision.phase, None)
            reply = self.act(player, decision.action)
            error = reply.get("error") or ""
            if (error.startswith("ActorBusy") or (error.startswith("InvalidAction")
                                                   and decision.action.get("type") == "move"))                     and refused < MAX_REFUSED_STEPS and not controller._at_budget():
                # A movement restriction (bear trap, hold) or a step the server
                # will not take refuses the move; wait as a Rogue player would,
                # then replan from the next settled state.
                refused += 1
                controller._pending_stair = None
                controller.consume_command("action", action={"type": "wait"})
                reply = self.act(player, {"type": "wait"})
            elif not error:
                refused = 0
            if reply.get("error"):
                outcome = "refused_action"
                refusal = {"action": decision.action, "phase": decision.phase,
                           "error": reply["error"]}
                break
            # Paid work paused by danger resumes only with disclosed permission.
            try:
                settle_rogue_action(self, player, reply, survive=False)
            except AssertionError as error:
                # Some completed actions publish no further intention update; the
                # next loop polls snapshots until the adventurer is ready again.
                if "Process output deadline" not in str(error):
                    raise
                deadlines += 1
        CampaignRun.result = summarize(controller, samples, outcome, seed=self.seed,
                                       wall_seconds=time.monotonic() - started)
        CampaignRun.result["refusal"] = refusal
        CampaignRun.result["searches"] = searches
        CampaignRun.result["settlement_deadlines"] = deadlines
        CampaignRun.result["item_generation"] = item_generation(self.private_receipts())


def expected_armor(depth):
    """Median best armor among Rogue's expected armor finds before `depth`.

    Armor is 7% of nine 36% item attempts per floor. With n finds, the best
    armor class is at most a given class with probability 1 - (1 - p)^n, where
    p is that class's share of the armor table (leather is the 20% remainder).
    """
    finds = (depth - 1) * 9 * 0.36 * 0.07
    covered = 0
    for archetype, _, weight in ARMOR:
        covered += weight / 100
        if 1 - (1 - covered) ** finds >= 0.5:
            return archetype
    return "rogue.ring_mail"


def expected_healing(depth):
    """Half the expected healing and extra-healing potions found before `depth`."""
    per_floor = 9 * 0.36 * 0.26 * 0.18
    return round(per_floor * (depth - 1) / 2)


def deep_start_level(depth):
    """The reference model's expected level on arrival at `depth`."""
    [row] = rogue_reference_model.expectations({depth})
    return row.level_on_arrival


def run_seed(seed, floor_limit, run_limit, start_depth=1, work_root=None):
    case = type("SeededCampaignRun", (CampaignRun,), {
        "seed": seed, "floor_limit": floor_limit, "run_limit": run_limit,
        "start_depth": start_depth, "process_root": work_root})("runTest")
    CampaignRun.result = None
    if not hasattr(CampaignRun, "bin"):
        CampaignRun.setUpClass()  # unittest only does this for collected suites
    outcome = unittest.TestResult()
    case.run(outcome)
    if not outcome.wasSuccessful():
        problems = [text for _, text in outcome.errors + outcome.failures]
        return {"summary": SUMMARY, "seed": seed, "outcome": "harness_error",
                "error": problems[-1][:3000] if problems else "unknown"}
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
        "items_per_floor": pooled(completed, "items_per_floor"),
        "food_per_floor": pooled(completed, "food_per_floor"),
    }


def pooled(runs, field):
    """Floor-weighted mean of one item-generation rate across runs."""
    rows = [run["item_generation"] for run in runs if run.get("item_generation")]
    floors = sum(row["floors"] for row in rows)
    if not floors:
        return None
    return sum(row[field] * row["floors"] for row in rows if row[field] is not None) / floors


def parse_seeds(text):
    first, _, last = text.partition("-")
    return list(range(int(first), int(last or first) + 1))


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--seeds", default="0-3", type=parse_seeds)
    parser.add_argument("--floor-limit", default=2500, type=int)
    parser.add_argument("--run-limit", default=65000, type=int)
    parser.add_argument("--start-depth", default=1, type=int,
                        help="Start on this floor with the model's expected level")
    # A run's save journals every entry and the headless client prints several
    # megabytes of frames per second; on a slow disk, write stalls block the
    # harness reader past its reply deadline.
    parser.add_argument("--work-root", type=Path,
                        help="Directory for each run's save and process logs, such as a fast local volume")
    args = parser.parse_args(argv)
    args.output.mkdir(parents=True, exist_ok=True)
    runs = []
    for seed in args.seeds:
        run = run_seed(seed, args.floor_limit, args.run_limit, args.start_depth, args.work_root)
        run["start_depth"] = args.start_depth
        (args.output / f"run-{seed}.json").write_text(json.dumps(run, indent=2), encoding="utf-8")
        print(seed, run["outcome"], run.get("deepest_depth"), run.get("final_level"), flush=True)
        runs.append(run)
    report = aggregate(runs)
    (args.output / "campaign.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
