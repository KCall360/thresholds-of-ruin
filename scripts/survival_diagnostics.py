"""Strict reader for private survival diagnostics, never an action-selection input.

Only completed loss-free segments count. This is not crash-safe exactly-once
storage: publication and diagnostic I/O are separate from durable save admission.
A complete-looking prefix cannot reveal a writer failure after its last barrier.
Bind measurements to an independently observed final durable anchor to detect
that case; never derive that expected anchor from this same diagnostic file.
"""
import json
from pathlib import Path

SCALARS = ("active_ticks", "periodic_health", "nutrition_spent", "nutrition_time_demand",
           "nutrition_regeneration_demand", "nutrition_clipped", "direct_health",
           "nourishment_gain", "nourishment_debt_cleared", "nourishment_offered",
           "nourishment_wasted", "items_consumed", "food_consumed",
           "projectile_numeric_budget_refusals")
POOLS = ("periodic_resources", "paid_start", "paid_resolution", "holds_acquired",
         "holds_released", "direct_resources")
RESOURCES = ("stamina", "focus", "mana")
ANCHOR_KEYS = ("lineage", "branch", "history_sequence", "entry_id", "tick")
COMMON_KEYS = (*ANCHOR_KEYS, "survival_version", "segment", "event", "selected_actor",
               "diagnostic_dropped", "diagnostic_incomplete")
EVENT_KEYS = {"survival.begin": (), "survival.barrier": ("durable_sequence",),
              "survival.commit": ("actor", "start_tick", "end_tick", "branch_changed", "flows")}
FLOW_KEYS = (*SCALARS, *POOLS, "consumed_items", "nutrition_before", "nutrition_after")
# Match the private simulation collector's per-batch bounds, not aggregate totals.
MAX_ACTORS = 1024
MAX_ITEMS_PER_ACTOR = 128


def _require(condition, message):
    if not condition:
        raise ValueError(message)


def _integer(value, minimum, maximum, message):
    _require(type(value) is int and minimum <= value <= maximum, message)
    return value


def _counter(value):
    return _integer(value, 0, 2**64 - 1, "invalid bounded counter")


def _identity(value):
    _require(isinstance(value, str) and value, "missing or invalid history identity")


def _actor(value):
    return _integer(value, 1, 2**64 - 1, "invalid actor identity")


def _key_identity(value):
    _require(isinstance(value, str) and value.isascii() and value.isdecimal()
             and not value.startswith("0") and len(value) <= 20,
             "invalid canonical actor or item identity")
    _actor(int(value))


def _shape(value, keys, message):
    _require(isinstance(value, dict) and set(value) == set(keys), message)


def _anchor(value):
    _shape(value, ANCHOR_KEYS, "invalid history anchor shape")
    _identity(value["lineage"])
    _identity(value["branch"])
    sequence = _counter(value["history_sequence"])
    _counter(value["tick"])
    if sequence == 0:
        _require(value["entry_id"] is None, "initial history anchor has an entry")
    else:
        _identity(value["entry_id"])
    return value


def _nutrition(value):
    if value is None:
        return
    _require(isinstance(value, dict), "invalid nutrition boundary")
    if value.get("kind") == "exempt":
        _shape(value, ("kind",), "invalid exempt nutrition boundary")
        return
    _shape(value, ("kind", "reserve", "elapsed", "digestion_remainder", "regeneration_remainder"),
           "invalid required nutrition boundary")
    _require(value["kind"] == "required", "invalid nutrition boundary kind")
    _integer(value["reserve"], -2**31, 2**31 - 1, "invalid nutrition reserve")
    _counter(value["elapsed"])
    _integer(value["digestion_remainder"], 0, 29,
             "invalid nutrition digestion remainder")
    _integer(value["regeneration_remainder"], 0, 2**32 - 1,
             "invalid nutrition regeneration remainder")
    # These are serialized primitive bounds. The rate denominator, starvation
    # floor and capacity are authored policy, absent from this private schema;
    # a shape check cannot independently recompute fractional prices or bands.


def _flow(flow):
    _shape(flow, FLOW_KEYS, "invalid or incomplete actor flow")
    values = {key: _counter(flow[key]) for key in SCALARS}
    for key in POOLS:
        _shape(flow[key], RESOURCES, "invalid resource counter map")
        for resource in RESOURCES:
            _counter(flow[key][resource])
    _require(values["nutrition_time_demand"] + values["nutrition_regeneration_demand"]
             == values["nutrition_spent"] + values["nutrition_clipped"],
             "nutrition demand accounting mismatch")
    _require(values["food_consumed"] <= values["items_consumed"], "invalid edible stock count")
    consumed = flow["consumed_items"]
    _require(isinstance(consumed, dict) and len(consumed) <= MAX_ITEMS_PER_ACTOR,
             "invalid or oversized consumed item map")
    for item, quantity in consumed.items():
        _key_identity(item)
        _require(_counter(quantity) > 0, "invalid consumed item quantity")
    _require(sum(consumed.values()) == values["items_consumed"], "consumed item quantity mismatch")
    # Offers and waste cover food resolution; other direct Nourish effects also
    # contribute to gain. Cleared negative reserve is separate from both.
    _require(values["nourishment_wasted"] <= values["nourishment_offered"]
             <= values["nourishment_gain"] + values["nourishment_wasted"],
             "food nourishment accounting mismatch")
    before, after = flow["nutrition_before"], flow["nutrition_after"]
    _nutrition(before)
    _nutrition(after)
    required = any(value is not None and value["kind"] == "required" for value in (before, after))
    nutrition_counters = ("nutrition_spent", "nutrition_time_demand", "nutrition_regeneration_demand",
                          "nutrition_clipped", "nourishment_gain", "nourishment_debt_cleared")
    _require(required or not any(values[key] for key in nutrition_counters),
             "nutrition flow has no required nutrition boundary")
    if before is not None and after is not None and before["kind"] == after["kind"] == "required":
        _require(after["reserve"] - before["reserve"] == values["nourishment_gain"]
                 + values["nourishment_debt_cleared"] - values["nutrition_spent"],
                 "nutrition reserve conservation failed")


def validate_records(rows, *, expected_final_anchor=None):
    """Validate complete segments and aggregate gross counters across a restart.

    Rewinds retain their real costs; branches remain explicit in commit records.
    A divergent crash suffix, missing begin anchor or duplicate entry is rejected,
    rather than silently treating it as another ordinary-play sample.

    Without expected_final_anchor this certifies only the represented prefix.
    Measurements must supply an independently read durable anchor with exactly
    lineage, branch, history_sequence, entry_id and tick to detect unseen tails.
    A first begin may anchor an existing save; callers claiming whole-run totals
    must also verify that it matches their independently known measurement start.
    Gross aggregate totals use exact Python integers, even across u64 boundaries.
    """
    if expected_final_anchor is not None:
        _anchor(expected_final_anchor)
    try:
        rows = iter(rows)
    except TypeError as error:
        raise ValueError("invalid survival record sequence") from error
    segment = live = None
    closed = False
    seen_segments, seen_entries = set(), set()
    commits, actors = 0, {}
    for row in rows:
        _require(isinstance(row, dict), "invalid survival diagnostic record")
        event = row.get("event")
        _require(isinstance(event, str) and event in EVENT_KEYS, "unknown survival event")
        _shape(row, (*COMMON_KEYS, *EVENT_KEYS[event]), "invalid survival diagnostic record shape")
        _require(_counter(row["survival_version"]) == 2, "unsupported survival diagnostic record")
        _require(_counter(row["diagnostic_dropped"]) == 0, "survival diagnostic records were lost")
        _require(row["diagnostic_incomplete"] is False, "survival diagnostic counters are incomplete")
        _identity(row["segment"])
        if row["selected_actor"] is not None:
            _actor(row["selected_actor"])
        current = _anchor({key: row[key] for key in ANCHOR_KEYS})
        if event == "survival.begin":
            _require(segment is None or closed, "previous segment has no final durable barrier")
            _require(row["segment"] not in seen_segments, "duplicate segment")
            if live is not None:
                _require(current == live, "restart anchor diverges from previous measured history")
            segment, live = row["segment"], current
            if live["entry_id"] is not None:
                seen_entries.add(live["entry_id"])
            seen_segments.add(segment)
            closed = False
            continue
        _require(segment is not None and row["segment"] == segment
                 and current["lineage"] == live["lineage"], "missing or mismatched begin anchor")
        if event == "survival.barrier":
            _require(current == live, "barrier anchor differs from live history")
            durable = _counter(row["durable_sequence"])
            _require(durable <= live["history_sequence"], "durable barrier is ahead of live history")
            closed = durable == live["history_sequence"]
            continue
        _require(current["history_sequence"] == live["history_sequence"] + 1, "noncontiguous live history")
        _require(current["entry_id"] not in seen_entries, "duplicate history entry")
        _require(row["branch_changed"] is (current["branch"] != live["branch"]), "invalid branch transition")
        _actor(row["actor"])
        _counter(row["start_tick"]); _counter(row["end_tick"])
        _require(row["start_tick"] == live["tick"], "commit start tick differs from live history")
        _require(row["end_tick"] == row["tick"], "commit tick mismatch")
        _require(row["branch_changed"] or row["end_tick"] >= row["start_tick"],
                 "time moved backwards without an explicit branch transition")
        flows = row["flows"]
        _shape(flows, ("incomplete", "actors"), "invalid flow batch shape")
        _require(flows["incomplete"] is False and isinstance(flows["actors"], dict), "invalid flow batch")
        _require(len(flows["actors"]) <= MAX_ACTORS, "oversized actor flow batch")
        for actor, flow in flows["actors"].items():
            _key_identity(actor)
            _flow(flow)
            total = actors.setdefault(actor, {**dict.fromkeys(SCALARS, 0),
                **{key: dict.fromkeys(RESOURCES, 0) for key in POOLS}, "consumed_items": {}})
            for key in SCALARS:
                total[key] += flow[key]
            for key in POOLS:
                for resource in RESOURCES:
                    total[key][resource] += flow[key][resource]
            for item, quantity in flow["consumed_items"].items():
                total["consumed_items"][item] = total["consumed_items"].get(item, 0) + quantity
        seen_entries.add(current["entry_id"])
        live = current
        commits += 1
        closed = False
    _require(segment is not None and closed, "missing final durable survival barrier")
    if expected_final_anchor is not None:
        _require(live == expected_final_anchor, "final survival barrier differs from expected durable anchor")
    return {"survival_version": 2, **live, "segments": len(seen_segments),
            "commits": commits, "actors": actors}


def _unique_object(pairs):
    value = {}
    for key, item in pairs:
        _require(key not in value, "duplicate survival JSON object key")
        value[key] = item
    return value


def _invalid_constant(value):
    raise ValueError("invalid survival JSON constant: " + value)


def read_log(path, *, expected_final_anchor=None):
    """Read strict JSONL; an independent final anchor is needed to exclude stale prefixes."""
    text = Path(path).read_text(encoding="utf-8")
    _require(text.endswith("\n"), "partial survival log")
    rows = (json.loads(line, object_pairs_hook=_unique_object, parse_constant=_invalid_constant)
            for line in text.splitlines())
    return validate_records(rows, expected_final_anchor=expected_final_anchor)
