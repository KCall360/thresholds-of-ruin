"""Gravity, body disclosure, and resume through the real text and ASCII clients."""
import json
import unittest

from process_harness import ProcessTestCase


class PhysicsProcesses(ProcessTestCase):
    graphical = True

    def setUp(self):
        super().setUp()
        self.start()

    def start(self, package="physics"):
        self.game = self.server(scenario=package, seed=None, spectator=False)

    def test_text_falling_narration_and_saved_continuation(self):
        player, _ = self.adventure()
        self.assertIn("can't go", self.say(player, 'step east').lower(), 'unsupported walking is free')
        self.assertIn('move involuntarily', self.say(player, 'wait'))
        self.say(player, 'save')
        player.stop()
        self.game.stop()
        self.start()
        player, _ = self.adventure()
        transcript = '\n'.join(self.say(player, 'wait') for _ in range(5))
        self.assertIn('collide with an obstruction', transcript)
        self.assertIn('move east', self.say(player, 'step east'))

    def test_ascii_discloses_body_cells_and_updates_falling_state(self):
        player = self.launch('tor-client-ascii', ['--connect', self.address, '--automation'])
        initial = self.ascii_frame(player, lambda f: f['state'] is not None and not f['busy'])
        view = initial['state']['observation']
        self.assertTrue(any(a['id'] == 1 and a['position']['z'] == 1 for a in view['visible_actors']))
        browsed=self.key(player,'map_lower')
        self.assertEqual(browsed['state']['observation']['tick'],0)
        self.assertIn('Viewing height -1',browsed['status'])
        self.key(player,'map_higher')
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
        self.game.stop()
        self.save=self.save.with_name('portal.db')
        self.start('physics-portal')
        player=self.launch('tor-client-ascii',['--connect',self.address,'--automation'])
        self.ascii_frame(player,lambda f:f['state'] is not None and not f['busy'])
        crossed=self.key(player,'wait')['state']['observation']
        self.assertTrue(crossed['motion']['displaced'])
        self.assertGreater(crossed['motion']['velocity'][0],4096)
        self.assertEqual(crossed['motion']['velocity'][2],0)
        self.assertTrue(any(a['id']==1 and a['position']['z']==1 for a in crossed['visible_actors']))
        self.assertNotIn('region',json.dumps(crossed))


if __name__ == '__main__':
    unittest.main()
