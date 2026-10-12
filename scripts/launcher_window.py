"""The graphical launcher: one window to choose clients, scenario, seed and save.

`launch_game.py` opens this window unless `--console` is given. Games run on a
worker thread through `launch_game.launch`, so the window stays responsive and
can start another game after one ends. Text clients get their own console
windows on Windows; on other platforms the text player uses the terminal that
started the launcher.

`--automation` (process tests only) reads JSON commands on stdin and prints one
JSON state report per line; see `Automation`.
"""
import json
from pathlib import Path
import queue
import random
import sys
import threading
import tkinter as tk
from tkinter import filedialog, ttk

import launch_game
from launch_game import Choices, LaunchError

TITLE = "Thresholds of Ruin"
CLIENT_LABELS = {"text": "Text adventure", "ascii": "ASCII map"}
SPECTATOR_LABELS = {"none": "None", "text": "Text", "ascii": "ASCII map"}
NEW, SAVED = "new", "saved"
SAVE_FILETYPES = [("Thresholds of Ruin saves", " ".join(launch_game.SAVE_FILES)), ("All files", "*")]


class LauncherWindow:
    def __init__(self, root, *, bin_dir, scenarios_dir, saves_dir, report_frames=False,
                 start=None, ask_save_file=None, initial=None):
        self.root = root
        self.bin_dir = Path(bin_dir)
        self.scenarios_dir = Path(scenarios_dir)
        self.saves_dir = Path(saves_dir).resolve()
        self.report_frames = report_frames
        self.start = start or self._start_thread
        self.ask_save_file = ask_save_file or self._ask_save_file
        self.listeners = []
        self.events = queue.Queue()
        self.session = None
        self.running = False
        self.closing = False
        self.destroyed = False
        self.saves = {}  # tree item id -> launch_game.Save
        self.available = launch_game.scenarios(self.scenarios_dir)
        initial = initial or {}

        root.title(TITLE)
        root.minsize(560, 460)
        self.client = tk.StringVar(root, initial.get("client") or "ascii")
        self.spectator = tk.StringVar(root, initial.get("spectator") or "none")
        names = list(self.available)
        default_scenario = "rogue-exploration" if "rogue-exploration" in names else (names[0] if names else "")
        self.scenario = tk.StringVar(root, initial.get("scenario") or default_scenario)
        self.seed = tk.StringVar(root, initial.get("seed") or str(random.randrange(2**32)))
        self.name = tk.StringVar(root, initial.get("new") or launch_game.default_name())
        self.origin = tk.StringVar(root, "")
        self.status = tk.StringVar(root, "")
        self._build(names)
        self.refresh_saves()
        if initial.get("save"):
            self.tabs.select(self.saved_tab)
            try:
                self.add_save(launch_game.resolve_save(initial["save"], self.saves_dir))
            except LaunchError as error:
                self.set_status(str(error), error=True)
        root.protocol("WM_DELETE_WINDOW", self.close)
        root.after(100, self._drain_events)

    # Layout.

    def _build(self, names):
        outer = ttk.Frame(self.root, padding=12)
        outer.pack(fill="both", expand=True)
        outer.columnconfigure(0, weight=1)
        outer.columnconfigure(1, weight=1)

        player = ttk.LabelFrame(outer, text="Player client", padding=8)
        player.grid(row=0, column=0, sticky="nsew", padx=(0, 6))
        self.client_buttons = {}
        for value in launch_game.CLIENTS:
            button = ttk.Radiobutton(player, text=CLIENT_LABELS[value], value=value, variable=self.client)
            button.pack(anchor="w")
            self.client_buttons[value] = button

        spectator = ttk.LabelFrame(outer, text="Spectator (read-only)", padding=8)
        spectator.grid(row=0, column=1, sticky="nsew", padx=(6, 0))
        self.spectator_buttons = {}
        for value in launch_game.SPECTATORS:
            button = ttk.Radiobutton(spectator, text=SPECTATOR_LABELS[value], value=value, variable=self.spectator)
            button.pack(anchor="w")
            self.spectator_buttons[value] = button
        if not launch_game.WINDOWS:
            self.spectator_buttons["text"].state(["disabled"])
            if self.spectator.get() == "text":
                self.spectator.set("none")

        self.tabs = ttk.Notebook(outer)
        self.tabs.grid(row=1, column=0, columnspan=2, sticky="nsew", pady=(12, 0))
        outer.rowconfigure(1, weight=1)

        self.new_tab = ttk.Frame(self.tabs, padding=10)
        self.new_tab.columnconfigure(1, weight=1)
        self.tabs.add(self.new_tab, text="New game")
        ttk.Label(self.new_tab, text="Scenario").grid(row=0, column=0, sticky="w", padx=(0, 10), pady=4)
        self.scenario_box = ttk.Combobox(self.new_tab, textvariable=self.scenario, values=names, state="readonly")
        self.scenario_box.grid(row=0, column=1, columnspan=2, sticky="ew", pady=4)
        ttk.Label(self.new_tab, text="Seed").grid(row=1, column=0, sticky="w", padx=(0, 10), pady=4)
        self.seed_entry = ttk.Entry(self.new_tab, textvariable=self.seed)
        self.seed_entry.grid(row=1, column=1, sticky="ew", pady=4)
        self.random_button = ttk.Button(self.new_tab, text="Random", command=self.random_seed)
        self.random_button.grid(row=1, column=2, sticky="e", padx=(6, 0), pady=4)
        ttk.Label(self.new_tab, text="Save name").grid(row=2, column=0, sticky="w", padx=(0, 10), pady=4)
        self.name_entry = ttk.Entry(self.new_tab, textvariable=self.name)
        self.name_entry.grid(row=2, column=1, columnspan=2, sticky="ew", pady=4)
        ttk.Label(self.new_tab, text=f"Saved in {self.saves_dir}", foreground="gray").grid(
            row=3, column=0, columnspan=3, sticky="w", pady=(8, 0))

        self.saved_tab = ttk.Frame(self.tabs, padding=10)
        self.saved_tab.columnconfigure(0, weight=1)
        self.saved_tab.rowconfigure(0, weight=1)
        self.tabs.add(self.saved_tab, text="Saved games")
        columns = ("scenario", "seed", "saved")
        self.tree = ttk.Treeview(self.saved_tab, columns=columns, selectmode="browse", height=8)
        self.tree.heading("#0", text="Game")
        self.tree.heading("scenario", text="Scenario")
        self.tree.heading("seed", text="Seed")
        self.tree.heading("saved", text="Last saved")
        self.tree.column("#0", width=180)
        self.tree.column("scenario", width=130)
        self.tree.column("seed", width=90)
        self.tree.column("saved", width=130)
        scroll = ttk.Scrollbar(self.saved_tab, orient="vertical", command=self.tree.yview)
        self.tree.configure(yscrollcommand=scroll.set)
        self.tree.grid(row=0, column=0, columnspan=3, sticky="nsew")
        scroll.grid(row=0, column=3, sticky="ns")
        self.tree.bind("<<TreeviewSelect>>", lambda event: self._selection_changed())
        self.tree.bind("<Double-1>", lambda event: self.play())
        ttk.Label(self.saved_tab, text="Started from").grid(row=1, column=0, sticky="w", pady=(8, 0))
        self.origin_box = ttk.Combobox(self.saved_tab, textvariable=self.origin, values=names, state="disabled")
        self.origin_box.grid(row=1, column=1, sticky="ew", pady=(8, 0))
        self.browse_button = ttk.Button(self.saved_tab, text="Browse…", command=self.browse)
        self.browse_button.grid(row=1, column=2, sticky="e", padx=(6, 0), pady=(8, 0))

        actions = ttk.Frame(outer)
        actions.grid(row=2, column=0, columnspan=2, sticky="ew", pady=(12, 0))
        actions.columnconfigure(0, weight=1)
        self.status_label = ttk.Label(actions, textvariable=self.status, wraplength=380)
        self.status_label.grid(row=0, column=0, sticky="w")
        self.end_button = ttk.Button(actions, text="End game", command=self.end_game, state="disabled")
        self.end_button.grid(row=0, column=1, padx=(6, 0))
        self.play_button = ttk.Button(actions, text="Play", command=self.play, default="active")
        self.play_button.grid(row=0, column=2, padx=(6, 0))
        self.root.bind("<Return>", lambda event: self.play())

    # Saves.

    def refresh_saves(self, select=None):
        """List every save in the saves directory, newest first, keeping browsed ones."""
        browsed = {item: save for item, save in self.saves.items() if save.directory.parent != self.saves_dir}
        self.tree.delete(*self.tree.get_children())
        self.saves = {}
        for save in [*launch_game.saves(self.saves_dir), *browsed.values()]:
            self.add_save(save, select=False)
        if select:
            self.select_save(select)
        self._selection_changed()

    def add_save(self, save, select=True):
        for item, existing in self.saves.items():
            if existing.path == save.path:
                break
        else:
            seed = save.metadata.get("seed")
            saved = launch_game.datetime.datetime.fromtimestamp(save.path.stat().st_mtime).strftime("%Y-%m-%d %H:%M")
            in_saves = save.directory.parent == self.saves_dir
            item = self.tree.insert("", "end", text=save.name if in_saves else str(save.path), values=(
                save.metadata.get("scenario", "unknown"), "" if seed is None else seed, saved))
            self.saves[item] = save
        if select:
            self.tree.selection_set(item)
            self.tree.see(item)
            self._selection_changed()
        return item

    def select_save(self, name):
        for item, save in self.saves.items():
            if name in (save.name, str(save.path), str(save.directory)):
                self.tabs.select(self.saved_tab)
                self.tree.selection_set(item)
                self.tree.see(item)
                self._selection_changed()
                return True
        return False

    def selected_save(self):
        selection = self.tree.selection()
        return self.saves.get(selection[0]) if selection else None

    def _selection_changed(self):
        save = self.selected_save()
        recorded = save and launch_game.resume_scenario(save, self.available)
        if save and not recorded:
            self.origin_box.configure(state="readonly")
            if self.origin.get() not in self.available:
                self.origin.set("")
        else:
            self.origin_box.configure(state="disabled")
            self.origin.set(recorded.name if recorded else "")

    def browse(self):
        path = self.ask_save_file()
        if not path:
            return
        try:
            self.add_save(launch_game.resolve_save(path, self.saves_dir))
            self.tabs.select(self.saved_tab)
            self.set_status("")
        except LaunchError as error:
            self.set_status(str(error), error=True)

    def _ask_save_file(self):
        self.saves_dir.mkdir(parents=True, exist_ok=True)
        return filedialog.askopenfilename(parent=self.root, title="Choose a saved game",
                                          initialdir=self.saves_dir, filetypes=SAVE_FILETYPES)

    # Choices.

    def mode(self):
        return SAVED if self.tabs.select() == str(self.saved_tab) else NEW

    def random_seed(self):
        self.seed.set(str(random.randrange(2**32)))

    def choices(self):
        """The launch choices the window shows; raises LaunchError for invalid ones."""
        client, spectator = self.client.get(), self.spectator.get()
        if self.mode() == NEW:
            if self.scenario.get() not in self.available:
                raise LaunchError("Choose a scenario")
            seed = launch_game.parse_seed(self.seed.get())
            save = launch_game.new_save(self.name.get(), self.saves_dir)
            choices = Choices(client, spectator, save, True, self.available[self.scenario.get()], seed)
        else:
            save = self.selected_save()
            if save is None:
                raise LaunchError("Choose a saved game, or browse for a save file")
            if not save.path.is_file():
                raise LaunchError(f"The save {save.path} no longer exists")
            scenario = launch_game.resume_scenario(save, self.available)
            if scenario is None:
                if self.origin.get() not in self.available:
                    raise LaunchError("This save has no launch record; choose the scenario it started from")
                scenario = self.available[self.origin.get()]
            choices = Choices(client, spectator, save.path, False, scenario, None)
        launch_game.check_platform(choices)
        return choices

    # Running games.

    def play(self):
        if self.running:
            return
        try:
            choices = self.choices()
        except LaunchError as error:
            self.set_status(str(error), error=True)
            return
        self.running = True
        self.session = launch_game.Session()
        self._set_inputs_enabled(False)
        verb = "Starting" if choices.new else "Resuming"
        self.set_status(f"{verb} {choices.save.parent.name}…")
        self.start(choices, self.session)

    def _start_thread(self, choices, session):
        def run():
            try:
                launch_game.launch(choices, self.bin_dir, report_frames=self.report_frames, session=session,
                                   text_window=launch_game.WINDOWS,
                                   write=lambda text: self.events.put(("message", text)))
                self.events.put(("finished", choices, None))
            except LaunchError as error:
                self.events.put(("finished", choices, str(error)))
            except Exception as error:  # Shown in the window rather than lost with no console.
                self.events.put(("finished", choices, f"{type(error).__name__}: {error}"))
        threading.Thread(target=run, daemon=True).start()

    def _drain_events(self):
        try:
            while not self.destroyed:
                self.handle(self.events.get_nowait())
        except queue.Empty:
            pass
        if not self.destroyed:
            self.root.after(100, self._drain_events)

    def _destroy(self):
        self.destroyed = True
        self.root.destroy()

    def handle(self, event):
        if event[0] == "message":
            self.set_status(event[1])
        elif event[0] == "finished":
            _, choices, error = event
            self.running = False
            self.session = None
            self._set_inputs_enabled(True)
            if choices.new:
                self.name.set(launch_game.default_name())
                self.random_seed()
            self.refresh_saves(select=str(choices.save) if choices.save.is_file() else None)
            if error:
                self.set_status(error, error=True)
            else:
                self.set_status(f"Game saved in {choices.save.parent}")
            if self.closing:
                self._destroy()
                return
        self.notify("finished" if event[0] == "finished" else "message")

    def end_game(self):
        if self.session:
            self.set_status("Saving and ending the game…")
            self.session.end()

    def close(self):
        if self.running:
            # Save and stop the running game first; the window closes when it has.
            self.closing = True
            self.end_game()
        else:
            self._destroy()

    def _set_inputs_enabled(self, enabled):
        state = ["!disabled"] if enabled else ["disabled"]
        widgets = [*self.client_buttons.values(), *self.spectator_buttons.values(), self.seed_entry,
                   self.random_button, self.name_entry, self.browse_button, self.play_button]
        for widget in widgets:
            widget.state(state)
        self.scenario_box.configure(state="readonly" if enabled else "disabled")
        self.tree.state(state)
        for index in range(len(self.tabs.tabs())):
            self.tabs.tab(index, state="normal" if enabled else "disabled")
        self.end_button.state(["disabled"] if enabled else ["!disabled"])
        if enabled:
            if not launch_game.WINDOWS:
                self.spectator_buttons["text"].state(["disabled"])
            self._selection_changed()
        else:
            self.origin_box.configure(state="disabled")

    def set_status(self, text, error=False):
        self.status.set(text)
        self.status_label.configure(foreground="firebrick" if error else "")
        self.error = error
        self.notify("status")

    def notify(self, reason):
        for listener in self.listeners:
            listener(reason)

    def state(self):
        """A JSON-ready summary of what the window shows (automation and tests)."""
        save = self.selected_save()
        return {
            "client": self.client.get(), "spectator": self.spectator.get(), "tab": self.mode(),
            "scenario": self.scenario.get(), "scenarios": list(self.available), "seed": self.seed.get(),
            "name": self.name.get(), "origin": self.origin.get(),
            "origin_enabled": "disabled" not in self.origin_box.state(),
            "saves": [{"name": s.name, "path": str(s.path), "scenario": s.metadata.get("scenario"),
                       "seed": s.metadata.get("seed")} for s in self.saves.values()],
            "selected": save and str(save.path), "status": self.status.get(),
            "error": getattr(self, "error", False), "running": self.running,
            "play_enabled": "disabled" not in self.play_button.state(),
            "end_enabled": "disabled" not in self.end_button.state(),
            "geometry": [self.root.winfo_rootx(), self.root.winfo_rooty(),
                         self.root.winfo_width(), self.root.winfo_height()],
        }


class Automation:
    """Process-test driver: JSON commands on stdin, JSON state reports on stdout.

    Commands act through the window's own variables and buttons:
    `{"set": {"client"|"spectator"|"scenario"|"seed"|"name"|"origin"|"tab": value}}`,
    `{"select": save name or path}`, `{"browse": path}` (the file dialog's result),
    `{"click": "play"|"end"|"random"|"browse"}`, `{"report": true}`, `{"close": true}`.
    Every command and every game start, message and finish prints a state report.
    """

    def __init__(self, window):
        self.window = window
        self.browsed = None
        self.commands = queue.Queue()
        window.ask_save_file = lambda: self.browsed
        window.listeners.append(lambda reason: self.report(reason))
        # Only the Tk thread touches widgets; the reader thread just queues lines.
        threading.Thread(target=self._read, daemon=True).start()
        window.root.after(50, self._poll)

    def _read(self):
        for line in sys.stdin:
            line = line.lstrip("\ufeff").strip()
            if line:
                try:
                    self.commands.put(json.loads(line))
                except ValueError as error:
                    self.commands.put({"invalid": f"{error}: {line[:80]}"})
        self.commands.put({"close": True})

    def _poll(self):
        try:
            while not self.window.destroyed:
                self.command(self.commands.get_nowait())
        except queue.Empty:
            pass
        if not self.window.destroyed:
            self.window.root.after(50, self._poll)

    def command(self, command):
        window = self.window
        if "invalid" in command:
            window.set_status(f"Automation: invalid command ({command['invalid']})", error=True)
        for key, value in command.get("set", {}).items():
            if key == "tab":
                window.tabs.select(window.saved_tab if value == SAVED else window.new_tab)
            else:
                getattr(window, key).set(value)
        if "select" in command:
            window.select_save(command["select"])
        if "browse" in command:
            self.browsed = command["browse"]
            window.browse_button.invoke()
        if button := command.get("click"):
            getattr(window, button + "_button").invoke()
        if command.get("close"):
            window.close()
        if not window.destroyed:
            self.report("command")

    def report(self, reason):
        print(json.dumps({"event": reason, **self.window.state()}), flush=True)


def run(args):
    if launch_game.WINDOWS:
        import ctypes
        try:  # Crisp text on scaled displays instead of a blurred bitmap.
            ctypes.windll.shcore.SetProcessDpiAwareness(1)
        except (AttributeError, OSError):
            pass
    try:
        root = tk.Tk()
    except tk.TclError as error:
        print(f"Error: the graphical launcher needs a display ({error}); use --console", file=sys.stderr)
        return 2
    window = LauncherWindow(root, bin_dir=args.bin_dir, scenarios_dir=args.scenarios_dir,
                            saves_dir=args.saves_dir, report_frames=args.report_frames,
                            initial={key: getattr(args, key) for key in
                                     ("client", "spectator", "scenario", "seed", "save", "new")})
    if args.automation:
        Automation(window).report("ready")
    root.mainloop()
    return 0
