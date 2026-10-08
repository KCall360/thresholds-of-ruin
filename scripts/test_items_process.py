"""Quantity transfers through real clients using an ordinary validated package."""
import json
import sqlite3
import unittest

from process_harness import ProcessTestCase


class ItemProcesses(ProcessTestCase):
    graphical = True

    def setUp(self):
        super().setUp()
        self.start()

    def start(self, package="items"):
        self.game = self.server(scenario=package, seed=None, spectator=False)

    def test_direct_quantities_merge_drop_and_restart(self):
        probe, disclosed = self.client(observe=True)
        arrows = {ground['item']['quantity']: ground['item']['id']
                  for ground in disclosed['state']['observation']['ground_items']
                  if ground['item']['name'] == 'arrow'}
        p = self.launch('tor-client-text', ['--script', '--connect', self.address])
        initial = p.until(lambda s: s == 'Ready.')
        self.assertNotIn('potion of healing', initial)
        self.assertIn('Which item', p.command('take 3 arrows'))
        self.assertIn('Taken', p.command(f"take 3 #{arrows['10']}"))
        self.assertIn('Taken', p.command(f"take #{arrows['5']}"))
        self.assertIn('8 x arrow', p.command('inventory'))
        held = self.request(probe, {'type': 'snapshot'})['state']['observation']['inventory'][0]['id']
        self.assertIn('Dropped', p.command(f'drop 2 #{held}'))
        self.assertIn('6 x arrow', p.command('inventory'))
        self.assertIn('InvalidAction', p.command(f'drop 99 #{held}'))
        p.command('save'); p.stop(); probe.stop(); self.game.stop(); self.start()
        resumed = self.launch('tor-client-text', ['--script', '--connect', self.address])
        resumed.until(lambda s: s == 'Ready.')
        self.assertIn('6 x arrow', resumed.command('inventory'))

    def test_saved_action_facts_preserve_original_quantity_and_execution(self):
        player, initial = self.client()
        arrows = {ground['item']['quantity']: ground['item']['id']
                  for ground in initial['state']['observation']['ground_items']
                  if ground['item']['name'] == 'arrow'}
        for item, quantity in [(arrows['10'], None), (arrows['5'], "2")]:
            taken = self.act(player, {"type": "take", "item": item, "quantity": quantity})
            self.assertIsNone(taken["error"])
        self.assertEqual(taken["state"]["observation"]["inventory"][0]["quantity"], "12")
        self.assertIsNone(self.request(player, {"type": "save"})["error"])
        with sqlite3.connect(self.save) as db:
            rows = db.execute("SELECT sequence,frame FROM journal WHERE sequence>0 "
                "UNION ALL SELECT sequence,frame FROM history ORDER BY sequence").fetchall()
        records = [json.loads(frame[24:])["record"] for _, frame in rows]
        admissions = [r for r in records if r["entry"]["content"]["type"] == "intention_admitted"]
        self.assertEqual(len(admissions), 2)
        for record, item, quantity, executed_quantity in zip(admissions, [10, 11], [None, 2], [10, 2]):
            content = record["entry"]["content"]
            original = {"type": "take", "item": item, "quantity": quantity}
            self.assertEqual(record["receipt"]["command"]["action"], original)
            self.assertEqual(content["action"], original)
            effects = [r for r in records if r["entry"]["content"].get("admission") == record["entry"]["id"]]
            self.assertEqual(len(effects), 1)
            effect = effects[0]["entry"]["content"]
            self.assertEqual(effect["action"], original)
            self.assertEqual(effect["event"]["quantity"], executed_quantity)
            self.assertIsNone(effects[0]["receipt"])
        player.stop(); self.game.stop(); self.start()
        _, restored = self.client()
        self.assertEqual(restored["state"], taken["state"])
        self.assertEqual(restored["intentions"], [])

    def test_adventure_quantities_count_from_one_stack(self):
        p, _ = self.adventure()
        # The arrow stacks look alike, so a count comes from one of them
        # without a question.
        self.assertEqual('You pick up two arrows.\n> ', self.say(p, 'take 2 arrows'))
        self.assertIn('two arrows', self.say(p, 'inventory'))
        self.assertEqual('You drop an arrow.\n> ', self.say(p, 'drop 1 arrow'))
        self.assertEqual('You pick up a red potion.\n> ', self.say(p, 'take 1 potion'))
        inventory = self.say(p, 'inventory')
        self.assertIn('a red potion', inventory)
        self.assertIn('an arrow', inventory)
        self.assertNotIn('healing', inventory)

    def test_split_checkpoint_restore_and_rewind_preserve_stack_definitions(self):
        self.game.stop()
        self.game = self.server('--checkpoint-interval', 1, scenario='items', seed=None,
                                wizard=True, spectator=False)
        player, initial = self.client()

        def collection_edits(start):
            updates = []
            for line in player.transcript[start:]:
                if not line.startswith('{'):
                    continue
                frame = json.loads(line)
                update = (frame.get('message') or {}).get('update') or {}
                body = update.get('body') or {}
                if body.get('type') != 'observation_delta':
                    continue
                self.assertTrue(frame['synchronized'])
                for field in ['inventory', 'ground_items', 'visible_actors', 'places']:
                    for edit in body['state'][field]:
                        self.assertEqual(set(edit), {'start', 'remove', 'insert'})
                updates.append(body['state'])
            self.assertTrue(updates, 'Real item transfer must exercise a collection delta')
            return updates[-1]

        start = len(player.transcript)
        target = next(ground['item']['id'] for ground in initial['state']['observation']['ground_items']
                      if ground['item']['name'] == 'arrow' and ground['item']['quantity'] == '10')
        split = self.act(player, {'type': 'take', 'item': target, 'quantity': '3'})
        edits = collection_edits(start)
        self.assertTrue(edits['inventory'])
        self.assertTrue(edits['ground_items'])
        self.assertEqual(edits['places'], [])
        self.assertEqual(edits['visible_actors'], [])
        self.assertIsNone(split['error'])
        stack = split['state']['observation']['inventory'][0]
        self.assertEqual(stack['name'], 'arrow')
        self.assertEqual(stack['quantity'], '3')
        start = len(player.transcript)
        waited = self.act(player, {'type': 'wait'})
        self.assertIsNone(waited['error'])
        edits = collection_edits(start)
        for field in ['inventory', 'ground_items', 'visible_actors', 'places']:
            self.assertEqual(edits[field], [], f'Unchanged {field} was retransmitted')
        # Save/restart must preserve the actual reconstructed observation.
        split = waited
        self.assertIsNone(self.request(player, {'type': 'save'})['error'])
        player.stop(); self.game.stop()
        self.game = self.server('--checkpoint-interval', 1, scenario='items', seed=None,
                                wizard=True, spectator=False)
        player, resumed = self.client()
        self.assertEqual(resumed['state']['observation'], split['state']['observation'])
        start = len(player.transcript)
        dropped = self.act(player, {'type': 'drop', 'item': stack['id'], 'quantity': '1'})
        edits = collection_edits(start)
        self.assertTrue(edits['inventory'])
        self.assertTrue(edits['ground_items'])
        self.assertIsNone(dropped['error'])
        self.assertEqual(dropped['state']['observation']['inventory'][0]['quantity'], '2')
        wizard = self.wizard()
        self.wizard_command(wizard, 'rewind initial')
        rewound = self.request(player, {'type': 'snapshot'})
        self.assertEqual(rewound['state']['observation'], initial['state']['observation'])
        self.assertIsNone(self.request(player, {'type': 'save'})['error'])
        player.stop(); wizard.stop(); self.game.stop()
        self.start()
        _, restored = self.client()
        self.assertEqual(restored['state']['observation'], initial['state']['observation'])

    def test_ascii_quantity_picker_and_drop_present_authoritative_counts(self):
        p = self.launch('tor-client-ascii', ['--connect', self.address, '--automation'])
        self.ascii_frame(p, lambda f: f['state'] is not None and not f['busy'])
        self.key(p, 'pickup')
        p.child.stdin.write(json.dumps({'type':'text','text':'3'}) + '\n'); p.child.stdin.flush()
        taken = self.key(p, 'enter')
        self.assertEqual(taken['state']['observation']['inventory'][0]['quantity'], '3')
        self.key(p, 'drop')
        p.child.stdin.write(json.dumps({'type':'text','text':'2'}) + '\n'); p.child.stdin.flush()
        dropped = self.key(p, 'enter')
        self.assertEqual(dropped['state']['observation']['inventory'][0]['quantity'], '1')
        self.assertNotIn('healing', json.dumps(dropped))
        self.assertTrue(dropped['window_open'])


if __name__ == '__main__':
    unittest.main()
