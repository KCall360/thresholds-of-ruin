"""Run the versioned performance trace through real, disclosed-state clients.

Pacing, snapshots, and optional progress annotations are outside action timing.
All child processes and fresh saves belong to this invocation.
"""
import argparse
import json
import os
from pathlib import Path
import queue
import subprocess
import threading
import time
import uuid
from client_performance_report import validate_presentation_profile

ROOT = Path(__file__).resolve().parents[1]
SPEC = ROOT / "crates/server/fixtures/performance-v1.json"


class WindowClosed(RuntimeError):
    pass


class JsonProcess:
    def __init__(self, binary, args, env, directory, name, visible=False):
        self.name = name
        self.ack_line_received = None
        self.last_line_received = None
        self.lines = queue.Queue()
        self.stderr = (directory / (name + ".stderr.log")).open("w", encoding="utf-8")
        self.log = (directory / (name + ".stdout.jsonl")).open("w", encoding="utf-8")
        self.child = subprocess.Popen([str(binary), *map(str, args)], cwd=ROOT, env=env,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.stderr,
            text=True, encoding="utf-8", bufsize=1,
            creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" and not visible else 0)
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.reader.start()

    def _read(self):
        try:
            for line in self.child.stdout:
                received = time.perf_counter()
                self.log.write(line)
                self.log.flush()
                self.lines.put((json.loads(line), received))
        except Exception as error:
            self.lines.put(({"type": "fatal", "error": str(error)}, time.perf_counter()))
        finally:
            self.lines.put((None, time.perf_counter()))

    def until(self, predicate, seconds=30):
        deadline = time.monotonic() + seconds
        while True:
            try:
                value, received = self.lines.get(timeout=max(0, deadline - time.monotonic()))
            except queue.Empty:
                raise RuntimeError("Client readiness/presentation deadline; see retained logs") from None
            if value is None:
                if self.name == "ascii" and self.child.wait(timeout=2) == 0:
                    raise WindowClosed("Spectator window closed")
                raise RuntimeError("Owned process exited; see retained logs")
            if value.get("type") == "fatal":
                raise RuntimeError(value["error"])
            self.last_line_received = received
            if predicate(value):
                return value, received

    def send(self, value):
        encoded = json.dumps(value) + "\n"
        started = time.perf_counter()
        self.child.stdin.write(encoded)
        self.child.stdin.flush()
        acknowledgement = None
        self.ack_line_received = None
        def ready(value):
            nonlocal acknowledgement
            if (value.get("message") or {}).get("type") == "ack":
                acknowledgement = time.perf_counter()
                self.ack_line_received = self.last_line_received
            return value.get("type") == "ready"
        frame, received = self.until(ready)
        return frame, started, acknowledgement, received

    def snapshot(self):
        frame, *_ = self.send({"type":"request", "request":{"type":"snapshot"}})
        if frame["error"]:
            raise RuntimeError(frame["error"])
        return frame

    def stop(self):
        if self.child.poll() is None:
            self.child.kill()
        self.child.wait(timeout=10)
        self.reader.join(timeout=10)
        self.child.stdin.close()
        self.child.stdout.close()
        self.stderr.close()
        self.log.close()


def resolve(step, frame):
    assert frame["state"]["observation"]["ready"], "Actor must be scheduled"
    action = step["action"]
    if action["type"] != "door":
        return action.copy()
    doors = {c["door"]["id"] for c in frame["state"]["observation"]["visible_cells"]
        if c.get("door") and c["door"]["reachable"] and c["door"]["open"] != action["open"]}
    assert len(doors) == 1, "Expected one disclosed reachable door"
    return {"type":"set_door", "door":doors.pop(), "open":action["open"]}


def verify(step, action, before, after):
    if step["expected"] == "blocked":
        assert after["error"] and "InvalidAction" in after["error"], after.get("error")
        assert before["state"] == after["state"], "Blocked action advanced state"
        return
    assert after["error"] is None, (step["label"], after["error"])
    assert before["branch"] == after["branch"]
    old, new = before["state"]["observation"], after["state"]["observation"]
    center = lambda s: next(c["key"] for c in s["visible_cells"] if c["position"] == {"x":0,"y":0,"z":0})
    if action["type"] == "move":
        assert center(old) != center(new), "Successful move must displace actor"
        delta = {"north":(0,-1,0),"east":(1,0,0),"south":(0,1,0),"west":(-1,0,0),
            "north_east":(1,-1,0),"south_east":(1,1,0),"south_west":(-1,1,0),"north_west":(-1,-1,0),
            "up":(0,0,1),"down":(0,0,-1)}[action["direction"]]
        target = next(c for c in old["visible_cells"] if c["position"] == dict(zip(("x","y","z"),delta)))
        assert target["key"] == center(new), "Move did not reach disclosed destination"
    elif action["type"] == "wait":
        assert old["visible_cells"] == new["visible_cells"]
    else:
        assert any(c.get("door") and c["door"]["id"] == action["door"] and c["door"]["open"] == action["open"]
            for c in new["visible_cells"])
        assert old["visible_cells"] != new["visible_cells"]
    assert after["history"][-1]["content"]["event"]["type"] == step["expected"]


def run_demo(bin_dir, output, *, regions=256, actors=1, cycles=3, pace=0.25, stay_open=False, capture=True):
    bin_dir, output = Path(bin_dir).resolve(), Path(output).resolve()
    output.mkdir(parents=True, exist_ok=False)
    spec = json.loads(SPEC.read_text(encoding="utf-8"))
    assert 1 <= regions <= 256 and 1 <= actors <= 8 and cycles > 0 and pace >= 0
    suffix = ".exe" if os.name == "nt" else ""
    env = {k:v for k,v in os.environ.items() if k not in ("TOR_SERVER_TOKEN","TOR_SPECTATOR_TOKEN","TOR_WIZARD_TOKEN")}
    player, spectator_token = uuid.uuid4().hex, uuid.uuid4().hex
    processes = []
    def launch(name, args, token, label, visible=False, extra=None):
        process = JsonProcess(bin_dir/(name+suffix), args, {**env,"TOR_SERVER_TOKEN":token,**(extra or {})}, output, label, visible)
        processes.append(process)
        return process
    result = {"trace_version":spec["version"],"seed":spec["seed"],"regions":regions,"actors":actors,
        "cycles":0,"pace_seconds":pace,"capture":capture,"diagnostics_version":1,"samples":[]}
    clients = {}
    try:
        server = launch("tor-server", ["--listen","127.0.0.1:0","--seed",spec["seed"],"--regions",regions,
            "--actors",actors,"--save",output/"game.json"], player, "server", extra={"TOR_SPECTATOR_TOKEN":spectator_token})
        ready, _ = server.until(lambda f:"address" in f)
        address = ready["address"]
        spectator = launch("tor-client-ascii", ["--connect",address,"--report-frames",*(["--capture",output/"last-frame.ppm"] if capture else [])],
            spectator_token,"ascii",visible=True)
        initial, _ = spectator.until(lambda f:f.get("state") is not None and not f.get("busy"))
        assert initial["role"] == "spectator" and not initial["has_control"]
        result.update(spectator_role=initial["role"],initial_revision=initial["state"]["revision"])
        # Presented spectator frame is confirmed before creating the player driver.
        for actor in range(1,actors+1):
            client = launch("tor-client-headless",["--connect",address,"--actor",actor],player,f"actor-{actor}")
            state, _ = client.until(lambda f:f.get("type") == "ready")
            assert state["error"] is None and state["has_control"]
            clients[actor] = client
        result["client_memory_start"] = len(clients[1].snapshot()["memory"])
        (output/"processes.json").write_text(json.dumps({"server":server.child.pid,"ascii":spectator.child.pid,
            "drivers":[c.child.pid for c in clients.values()]}),encoding="utf-8")
        secondary = 0
        def perform(actor, step, cycle, index):
            if spectator.child.poll() is not None:
                if spectator.child.returncode == 0:
                    raise WindowClosed("Spectator window closed")
                raise RuntimeError("Spectator failed; see retained logs")
            before = clients[actor].snapshot()
            action = resolve(step,before)
            if pace and actor == 1:
                # Disclosed progress text appears in ASCII's history panel. These
                # demo-only annotations are outside the measured action boundary.
                progress = {"type":"command","branch":before["branch"],"command":{"type":"annotate",
                    "anchor":{"type":"state","revision":before["state"]["revision"]},
                    "text":f"Phase A | cycle {cycle+1}/{cycles} | step {index+1}: {step['label']}",
                    "source":"frontend","audience":"actor","category":"note"}}
                annotated, *_ = clients[actor].send({"type":"request","request":progress})
                assert annotated["error"] is None
                before = annotated
            after, start, ack, received = clients[actor].send({"type":"act","action":action})
            verify(step,action,before,after)
            sample = {"cycle":cycle,"step":index,"actor":actor,"label":step["label"],"expected":step["expected"],"action":action,
                "request_to_ack_ms":None if ack is None else (ack-start)*1000,
                "request_to_ack_line_ms":None if clients[actor].ack_line_received is None else (clients[actor].ack_line_received-start)*1000,
                "request_to_ready_ms":(received-start)*1000,"request_to_presentation_ms":None}
            if step["expected"] != "blocked":
                assert ack is not None, "Accepted action missing acknowledgement"
                if actor == 1:
                    frame, shown = spectator.until(lambda f:f.get("state") == after["state"])
                    assert frame["role"] == "spectator" and not frame["has_control"]
                    sample["request_to_presentation_ms"] = (shown-start)*1000
                    if "profile" in frame:
                        validate_presentation_profile(frame["profile"])
                        sample["presentation_profile"] = frame["profile"]
                    result["presented_revision"] = frame["state"]["revision"]
            result["samples"].append(sample)
            with (output/"samples.jsonl").open("a",encoding="utf-8") as stream:
                stream.write(json.dumps(sample)+"\n")
            if pace:
                time.sleep(pace)
        def scheduled(step, cycle, index):
            nonlocal secondary
            while not clients[1].snapshot()["state"]["observation"]["ready"]:
                for actor in range(2,actors+1):
                    if clients[actor].snapshot()["state"]["observation"]["ready"]:
                        if actor == 2:
                            other = spec["secondary"][secondary % len(spec["secondary"])]
                            secondary += 1
                        else:
                            other = {"label":"wait_same_region","expected":"waited","action":{"type":"wait"}}
                        perform(actor,other,cycle,index)
                        break
                else:
                    raise AssertionError("No disclosed ready actor")
            perform(1,step,cycle,index)
        region = 1
        for cycle in range(cycles):
            for index, step in enumerate(spec["steps"]):
                if step.get("min_regions",1) <= (regions if region < regions else 1):
                    scheduled(step,cycle,index)
            result["cycles"] += 1
            if region < regions and cycle+1 < cycles:
                for index, step in enumerate(spec["traversal"]):
                    scheduled(step,cycle,index)
                region += 1
        result["client_memory_end"] = len(clients[1].snapshot()["memory"])
        # Durability barrier is outside action timing; ordinary acks are in-memory.
        saved, *_ = clients[1].send({"type":"request","request":{"type":"save"}})
        assert saved["error"] is None, saved["error"]
        (output/"result.json").write_text(json.dumps(result,indent=2),encoding="utf-8")
        if stay_open:
            spectator.child.wait()
        return result
    except WindowClosed:
        if 1 in clients:
            saved, *_ = clients[1].send({"type":"request","request":{"type":"save"}})
            if saved["error"]:
                raise RuntimeError(saved["error"])
        result["cancelled"] = True
        (output/"result.json").write_text(json.dumps(result,indent=2),encoding="utf-8")
        return result
    except BaseException as error:
        (output/"failure.json").write_text(json.dumps({"error":str(error),"completed_cycles":result["cycles"]}),encoding="utf-8")
        raise
    finally:
        for process in reversed(processes):
            process.stop()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin-dir",type=Path,default=ROOT/"target/debug")
    parser.add_argument("--output",type=Path)
    parser.add_argument("--regions",type=int,default=256)
    parser.add_argument("--actors",type=int,default=1)
    parser.add_argument("--cycles",type=int,default=3)
    parser.add_argument("--pace-ms",type=float,default=250)
    parser.add_argument("--stay-open",action="store_true")
    parser.add_argument("--no-capture",action="store_true")
    args = parser.parse_args()
    output = args.output or ROOT/"saves/playtests"/("regions-256-"+uuid.uuid4().hex)
    print(f"Performance demo logs and fresh save: {output}",flush=True)
    result = run_demo(args.bin_dir,output,regions=args.regions,actors=args.actors,cycles=args.cycles,
        pace=args.pace_ms/1000,stay_open=args.stay_open,capture=not args.no_capture)
    print(f"Verified {result['cycles']} complete cycles.")


if __name__ == "__main__":
    main()
