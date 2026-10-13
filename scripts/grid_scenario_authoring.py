"""Deterministic authoring helpers shared by grid scenarios and test fixtures."""
from pathlib import Path


def manifest_lines(scenario_id: str) -> list[str]:
    manifest = [
        'format = 7', f'id = "{scenario_id}"', 'version = "1.0"',
        'ruleset = "rules.toml"', 'default_character = 1',
        '[[characters]]', 'id = 1',
        'anchor = "1/start"', 'turn_ticks = 100',
        '[generation_recipes.exploration]', 'version = 2',
    ]
    for identity, kind, parameters in [
        ("stone", "stone_fill", []), ("slots", "grid_partition", []),
        ("rooms", "rooms", ['width = [4, 24]', 'height = [3, 5]']),
        ("connections", "connected_graph", ['extra = [0, 2]']),
        ("corridors", "corridors", []), ("stairs", "stairs", []),
        ("lighting", "room_lighting", ["darkness_roll = 10", "darkness_start = 1"]),
    ]:
        manifest += ['[[generation_recipes.exploration.stages]]',
                     f'id = "{identity}"', 'version = 1',
                     '[generation_recipes.exploration.stages.operation]',
                     f'kind = "{kind}"', *parameters]
    return manifest


def write_rules(root: Path) -> None:
    root.mkdir(parents=True, exist_ok=True)
    (root / "rules.toml").write_text('import = "interactions-v31"\n', encoding="utf-8", newline="\n")


def build_explicit(root: Path, scenario_id: str, floors: int) -> None:
    if floors < 1:
        raise ValueError("At least one explicit floor is required")
    write_rules(root)
    regions = root / "regions"
    regions.mkdir(parents=True, exist_ok=True)
    manifest = manifest_lines(scenario_id)
    for depth in range(1, floors + 1):
        first = (depth - 1) * 9 + 1
        group = f"floor-{depth}"
        members = list(range(first, first + 9))
        manifest += [f'[generation_groups."{group}"]', f'depth = {depth}',
                     'recipe = "exploration"', f'members = {members}']
        if depth < floors:
            manifest += [f'[stair_pairs."descent-{depth}"]',
                         f'upper = "{first + 8}/down"', f'lower = "{first + 9}/up"']
        for slot, region in enumerate(members):
            col, row = slot % 3, slot // 3
            anchors = dict(west=[0, 0, 0], east=[25, 0, 0],
                           north=[0, 0, 0], south=[0, 6, 0])
            if region == 1:
                anchors["start"] = [13, 3, 0]
            text = [f'id = {region}', f'name = "Depth {depth}, slot {slot + 1}"',
                    'size = [26, 7, 2]', 'chamber = true',
                    'anchors = { ' + ', '.join(f'{k} = {v}' for k, v in anchors.items()) + ' }']
            neighbors = []
            if col:
                neighbors.append(("west", region - 1, "east", 7))
            if col < 2:
                neighbors.append(("east", region + 1, "west", 7))
            if row:
                neighbors.append(("north", region - 3, "south", 26))
            if row < 2:
                neighbors.append(("south", region + 3, "north", 26))
            for direction, target, endpoint, width in neighbors:
                text += ['[[portals]]', 'kind = "boundary"',
                         f'at = {anchors[direction]}', f'direction = "{direction}"',
                         f'to = "{target}/{endpoint}"', f'width = {width}', 'height = 2']
            stairs = []
            if slot == 0 and depth > 1:
                stairs.append("up")
            if slot == 8 and depth < floors:
                stairs.append("down")
            text += ['[generate]', 'generator = "group"', 'version = 1',
                     f'group = "{group}"', 'rooms = [1, 1]',
                     'boundary_anchors = ["west", "east", "north", "south"]',
                     'stair_anchors = [' + ', '.join(f'"{s}"' for s in stairs) + ']']
            (regions / f"{region}.toml").write_text("\n".join(text) + "\n", encoding="utf-8", newline="\n")
    (root / "scenario.toml").write_text("\n".join(manifest) + "\n", encoding="utf-8", newline="\n")


def build_compact(root: Path, domain: str) -> None:
    write_rules(root)
    source = "\n".join(manifest_lines("compact-grid-world")).replace(
        'anchor = "1/start"', 'anchor = "caves/1/start"') + "\n"
    source += ('\n[procedural_worlds.caves]\nkind = "grid_chain"\nversion = 1\nbase_region = 1\n'
               'grid = [2, 2]\nsize = [26, 7, 2]\nrecipe = "exploration"\n'
               '[procedural_worlds.caves.domain]\n' + domain + '\n')
    (root / "scenario.toml").write_text(source, encoding="utf-8", newline="\n")
