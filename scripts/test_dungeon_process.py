"""The fight/retrieve/escape loop through the actual text and native ASCII clients."""
import unittest

from process_harness import ProcessTestCase


class DungeonProcesses(ProcessTestCase):
    graphical = True

    def start(self, package="dungeon-loop"):
        self.game = self.server(scenario=package, seed=None, spectator=False)

    def native(self, observe=False):
        player = self.launch('tor-client-ascii', ['--connect',self.address,'--automation', *(['--observe'] if observe else [])])
        initial = self.ascii_frame(player, lambda f:f['state'] is not None and not f['busy'])
        return player, initial

    def settled(self, player, tick, current=None):
        if current and (current["state"]["observation"]["combat"]["terminal"] or (current["state"]["observation"]["tick"] > tick and current["state"]["observation"]["ready"])): return current
        return self.ascii_frame(player, lambda f:f['state'] is not None and (f['state']['observation']['combat']['terminal'] or (f['state']['observation']['tick'] > tick and f['state']['observation']['ready'])))

    def test_text_complete_loop_and_saved_victory(self):
        self.start()
        player, _ = self.adventure()
        observer, initial = self.native(True)
        self.assertEqual(initial['state']['observation']['combat']['hp'],20)
        self.say(player,'attack ruin guard')
        after = self.settled(observer,0)
        self.assertFalse(any(a['id']==2 for a in after['state']['observation']['visible_actors']))
        for command in ['step east','step east','step east','get dawn seal','step west','step west','step west']:
            tick = after['state']['observation']['tick']
            self.say(player,command)
            after = self.settled(observer,tick)
        self.assertTrue(after['state']['observation']['combat']['victory'])
        description = self.say(player,'look')
        self.assertIn('Victory',description)
        self.assertNotIn('must wait',description)
        try:
            self.say(player,'save')
        except AssertionError as error:
            # Preserve server diagnostics when a durable barrier fails or times out.
            pending = list(self.game.lines.queue)
            raise AssertionError(f'{error}\nServer output: {self.game.transcript + pending}') from error
        player.stop(); observer.stop(); self.game.stop()
        self.start()
        restored, state = self.native(True)
        self.assertTrue(state['state']['observation']['combat']['terminal'])
        self.assertTrue(state['state']['observation']['combat']['victory'])

    def test_native_complete_default_dungeon(self):
        self.start('first-dungeon')
        player, current = self.native()
        directions={(0,-1):'up',(1,-1):'north_east',(1,0):'right',(1,1):'south_east',(0,1):'down',(-1,1):'south_west',(-1,0):'left',(-1,-1):'north_west'}
        for _ in range(160):
            view=current['state']['observation']
            self.assertFalse(view['combat']['dead'])
            if view['combat']['victory']: return
            nearby=next((a for a in view['visible_actors'] if a['id']!=1 and a['position']['z']==0 and (a['position']['x'],a['position']['y']) in directions),None)
            if nearby:
                key=directions[(nearby['position']['x'],nearby['position']['y'])]
            elif any(i['item']['id']==100 and i['reachable'] for i in view['ground_items']): key='pickup'
            else: key='left' if any(i['id']==100 for i in view['inventory']) else 'right'
            result=self.key(player,key)
            if result['state']['observation']['ready'] and result['state']['revision']==current['state']['revision']:
                result=self.key(player,'wait')
            current=self.settled(player,view['tick'],result)
        self.fail('Default dungeon did not reach victory within the bounded walkthrough')

    def test_native_death_is_terminal_and_persistent(self):
        self.start('dungeon-death')
        player, _ = self.native()
        self.key(player,'attack'); result = self.key(player,'enter')
        dead = self.settled(player,0,result)
        self.assertTrue(dead['state']['observation']['combat']['dead'])
        unchanged = self.key(player,'wait')
        self.assertEqual(unchanged['state'],dead['state'])
        player.stop(); self.game.stop()
        self.start('dungeon-death')
        player, restored = self.native(True)
        self.assertTrue(restored['state']['observation']['combat']['dead'])

if __name__ == '__main__': unittest.main()
