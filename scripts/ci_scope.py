"""Classify complete Git changes for the narrow prose-only CI exception.

Any missing or ambiguous evidence selects full CI. No supplied path list, API
pagination, commit message or filename is allowed to control a shell command.
"""
import json
import os
from pathlib import Path
import re
import subprocess

# This Markdown is exercised by the real Rust wire decoders, not just doc checks.
RUST_DOCUMENTATION_INPUTS = {"docs/reference/protocol.md": {"tor-protocol"}}
ROOT_DOCUMENTS = {"README.md", "AGENTS.md", "CONTRIBUTING.md"}
SHA = re.compile(r"[0-9a-f]{40}\Z")
MAX_DIFF_BYTES = 4 * 1024 * 1024


def documentation_path(path):
    if not isinstance(path, str) or not re.fullmatch(r"[A-Za-z0-9_./-]+", path): return False
    parts = path.split("/")
    # Linux-only docs checks must not conceal invalid/aliased Windows paths.
    if any(not part or part.startswith(".") or part != part.rstrip(" .")
           or re.fullmatch(r"(?i)(?:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\..*)?", part) for part in parts):
        return False
    if path.lower() in RUST_DOCUMENTATION_INPUTS: return False
    if path in ROOT_DOCUMENTS: return True
    return (path.startswith("docs/") and path.endswith(".md")
            and all(part == part.lower() and part != "readme.md" for part in parts[:-1])
            and (parts[-1] == "README.md" or parts[-1] == parts[-1].lower() and parts[-1] != "readme.md"))


def require(condition):
    if not condition: raise ValueError("missing or ambiguous change evidence")


def git(root, *arguments):
    result = subprocess.run(["git", "--no-pager", *arguments], cwd=root, stdout=subprocess.PIPE,
                            stderr=subprocess.DEVNULL, timeout=20, check=True,
                            env={**os.environ, "GIT_OPTIONAL_LOCKS": "0", "GIT_NO_LAZY_FETCH": "1"})
    require(len(result.stdout) <= MAX_DIFF_BYTES)
    return result.stdout


def changed_paths(raw):
    """--no-renames exposes both old and new paths, including code renamed to prose."""
    require(len(raw) <= MAX_DIFF_BYTES and (not raw or raw.endswith(b"\0")))
    fields = raw.split(b"\0")[:-1]
    require(len(fields) % 2 == 0 and len(fields) <= 20000)
    paths = []
    for metadata, name in zip(fields[::2], fields[1::2]):
        parts = metadata.decode("ascii").split()
        require(len(parts) == 5 and parts[0].startswith(":"))
        before, after, old_sha, new_sha, status = parts
        require(SHA.fullmatch(old_sha) and SHA.fullmatch(new_sha))
        require((status, before, after) in (("M", ":100644", "100644"),
                                           ("A", ":000000", "100644"), ("D", ":100644", "000000")))
        path = name.decode("utf-8")
        require(documentation_path(path) and path not in paths)
        paths.append(path)
    return paths


def classify(root, event_name, event, checkout_sha):
    report = {"docs_only": False, "reason": "full CI required"}
    try:
        require(event_name in ("pull_request", "push"))
        require(isinstance(checkout_sha, str) and SHA.fullmatch(checkout_sha))
        require(git(root, "rev-parse", "--is-shallow-repository").strip() == b"false")
        require(git(root, "rev-parse", "HEAD").decode().strip() == checkout_sha)
        if event_name == "pull_request":
            base, head = (event["pull_request"][key]["sha"] for key in ("base", "head"))
            require(all(isinstance(sha, str) and SHA.fullmatch(sha) for sha in (base, head)))
            parents = git(root, "rev-list", "--parents", "-n", "1", checkout_sha).decode().split()
            require(parents == [checkout_sha, base, head] and base != head)
            merge_base = git(root, "merge-base", "--all", base, head).decode().split()
            require(len(merge_base) == 1 and SHA.fullmatch(merge_base[0]))
            ranges = [(merge_base[0], head), (base, checkout_sha)]
        else:
            base, head = event["before"], event["after"]
            require(event.get("created") is False and event.get("deleted") is False
                    and event.get("forced") is False and head == checkout_sha
                    and isinstance(base, str) and SHA.fullmatch(base) and base != "0" * 40 and base != head)
            git(root, "merge-base", "--is-ancestor", base, head)
            ranges = [(base, head)]
        counts = [len(changed_paths(git(root, "diff", "--raw", "--no-abbrev", "-z", "--no-renames",
                                        "--no-ext-diff", "--no-textconv", "--ignore-submodules=none", before, after, "--")))
                  for before, after in ranges]
        require(all(counts))
        report.update(docs_only=True, reason="complete prose-only diff", changed_paths=counts[-1])
    except (OSError, ValueError, TypeError, KeyError, subprocess.SubprocessError):
        pass
    return report


def main():
    try:
        with Path(os.environ["GITHUB_EVENT_PATH"]).open("rb") as handle:
            raw = handle.read(MAX_DIFF_BYTES + 1)
        require(len(raw) <= MAX_DIFF_BYTES)
        report = classify(Path.cwd(), os.environ.get("GITHUB_EVENT_NAME"), json.loads(raw), os.environ.get("GITHUB_SHA"))
    except (OSError, ValueError, KeyError):
        report = {"docs_only": False, "reason": "missing or ambiguous event"}
    print(json.dumps(report, sort_keys=True))
    # Only fixed, allowlisted output values; changed filenames never enter shell.
    with Path(os.environ["GITHUB_OUTPUT"]).open("a", encoding="utf-8") as handle:
        handle.write("docs_only=" + ("true" if report["docs_only"] else "false") + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
