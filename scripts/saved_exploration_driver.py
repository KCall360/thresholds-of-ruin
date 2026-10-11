"""Complete saved discovery through the native ASCII application, then restart.

UI automation enters ordinary keys. Timing ends at reader receipt of a presented
frame and includes transport, native presentation and optional diagnostic I/O.
"""
import argparse
import json
import os
from pathlib import Path
from workload_report import require, is_duration, nonnegative_counts, positive_counts
import sqlite3
import time
import uuid
from performance_driver import JsonProcess, SPEC, ROOT, resolve, verify, wall_time_ns, stop_all
from client_performance_report import validate_presentation_profile
from process_harness import ascii_input_completion

KEYS = {"north":"up", "east":"right", "south":"down", "west":"left",
        "up":"ascend", "down":"descend"}


def validate(result):
    require(isinstance(result, dict) and not result.get("diagnostics_error"), "Invalid saved exploration report")
    positive_counts(result, ("regions", "trace_version", "checkpoint_interval", "actions", "disclosed_cells",
                             "checkpoint_sequence", "checkpoint_bytes", "journal_records"))
    nonnegative_counts(result, ("tail_records",))
    require(result["regions"] in (8, 256), "Unsupported exploration size")
    spec = json.loads(SPEC.read_text())
    expected = spec["traversal"] * (result["regions"] - 1)
    require(result["trace_version"] == spec["version"] and result["seed"] == spec["seed"], "Wrong traversal version or seed")
    require(isinstance(result.get("samples"), list) and len(result["samples"]) == len(expected) == result["actions"],
            "Incomplete exploration actions")
    for index, (sample, step) in enumerate(zip(result["samples"], expected)):
        nonnegative_counts(sample, ("index",))
        require(sample["index"] == index and sample["label"] == step["label"] and sample["action"] == step["action"],
                "Wrong exploration action order")
        require(is_duration(sample.get("request_to_presentation_ms")), "Invalid request-to-presentation duration")
        validate_presentation_profile(sample["profile"])
    require(result["disclosed_cells"] == {8:1260, 256:41436}[result["regions"]], "Incomplete disclosed-cell coverage")
    require(result["checkpoint_bytes"] < 16 * 1024 * 1024, "Checkpoint must be under 16 MiB")
    require(result["journal_records"] == result["actions"] * 2, "Wrong journal action accounting")
    require(result["checkpoint_sequence"] <= result["journal_records"], "Checkpoint exceeds committed history")
    require(result["tail_records"] == result["journal_records"] - result["checkpoint_sequence"], "Wrong checkpoint tail")
    require(result["tail_records"] < result["checkpoint_interval"], "Unbounded checkpoint tail")
    require(result.get("restart_equal") is True and result.get("continued_after_restart") is True,
            "Restart must preserve state and accept continued input")
    for field in ("restart_to_frame_ms", "restart_input_ms"):
        require(is_duration(result.get(field)), f"Invalid {field} duration")


def run_saved_exploration(bin_dir, output, regions=8, interval=64, correlate=False, defer_logs=False):
    assert regions in (8, 256) and interval > 0
    bin_dir, output = Path(bin_dir).resolve(), Path(output).resolve()
    output.mkdir(parents=True, exist_ok=False)
    spec = json.loads(SPEC.read_text())
    env = {k:v for k,v in os.environ.items() if k not in ("TOR_SERVER_TOKEN", "TOR_SPECTATOR_TOKEN", "TOR_WIZARD_TOKEN")}
    env["TOR_SERVER_TOKEN"] = uuid.uuid4().hex
    env.pop("TOR_DRIVER_DEFER_LOGS", None)
    if defer_logs:
        env["TOR_DRIVER_DEFER_LOGS"] = "1"
    if correlate:
        env["TOR_TIMING_DIAGNOSTICS"] = "1"
    suffix = ".exe" if os.name == "nt" else ""
    owned = []
    save = output / "game.db"
    def launch(binary, args, name, visible=False):
        process = JsonProcess(bin_dir / (binary + suffix), args, env, output, name, visible)
        owned.append(process)
        return process
    def server(name):
        process = launch("tor-server", ["--listen", "127.0.0.1:0", "--seed", spec["seed"],
            "--regions", regions, "--save", save, "--checkpoint-interval", interval,
            "--save-target-ms", 10, "--save-max-ms", 50, "--save-idle-ms", 1], name)
        ready, _ = process.until(lambda f:"address" in f)
        return process, ready["address"]
    def key(window, name):
        started = time.perf_counter()
        window.input_unix_ns = wall_time_ns(started)
        window.child.stdin.write(json.dumps({"type":"key", "key":name}) + "\n")
        window.child.stdin.flush()
        frame, received = window.until(ascii_input_completion(name))
        assert frame["connected"] and frame["window_open"]
        return frame, (received-started)*1000
    result = {"trace_version":spec["version"], "seed":spec["seed"], "regions":regions,
              "checkpoint_interval":interval, "correlate":correlate, "defer_logs":defer_logs, "samples":[]}
    try:
        host, address = server("server")
        window = launch("tor-client-ascii", ["--connect", address, "--automation"], "ascii", True)
        before, _ = window.until(lambda f:f.get("state") is not None and f["has_control"] and not f["busy"])
        disclosed = {c["key"] for c in before["state"]["observation"]["visible_cells"]}
        for index, step in enumerate(spec["traversal"] * (regions-1)):
            action = resolve(step, before)
            if action["type"] == "set_door":
                cell = next(c for c in before["state"]["observation"]["visible_cells"]
                            if c.get("door") and c["door"]["id"] == action["door"])
                position = cell["position"]
                direction = {(1,0):"right", (-1,0):"left", (0,1):"down", (0,-1):"up"}[(position["x"],position["y"])]
                key(window, "open_door" if action["open"] else "close_door")
            else:
                direction = KEYS[action["direction"]]
            after, elapsed = key(window, direction)
            verify(step, action, before, {**after, "error":None})
            disclosed.update(c["key"] for c in after["state"]["observation"]["visible_cells"])
            sample = {"index":index, "label":step["label"], "action":step["action"],
                      "presented_frame":after["frame"],
                      "request_to_presentation_ms":elapsed, "profile":after["profile"]}
            if correlate:
                sample.update(input_unix_ns=window.input_unix_ns, line_unix_ns=window.last_line_unix_ns, reader_work_ms=window.last_reader_work_ms,
                              queue_delay_ms=window.last_queue_delay_ms, intermediate_profiles=window.last_frame_profiles)
            result["samples"].append(sample)
            before = after
        result.update(disclosed_cells=len(disclosed), actions=len(result["samples"]))
        # Save/restart uses the ordinary barrier after complete exploration.
        observer = launch("tor-client-headless", ["--connect",address,"--observe"], "observer")
        observer.until(lambda f:f.get("type") == "ready")
        flushed, *_ = observer.send({"type":"request", "request":{"type":"save"}})
        assert flushed["error"] is None
        with sqlite3.connect(save) as db:
            sequence, size = db.execute("SELECT sequence,length(payload) FROM checkpoint").fetchone()
            tail = db.execute("SELECT count(*) FROM journal WHERE sequence>0").fetchone()[0]
        result.update(checkpoint_sequence=sequence, checkpoint_bytes=size, tail_records=tail,
                      journal_records=sequence + tail)
        # Durably restart the exact completed exploration before any extra input.
        observer.stop(); owned.remove(observer)
        window.stop(); owned.remove(window)
        host.stop(); owned.remove(host)
        started = time.perf_counter()
        host, address = server("restarted-server")
        window = launch("tor-client-ascii", ["--connect",address,"--automation"], "restarted-ascii", True)
        resumed, _ = window.until(lambda f:f.get("state") is not None and f["has_control"] and not f["busy"])
        result["restart_to_frame_ms"] = (time.perf_counter()-started)*1000
        result["restart_equal"] = resumed["state"] == before["state"] and resumed["branch"] == before["branch"] and resumed["history"] == before["history"]
        continued, elapsed = key(window, "wait")
        result["continued_after_restart"] = int(continued["state"]["revision"]) == int(before["state"]["revision"]) + 1
        result["restart_input_ms"] = elapsed
        key(window, "release")
        # Text player follows the same recovered state and explicit-save barrier.
        from process_harness import Process
        text = Process(bin_dir/("tor-client-text"+suffix), ["--script","--connect",address],
                       token=env["TOR_SERVER_TOKEN"], diagnostics=output/"text-final-process")
        try:
            text.until(lambda line:line == "Ready.")
            waited = text.command("wait")
            assert "Action: Resolved." in waited and "Waited" in waited
            assert "Done." in text.command("save")
        finally:
            text.stop()
        validate(result)
        (output/"result.json").write_text(json.dumps(result, indent=2))
        return result
    except BaseException as error:
        (output/"failure.json").write_text(json.dumps({"error":str(error), "partial_result":result}))
        raise
    finally:
        stop_all(owned, output, result)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin-dir", type=Path, default=ROOT/"target/release")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--regions", type=int, choices=(8,256), default=256)
    parser.add_argument("--checkpoint-interval", type=int, default=64)
    parser.add_argument("--correlate", action="store_true")
    parser.add_argument("--defer-logs", action="store_true", help="Retain capped diagnostic logs until each child stops")
    args = parser.parse_args()
    result = run_saved_exploration(args.bin_dir,args.output,args.regions,args.checkpoint_interval,args.correlate,args.defer_logs)
    print(f"Verified {result['actions']} actions, {result['disclosed_cells']} disclosed cells, restart and continued input.")
