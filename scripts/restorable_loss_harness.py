"""Shared ordinary-action probes for restorable-loss process acceptance."""
from scalar_wire import u64
from restorable_loss_package import LOSS_HEALTH, LOSS_STRENGTH, WOUND
from status_condition_package import combat, observation


def losses(frame):
    return combat(frame)["own_stats"]["restorable_losses"]


def self_ability(frame, identity):
    return {"type": "use_ability", "id": identity,
            "invocation": {"aim": {"kind": "actor_target", "target": observation(frame)["self_target"]}, "weapon": {"kind": "default"}},
            "supplies": {"kind": "carried"}}


class LossHelpers:
    def snapshot(self, player):
        return self.request(player, {"type": "snapshot"})

    def finish(self, player, peer=None):
        for _ in range(20):
            frame = self.snapshot(player)
            if (observation(frame)["ready"] and combat(frame)["preparation_remaining"] is None
                    and int(combat(frame)["recovery_remaining"]) == 0):
                return frame
            self.assertIsNotNone(peer, "a solo action should finish before its next admission")
            self.assertIsNone(self.act(peer, {"type": "wait"})["error"])
        self.fail("the authored action did not finish within its bounded preparation")

    def apply_losses(self, player, peer=None):
        initial = self.snapshot(player)
        for identity in (WOUND, LOSS_STRENGTH, LOSS_HEALTH):
            frame = self.snapshot(player)
            self.assertIsNone(self.act(player, self_ability(frame, identity))["error"])
            self.finish(player, peer)
        changed = self.snapshot(player)
        self.assertEqual(losses(changed)["attributes"]["strength"], 1)
        self.assertEqual(losses(changed)["maximum_health"], 1)
        self.assertEqual(u64(combat(changed)["max_hp"]), u64(combat(initial)["max_hp"]) - 2)
        self.assertEqual(u64(combat(changed)["max_hp"]) - u64(combat(changed)["hp"]), 2)
        return changed

    def restoration_item(self, frame):
        return next(item["id"] for item in observation(frame)["inventory"]
                    if item["name"] == "fixture restoration")
