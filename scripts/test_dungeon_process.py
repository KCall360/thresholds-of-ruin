"""The fight/retrieve/escape loop through the actual text and native ASCII clients."""
import queue
import threading
import time
import unittest

from process_harness import ProcessTestCase, non_graphical
from stream_relay import StreamRelay


class DungeonProcesses(ProcessTestCase):
    graphical = True

    def start(self, package="dungeon-loop"):
        self.game = self.server(scenario=package, seed=None, spectator=False)

    def native(self, observe=False):
        player = self.launch('tor-client-ascii', ['--connect',self.address,'--automation', *(['--observe'] if observe else [])])
        initial = self.ascii_frame(player, lambda f:f['state'] is not None and not f['busy'])
        return player, initial

    def settled(self, player, tick, current=None):
        if current and (current["state"]["observation"]["combat"]["terminal"] or (int(current["state"]["observation"]["tick"]) > int(tick) and current["state"]["observation"]["ready"])): return current
        return self.ascii_frame(player, lambda f:f['state'] is not None and (f['state']['observation']['combat']['terminal'] or (int(f['state']['observation']['tick']) > int(tick) and f['state']['observation']['ready'])))

    @non_graphical
    def test_text_wins_by_going_to_the_exit(self):
        self.start()
        player, welcome = self.adventure()
        self.assertIn("You are standing at the exit.", welcome)
        self.assertIn("falls dead", self.say(player, "attack ruin guard"))
        taken = self.say(player, "take the dawn seal")
        self.assertIn("pick it up", taken)
        self.assertIn("The exit is to the west.", self.say(player, "look"))
        won = self.say(player, "go to the exit")
        self.assertTrue(won.startswith("You make your way to the exit. You have done it! This run has ended.\n"), won)
        # The end is told once, with a qualified account of this client session.
        self.assertIn("*** You have won ***", won)
        self.assertIn("You made it out. This session recorded", won)
        self.assertIn("goals attempted", won)
        self.assertNotIn("***", self.say(player, "look"))

    @non_graphical
    def test_text_attack_waits_for_resolution_and_permissions_before_pickup(self):
        for hold_at in ("resolved", "readiness"):
            with self.subTest(hold_at=hold_at):
                self.save = self.directory / f"attack-boundary-{hold_at}.db"
                self.start()
                held = threading.Event()
                release = threading.Event()
                admitted = None
                resolved = False

                def hold_attack_boundary(message):
                    nonlocal admitted, resolved
                    if message["type"] == "ack" and message["receipt"]["type"] == "admitted":
                        admitted = message["receipt"]["intention"]
                    if message["type"] != "update" or admitted is None or held.is_set():
                        return
                    body = message["update"]["body"]
                    if body["type"] == "intention":
                        status = body["status"]
                        resolved = status["intention"] == admitted and status["phase"] == "resolved"
                    if resolved and (body["type"] == "readiness" or hold_at == "resolved"):
                        held.set()
                        if not release.wait(15):
                            raise AssertionError("Test did not release the attack boundary")

                relay = StreamRelay(self.address, on_server_message=hold_attack_boundary)
                self.addCleanup(relay.close)
                self.addCleanup(release.set)
                upstream = self.address
                self.address = relay.address
                try:
                    player, welcome = self.adventure()
                finally:
                    self.address = upstream
                self.assertIn("You are standing at the exit.", welcome)
                player.write("attack ruin guard")
                output = []
                try:
                    self.assertTrue(held.wait(15), "Attack never reached its resolution boundary")
                    # The ready observation is already delivered. Hold the actual
                    # lifecycle/permission envelope, rather than hoping a racing
                    # second command happens to land between the server frames.
                    deadline = time.monotonic() + 1
                    while (remaining := deadline - time.monotonic()) > 0:
                        try:
                            line = player.lines.get(timeout=remaining)
                        except queue.Empty:
                            break
                        self.assertIsNotNone(line, "Text client exited before attack resolution")
                        player.transcript.append(line)
                        output.append(line)
                        self.assertNotEqual(line, "> ", "Prompt preceded the attack resolution boundary")
                finally:
                    release.set()
                output.append(player.until(lambda line: line == "> "))
                self.assertIn("falls dead", "\n".join(output))
                self.assertIn("pick it up", self.say(player, "take the dawn seal"))
                self.assertIn("The exit is to the west.", self.say(player, "look"))
                won = self.say(player, "go to the exit")
                self.assertTrue(won.startswith("You make your way to the exit. You have done it! This run has ended.\n"), won)
                self.assertIn("*** You have won ***", won)
                player.stop()
                relay.close()
                self.game.stop()

    @non_graphical
    def test_text_wins_by_going_back_to_the_start(self):
        # The exit isn't disclosed, but the character began there, and the
        # text client remembers where it began.
        self.start()
        player, _ = self.adventure()
        self.assertIn("falls dead", self.say(player, "attack ruin guard"))
        self.assertIn("pick it up", self.say(player, "take the dawn seal"))
        won = self.say(player, "go back to the start")
        self.assertTrue(won.startswith("You make your way back to where you started. You have done it! This run has ended.\n"), won)
        self.assertIn("*** You have won ***", won)

    def test_text_complete_loop_and_saved_victory(self):
        self.start()
        player, _ = self.adventure()
        observer, initial = self.native(True)
        self.assertEqual(initial['state']['observation']['combat']['hp'],20)
        guard = next(actor['id'] for actor in initial['state']['observation']['visible_actors']
                     if actor['name'] == 'ruin guard')
        self.say(player,'attack ruin guard')
        after = self.settled(observer,0)
        self.assertFalse(any(a['id']==guard for a in after['state']['observation']['visible_actors']))
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
            nearby=next((a for a in view['visible_actors'] if a['id']!=view['self_target'] and a['position']['z']==0 and (a['position']['x'],a['position']['y']) in directions),None)
            if nearby:
                key=directions[(nearby['position']['x'],nearby['position']['y'])]
            elif any(i['item']['name']=='dawn seal' and i['reachable'] for i in view['ground_items']): key='pickup'
            else: key='left' if any(i['name']=='dawn seal' for i in view['inventory']) else 'right'
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
