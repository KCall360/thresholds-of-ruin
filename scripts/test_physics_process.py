"""Gravity, body disclosure, and resume through the real text and ASCII clients."""
import json
from pathlib import Path
import unittest
import test_text_process as support
import test_ascii_process as ascii_support
import test_adventure_process as adventure


class PhysicsProcesses(unittest.TestCase):
    setUpClass = classmethod(support.TextProcesses.setUpClass.__func__)
    frame = ascii_support.AsciiProcesses.frame
    key = ascii_support.AsciiProcesses.key
    say = adventure.AdventureProcesses.say

    def launch(self, name, args, cls=support.Process):
        process = cls(self.bin / (name + self.suffix), args)
        self.addCleanup(process.stop)
        return process

    def setUp(self):
        directory = support.ProcessTestDirectory()
        self.addCleanup(directory.cleanup)
        self.save = Path(directory.name) / 'physics.db'
        self.start()

    def start(self, package="physics"):
        self.server = self.launch('tor-server', ['--listen', '127.0.0.1:0',
            '--scenario', support.ROOT / 'scenarios/tests' / package, '--save', self.save])
        self.address = json.loads(self.server.until(lambda s: s.startswith('{')))['address']

    def test_text_falling_narration_and_saved_continuation(self):
        player = self.launch('tor-client-text', ['--connect', self.address], adventure.AdventureProcess)
        player.until(lambda s: s == '> ')
        self.assertIn("can't go", self.say(player, 'step east').lower(), 'unsupported walking is free')
        self.assertIn('move involuntarily', self.say(player, 'wait'))
        self.say(player, 'save')
        player.stop()
        self.server.stop()
        self.start()
        player = self.launch('tor-client-text', ['--connect', self.address], adventure.AdventureProcess)
        player.until(lambda s: s == '> ')
        transcript = '\n'.join(self.say(player, 'wait') for _ in range(5))
        self.assertIn('collide with an obstruction', transcript)
        self.assertIn('move east', self.say(player, 'step east'))

    def test_ascii_discloses_body_cells_and_updates_falling_state(self):
        player = self.launch('tor-client-ascii', ['--connect', self.address, '--automation'])
        initial = self.frame(player, lambda f: f['state'] is not None and not f['busy'])
        view = initial['state']['observation']
        self.assertTrue(any(a['id'] == 1 and a['position']['z'] == 1 for a in view['visible_actors']))
        falling = self.key(player, 'wait')['state']['observation']
        self.assertTrue(falling['motion']['displaced'])
        self.assertLess(falling['motion']['velocity'][2], 0)
        impacted = False
        for _ in range(5):
            current = self.key(player, 'wait')
            impacted |= current['state']['observation']['motion']['impacted']
        self.assertTrue(impacted)
        self.assertEqual(current['state']['observation']['motion']['velocity'], [0, 0, 0])
        self.assertTrue(current['window_open'])


    def test_ascii_sideways_portal_preserves_observer_axes(self):
        self.server.stop()
        self.save=self.save.with_name('portal.db')
        self.start('physics-portal')
        player=self.launch('tor-client-ascii',['--connect',self.address,'--automation'])
        self.frame(player,lambda f:f['state'] is not None and not f['busy'])
        crossed=self.key(player,'wait')['state']['observation']
        self.assertTrue(crossed['motion']['displaced'])
        self.assertGreater(crossed['motion']['velocity'][0],4096)
        self.assertEqual(crossed['motion']['velocity'][2],0)
        self.assertTrue(any(a['id']==1 and a['position']['z']==1 for a in crossed['visible_actors']))
        self.assertNotIn('region',json.dumps(crossed))


if __name__ == '__main__':
    unittest.main()
