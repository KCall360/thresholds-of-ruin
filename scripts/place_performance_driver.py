"""Real headless/native place workload. Retain raw diagnostics, saves and failures."""
import argparse
import json
import os
from pathlib import Path
import uuid
from performance_driver import JsonProcess, stop_all
from client_performance_report import validate_presentation_profile
from place_performance_report import SPEC, validate_client


def run(binary_dir, output, rooms, fresh_player=False):
    output.mkdir(parents=True, exist_ok=False)
    processes = []
    env = os.environ.copy()
    env.update(TOR_SERVER_TOKEN=uuid.uuid4().hex, TOR_WIZARD_TOKEN=uuid.uuid4().hex,
               TOR_SPECTATOR_TOKEN=uuid.uuid4().hex, TOR_DRIVER_DEFER_LOGS="1")
    suffix = ".exe" if os.name == "nt" else ""
    result = dict(version=1, extra_rooms=rooms, samples=[], diagnostic_mode="deferred", fresh_player=fresh_player)

    def launch(binary, args, name, token=None):
        child_env = env.copy()
        if token:
            child_env["TOR_SERVER_TOKEN"] = token
        process = JsonProcess(binary_dir / (binary + suffix), args, child_env, output, name)
        processes.append(process)
        return process

    try:
        server = launch("tor-server", ["--listen", "127.0.0.1:0", "--seed", SPEC["seed"], "--wizard", "--save", output / "game.db"], "server")
        ready, _ = server.until(lambda f: "address" in f)
        player = launch("tor-client-headless", ["--connect", ready["address"]], "player", env["TOR_WIZARD_TOKEN"])
        state, _ = player.until(lambda f: f.get("type") == "ready")

        def command(value):
            nonlocal state
            value["expected_revision"] = state["state"]["revision"]
            state, start, ack, received = player.send({"type": "request", "request": {
                "type": "command", "branch": state["branch"], "command": value}})
            assert state["error"] is None, state["error"]
            return start, ack, received

        def wizard(operation):
            command(dict(type="wizard", operation=operation))

        for i in range(rooms):
            region = i + 3
            wizard(f"room {region} 5 5 1 Private")
            for x, y, z in SPEC["hints"]:
                wizard(f"place {region} {x} {y} {z} on")
            wizard(f"teleport 1 {region} 0 0 0")
        wizard("teleport 1 1 1 1 0")
        if fresh_player:
            # A fresh attachment retains durable names but drops headless diagnostic
            # map memory. Keep this a distinct case, never replace the original run.
            player.stop()
            player = launch("tor-client-headless", ["--connect", ready["address"]], "fresh-player", env["TOR_WIZARD_TOKEN"])
            state, _ = player.until(lambda f: f.get("type") == "ready")
        places = state["state"]["observation"]["places"]
        assert len(places) == 2 + rooms * len(SPEC["hints"])
        native = launch("tor-client-ascii", ["--connect", ready["address"], "--automation"], "native", env["TOR_SPECTATOR_TOKEN"])
        native.until(lambda f: f.get("state") == state["state"] and not f["busy"])
        native.child.stdin.write(json.dumps(dict(type="key", key="places")) + "\n")
        native.child.stdin.flush()
        native.until(lambda f: f.get("places_open") is True)
        for i in range(SPEC["samples"]):
            tick = state["state"]["observation"]["tick"]
            if i % 2 == 0:
                start, ack, received = command(dict(type="rename_place", key=places[-1]["key"], name=f"Reverie {i}"))
                assert state["state"]["observation"]["tick"] == tick
            else:
                start, ack, received = command(dict(type="act", action=dict(type="wait")))
                assert state["state"]["observation"]["tick"] == tick + 100
            assert ack is not None
            frame, shown = native.until(lambda f: f.get("state") == state["state"])
            assert frame["places_open"] and frame["role"] == "spectator" and not frame["has_control"]
            validate_presentation_profile(frame["profile"])
            sample = dict(sample=i, label="rename" if i % 2 == 0 else "wait", places=len(places),
                          request_to_ack_ms=(ack-start)*1000, request_to_ready_ms=(received-start)*1000,
                          request_to_presentation_ms=(shown-start)*1000, profile=frame["profile"],
                          request_to_ack_line_ms=(player.ack_line_received-start)*1000,
                          ready_reader_work_ms=player.last_reader_work_ms,
                          ready_queue_delay_ms=player.last_queue_delay_ms,
                          headless_memory_cells=len(state["memory"]),
                          headless_ready_bytes=len(json.dumps(state).encode("utf-8")))
            result["samples"].append(sample)
        saved, *_ = player.send({"type": "request", "request": {"type": "save"}})
        assert saved["error"] is None
        result["final_places"] = saved["state"]["observation"]["places"]
        validate_client(result)
        with (output / "samples.jsonl").open("w", encoding="utf-8") as stream:
            for sample in result["samples"]:
                stream.write(json.dumps(sample) + "\n")
        (output / "result.json").write_text(json.dumps(result), encoding="utf-8")
    except Exception as error:
        (output / "failure.json").write_text(json.dumps(dict(error=str(error), partial_result=result)), encoding="utf-8")
        raise
    finally:
        stop_all(processes, output, result)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin-dir", type=Path, default=Path("target/release"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--fresh-player", action="store_true")
    args = parser.parse_args()
    for rooms in SPEC["extra_rooms"]:
        run(args.bin_dir.resolve(), args.output.resolve() / f"rooms-{rooms}", rooms, args.fresh_player)
