import copy
import json
from pathlib import Path
import unittest
from parity_queue import inspect


class QueueTests(unittest.TestCase):
    def setUp(self):
        self.queue = json.loads((Path(__file__).resolve().parents[1] /
                                 "PARITY_EXECUTION_QUEUE.json").read_text())

    def test_d03_is_independent_of_unfinished_d02_and_native_d01(self):
        # Exercise the scheduling rule with explicit fixture states, rather than
        # assuming the live backlog will always leave D03 unimplemented.
        for item in self.queue["items"]:
            if item["id"]=="D03":item.update(state="Implementation remaining",implementation_checkpoint=False)
            if item["id"]=="D02":item.update(state="Implementation remaining",implementation_checkpoint=False)
            if item["id"]=="D01":item.update(state="Implemented, validation remaining",implementation_checkpoint=True)
        result = inspect(self.queue)
        self.assertIn("D03", result["actionable"])
        self.assertIn("D02", result["unfinished"])
        self.assertEqual(result["decision"], "continue")
        self.assertFalse(result["automatic_continuation_configured"])

    def test_missing_requirements_and_unexplained_blocks_are_errors(self):
        broken = copy.deepcopy(self.queue)
        broken["items"].pop()
        with self.assertRaises(ValueError):
            inspect(broken)
        broken = copy.deepcopy(self.queue)
        broken["items"][0].update(state="Blocked", dependency="")
        with self.assertRaises(ValueError):
            inspect(broken)

    def test_validation_pending_never_reports_goal_complete(self):
        for item in self.queue["items"]:
            item["state"] = "Implemented, validation remaining"
            item["implementation_checkpoint"] = True
        self.assertEqual(inspect(self.queue)["decision"], "validation_or_external_input_required")

    def test_cycles_and_completion_without_evidence_are_errors(self):
        self.queue["items"][0]["requires"] = ["F01"]
        with self.assertRaises(ValueError):
            inspect(self.queue)
        self.queue["items"][0].update(requires=[], state="Verified complete")
        with self.assertRaises(ValueError):
            inspect(self.queue)


if __name__ == "__main__":
    unittest.main()
