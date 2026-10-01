"""Quantity transfers through real clients using an ordinary validated package."""
import json
from pathlib import Path
import unittest
import test_text_process as support
import test_adventure_process as adventure
import test_ascii_process as ascii_support


class ItemProcesses(unittest.TestCase):
    setUpClass = classmethod(support.TextProcesses.setUpClass.__func__)
    frame = ascii_support.AsciiProcesses.frame
    key = ascii_support.AsciiProcesses.key
    say = adventure.AdventureProcesses.say

    def launch(self, name, args, cls=support.Process):
        p = cls(self.bin / (name + self.suffix), args)
        self.addCleanup(p.stop)
        return p

    def setUp(self):
        directory = support.ProcessTestDirectory()
        self.addCleanup(directory.cleanup)
        self.save = Path(directory.name) / 'items.db'
        self.start()

    def start(self):
        self.server = self.launch('tor-server', ['--listen', '127.0.0.1:0', '--scenario',
            support.ROOT / 'scenarios/tests/items', '--save', self.save])
        self.address = json.loads(self.server.until(lambda s: s.startswith('{')))['address']

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
        p.command('save'); p.stop(); self.server.stop(); self.start()
        resumed = self.launch('tor-client-text', ['--script', '--connect', self.address])
        resumed.until(lambda s: s == 'Ready.')
        self.assertIn('6 x arrow', resumed.command('inventory'))

    def test_adventure_quantity_survives_clarification_and_walk(self):
        p = self.launch('tor-client-text', ['--connect', self.address], adventure.AdventureProcess)
        p.until(lambda s: s == '> ')
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
        initial = self.frame(p, lambda f: f['state'] is not None and not f['busy'])
        opened = self.key(p, 'pickup')
        self.assertEqual(opened['state']['revision'], initial['state']['revision'])
        self.assertEqual(opened['state']['observation']['inventory'], [])
        self.key(p, 'key3')
        self.key(p, 'a')
        taken = self.key(p, 'space')
        stack = taken['state']['observation']['inventory'][0]
        self.assertEqual(stack['quantity'], 3)
        self.assertEqual(taken['inventory_letters'][str(stack['id'])], 'a')
        self.key(p, 'drop')
        self.key(p, 'key2')
        dropped = self.key(p, 'a')
        kept = dropped['state']['observation']['inventory'][0]
        self.assertEqual(kept['id'], stack['id'])
        self.assertEqual(kept['quantity'], 1)
        self.assertEqual(dropped['inventory_letters'][str(kept['id'])], 'a')
        self.assertNotIn('healing', json.dumps(dropped))
        self.assertTrue(dropped['window_open'])


if __name__ == '__main__':
    unittest.main()
