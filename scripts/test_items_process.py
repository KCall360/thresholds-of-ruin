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

    def test_adventure_quantity_survives_clarification_and_walk(self):
        p, _ = self.adventure()
        self.assertIn('Which', self.say(p, 'take 2 arrows'))
        self.assertIn('pick up 2', self.say(p, '1'))
        self.assertIn('2 x arrow', self.say(p, 'inventory'))
        self.assertIn('drop 1', self.say(p, 'drop 1 arrow'))
        self.assertIn('pick it up', self.say(p, 'take 1 #22'))
        inventory = self.say(p, 'inventory')
        self.assertIn('1 x red potion', inventory)
        self.assertNotIn('healing', inventory)

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
