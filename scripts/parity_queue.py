#!/usr/bin/env python3
"""Read the durable queue. A check result cannot override unfinished work.

This is a continuation decision interface, not a scheduler. It never edits
evidence, advances states, starts another agent or claims runner configuration.
"""
import argparse
import json
from pathlib import Path

STATES = {
    "Implementation remaining", "Implemented, validation remaining",
    "Blocked", "Verified complete",
}


def inspect(queue):
    items = queue["items"]
    expected = {f"{p}{n:02}" for p, count in
                [("F", 6), ("T", 6), ("W", 5), ("S", 7), ("D", 4),
                 ("X", 5), ("P", 8), ("B", 6), ("E", 5), ("V", 8)]
                for n in range(1, count + 1)}
    by_id = {item["id"]: item for item in items}
    if len(items) != 60 or set(by_id) != expected:
        raise ValueError("Queue must contain each of the 60 approved IDs exactly once")
    for item in items:
        if item["state"] not in STATES:
            raise ValueError(f"Invalid state for {item['id']}")
        if item["state"] == "Blocked" and not item.get("dependency", "").strip():
            raise ValueError(f"Blocked item {item['id']} needs an exact dependency")
        if set(item["requires"]) - expected:
            raise ValueError(f"Unknown dependency for {item['id']}")
        if item["state"] == "Verified complete" and (
                not item["evidence"] or item.get("validation_remaining") or
                not item["implementation_checkpoint"]):
            raise ValueError(f"Complete item {item['id']} lacks completion evidence")
    visiting, visited = set(), set()

    def visit(key):
        if key in visiting:
            raise ValueError(f"Dependency cycle at {key}")
        if key in visited:
            return
        visiting.add(key)
        for dependency in by_id[key]["requires"]:
            visit(dependency)
        visiting.remove(key)
        visited.add(key)

    for key in by_id:
        visit(key)
    actionable = [item for item in items
                  if item["state"] == "Implementation remaining" and
                  all(by_id[key]["implementation_checkpoint"]
                      for key in item["requires"])]
    unfinished = [item["id"] for item in items
                  if item["state"] != "Verified complete"]
    return {
        "decision": "continue" if actionable else
                    ("validation_or_external_input_required" if unfinished else "complete"),
        "actionable": [item["id"] for item in actionable],
        "unfinished": unfinished,
        "automatic_continuation_configured": queue["runner"]["automatic_continuation_configured"],
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--queue", type=Path, default=Path(__file__).resolve().parents[1] /
                        "PARITY_EXECUTION_QUEUE.json")
    parser.add_argument("--builder-result", choices=["partial", "complete", "blocked"])
    parser.add_argument("--check-status", help="Informational only; never closes the backlog")
    args = parser.parse_args()
    print(json.dumps(inspect(json.loads(args.queue.read_text())), indent=2))
