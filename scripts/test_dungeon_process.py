"""The fight/retrieve/escape loop through the actual text and native ASCII clients."""
import json
from pathlib import Path
import unittest
import test_text_process as support
import test_ascii_process as ascii_support
import test_adventure_process as adventure

class DungeonProcesses(unittest.TestCase):
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
        self.save = Path(directory.name) / 'dungeon.db'

    def start(self, package='dungeon-loop'):
        self.server = self.launch('tor-server', ['--listen','127.0.0.1:0', '--scenario',support.ROOT / ('scenarios' if package == 'first-dungeon' else 'scenarios/tests') / package, '--save',self.save])
        self.address = json.loads(self.server.until(lambda s:s.startswith('{')))['address']

    def native(self, observe=False):
        player = self.launch('tor-client-ascii', ['--connect',self.address,'--automation', *(['--observe'] if observe else [])])
        initial = self.frame(player, lambda f:f['state'] is not None and not f['busy'])
        return player, initial

    def settled(self, player, tick, current=None):
        if current and (current["state"]["observation"]["combat"]["terminal"] or (current["state"]["observation"]["tick"] > tick and current["state"]["observation"]["ready"])): return current
        return self.frame(player, lambda f:f['state'] is not None and (f['state']['observation']['combat']['terminal'] or (f['state']['observation']['tick'] > tick and f['state']['observation']['ready'])))

    def test_text_complete_loop_and_saved_victory(self):
        self.start()
        player = self.launch('tor-client-text',['--connect',self.address],adventure.AdventureProcess)
        player.until(lambda s:s=='> ')
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
            pending = list(self.server.lines.queue)
            raise AssertionError(f'{error}\nServer output: {self.server.transcript + pending}') from error
        player.stop(); observer.stop(); self.server.stop()
        self.start()
        restored, state = self.native(True)
        self.assertTrue(state['state']['observation']['combat']['terminal'])
        self.assertTrue(state['state']['observation']['combat']['victory'])

    def test_native_bump_attack_retrieval_and_escape(self):
        self.start()
        player, initial = self.native()
        result = self.key(player,'right')
        after = self.settled(player,0,result)
        self.assertEqual(after['state']['observation']['combat']['hp'],20)
        for key in ['right','right','right','pickup','left','left','left']:
            result = self.key(player,key)
            if key=='pickup' and result.get('pickup'):
                result=self.key(player,'enter')
            after=result
        self.assertTrue(after['state']['observation']['combat']['victory'])
        self.assertTrue(after['window_open'])

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
        player.stop(); self.server.stop()
        self.start('dungeon-death')
        player, restored = self.native(True)
        self.assertTrue(restored['state']['observation']['combat']['dead'])

if __name__ == '__main__': unittest.main()
