"""Explicit work/output bounds for controlled, multi-client process probes.

This guard does not change gameplay, retry failed commands, or hide adverse
outcomes. Exceeding any bound is a compact test failure with evidence retained.
"""
import json
from pathlib import Path


class ProcessBudgetExceeded(AssertionError):
    """A bounded diagnostic outcome; never a gameplay win or ordinary death."""
    def __init__(self, bound, used, limit, *, last=None):
        self.bound, self.used, self.limit = bound, used, limit
        super().__init__("Process work budget exhausted: " + json.dumps({
            "bound": bound, "used": used, "limit": limit, "last": last or {}}, sort_keys=True))


class ProcessOutputBudget:
    """Track bytes added to explicit log files/directories since construction.

    Nonexistent paths are valid for a not-yet-launched process. Directory trees
    count every file, including stderr/spool outputs; overlapping paths count
    each file once. Shrinkage, replacement and disappearance fail closed during
    an active chunk. Call check before/after commands and each consumed frame.
    """
    def __init__(self, paths, *, limit_bytes):
        if type(limit_bytes) is not int or limit_bytes <= 0:
            raise ValueError("Output budget must be a positive integer")
        self.paths = tuple(Path(path) for path in paths)
        self.limit_bytes = limit_bytes
        self.observed = self._files()
        self.total_bytes = 0

    def _files(self):
        files = set()
        for path in self.paths:
            if path.is_file():
                files.add(path.resolve())
            elif path.is_dir():
                files.update(child.resolve() for child in path.rglob("*") if child.is_file())
        return {path: (path.stat().st_dev, path.stat().st_ino, path.stat().st_size) for path in files}

    def used_bytes(self):
        current = self._files()
        for path, prior in self.observed.items():
            observed = current.get(path)
            if observed is None or observed[:2] != prior[:2] or observed[2] < prior[2]:
                raise ProcessBudgetExceeded("output_continuity", 1, 0, last={
                    "path": str(path), "problem": "tracked output disappeared, was replaced or shrank"})
        self.total_bytes += sum(value[2] - self.observed.get(path, (0, 0, 0))[2]
                                for path, value in current.items())
        self.observed = current
        return self.total_bytes

    def check(self):
        used = self.used_bytes()
        if used > self.limit_bytes:
            raise ProcessBudgetExceeded("transcript_bytes", used, self.limit_bytes)
        return used


class ProcessWorkBudget:
    def __init__(self, *, requests, actor_ticks, transcript_bytes):
        for value in (requests, actor_ticks, transcript_bytes):
            if type(value) is not int or value <= 0:
                raise ValueError("Process work budgets must be positive integers")
        self.limits = {"requests": requests, "actor_ticks": actor_ticks,
                       "transcript_bytes": transcript_bytes}
        self.requests = 0
        self.actor_ticks = 0
        self.clients = {}
        self.last = {}
        self.output = ProcessOutputBudget([], limit_bytes=transcript_bytes)

    def track(self, client, frame):
        """Count all output, including initial disclosure, for every participant."""
        self.clients[id(client)] = {"path": client.transcript.path,
                                    "tick": int(frame["state"]["observation"]["tick"])}
        self.output.paths += (client.transcript.path,)
        self.after(client, frame)

    def frame_wait(self, wait, client, predicate, *, seconds=15):
        """Guard every consumed JSON frame, including post-ack settlement.

        Queries and mutations increment request count through before(), once.
        Consuming asynchronous lifecycle frames never increments that count.
        """
        self.check()

        def checked(frame):
            self.after(client, frame)
            return predicate(frame)

        result = wait(client, checked, seconds=seconds)
        self.after(client, result)
        return result

    def before(self):
        self.check()
        if self.requests == self.limits["requests"]:
            self.fail("requests")
        self.requests += 1

    def after(self, client, frame):
        view = (frame.get("state") or {}).get("observation")
        if view is not None:
            tracked = self.clients.get(id(client))
            if tracked is not None:
                tick = int(view["tick"])
                if tick < tracked["tick"]:
                    raise AssertionError("Budgeted driver unexpectedly rewound actor time")
                self.actor_ticks += tick - tracked["tick"]
                tracked["tick"] = tick
            combat = view.get("combat") or {}
            self.last = {"tick": view["tick"], "hp": combat.get("hp"),
                         "max_hp": combat.get("max_hp"), "dead": combat.get("dead"),
                         "phases": [row["phase"] for row in frame.get("intentions", [])][:4],
                         "error": frame.get("error")}
        self.check()

    def usage(self):
        return {"requests": self.requests, "actor_ticks": self.actor_ticks,
                "transcript_bytes": self.output.used_bytes()}

    def check(self):
        for kind, used in self.usage().items():
            if used > self.limits[kind]:
                self.fail(kind)

    def report(self):
        """Best-effort final counters without masking the original failure."""
        error = None
        try:
            used = self.usage()
        except (AssertionError, OSError) as failure:
            used = {"requests": self.requests, "actor_ticks": self.actor_ticks,
                    "transcript_bytes": self.output.total_bytes}
            error = str(failure)[:1000]
        return {"limits": dict(self.limits), "used": used, "last": dict(self.last),
                "measurement_error": error}

    def fail(self, kind):
        raise ProcessBudgetExceeded(kind, self.usage()[kind], self.limits[kind], last=self.last)
