"""Quantity transfers through real clients using an ordinary validated package."""
import json
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
        p = self.launch('tor-client-text', ['--script', '--connect', self.address])
        initial = p.until(lambda s: s == 'Ready.')
        self.assertNotIn('potion of healing', initial)
        self.assertIn('Which item', p.command('take 3 arrows'))
        self.assertIn('Taken', p.command('take 3 #10'))
        self.assertIn('Taken', p.command('take #11'))
        self.assertIn('8 x arrow', p.command('inventory'))
        self.assertIn('Dropped', p.command('drop 2 #23'))
        self.assertIn('6 x arrow', p.command('inventory'))
        self.assertIn('InvalidAction', p.command('drop 99 #23'))
        p.command('save'); p.stop(); self.game.stop(); self.start()
        resumed = self.launch('tor-client-text', ['--script', '--connect', self.address])
        resumed.until(lambda s: s == 'Ready.')
        self.assertIn('6 x arrow', resumed.command('inventory'))

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
        split = self.act(player, {'type': 'take', 'item': 10, 'quantity': 3})
        self.assertIsNone(split['error'])
        stack = split['state']['observation']['inventory'][0]
        self.assertEqual(stack['name'], 'arrow')
        self.assertEqual(stack['quantity'], 3)
        self.assertIsNone(self.request(player, {'type': 'save'})['error'])
        player.stop(); self.game.stop()
        self.game = self.server('--checkpoint-interval', 1, scenario='items', seed=None,
                                wizard=True, spectator=False)
        player, resumed = self.client()
        self.assertEqual(resumed['state']['observation'], split['state']['observation'])
        dropped = self.act(player, {'type': 'drop', 'item': stack['id'], 'quantity': 1})
        self.assertIsNone(dropped['error'])
        self.assertEqual(dropped['state']['observation']['inventory'][0]['quantity'], 2)
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
        self.assertEqual(taken['state']['observation']['inventory'][0]['quantity'], 3)
        self.key(p, 'drop')
        p.child.stdin.write(json.dumps({'type':'text','text':'2'}) + '\n'); p.child.stdin.flush()
        dropped = self.key(p, 'enter')
        self.assertEqual(dropped['state']['observation']['inventory'][0]['quantity'], 1)
        self.assertNotIn('healing', json.dumps(dropped))
        self.assertTrue(dropped['window_open'])


if __name__ == '__main__':
    unittest.main()
