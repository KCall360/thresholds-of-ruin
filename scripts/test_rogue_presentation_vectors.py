"""Independent source-tuple/unsigned selector oracle; never emits recorded artifacts."""
import unittest
from pathlib import Path
import tomllib

PRODUCTION = Path(__file__).resolve().parents[1] / "scenarios/rogue-exploration"

MASK = (1 << 64) - 1

def select(seed, observer, subject, channel, phase, count):
    value = 0x6170706561727631
    for operand in (seed, observer, subject, channel, phase):
        value = ((value ^ operand) + 0x9e3779b97f4a7c15) & MASK
        value = ((value ^ (value >> 30)) * 0xbf58476d1ce4e5b9) & MASK
        value = ((value ^ (value >> 27)) * 0x94d049bb133111eb) & MASK
        value ^= value >> 31
    return value % count

def rosters(root):
    catalog = tomllib.loads((root / 'catalog/items.toml').read_text())
    actors = tomllib.loads((root / 'catalog/actors.toml').read_text())['actors']
    creatures = sorted({(actor['creature']['name'], actor.get('asset')) for actor in actors.values()})
    objects = {('', None, 'stairs')}
    pools = {}
    for item in catalog['items'].values():
        # A seeded physical form changes the run's item, not the complete cosmetic
        # roster. Include every declared form pool once, never only one seed.
        variants = item.get('forms') or [item]
        for form in variants:
            if 'appearance_pool' in form:
                key, kind = form['appearance_pool'], item.get('class', 'misc')
                assert key not in pools or pools[key] == kind
                pools[key] = kind
            elif 'name' in form:
                objects.add((form['name'], form.get('asset'), item.get('class', 'misc')))
    for key, kind in pools.items():
        pool = catalog['appearance_pools'][key]
        objects.update((name, pool.get('asset'), kind) for name in pool['appearances'])
    objects = sorted(objects, key=lambda row: (row[0], row[1] is not None, row[1] or '', row[2]))
    # Name/asset tuples are unique here, so Rust enum discriminants cannot affect order.
    assert len({(name, asset) for name, asset, _ in objects}) == len(objects)
    return creatures, objects

def vectors(root):
    creatures, objects = rosters(root)
    return {'creatures': len(creatures), 'objects': len(objects), 'phases': [{
        'phase': phase,
        'visual': creatures[select(42, 1, 2, 1, phase, len(creatures))],
        'object': objects[select(42, 1, 40, 2, phase, len(objects))],
        'monster_sense': creatures[select(42, 1, 2, 4, phase, len(creatures))],
    } for phase in (0, 1)]}

class RoguePresentationVectors(unittest.TestCase):
    def test_unsigned_recipe_has_independent_fixed_reference_vectors(self):
        self.assertEqual(select(42, 1, 2, 1, 0, 257), 183)
        self.assertEqual(select(42, 1, 2, 2, 0, 257), 63)

    def test_complete_authored_rosters_keep_visual_and_monster_sense_channels_distinct(self):
        self.assertEqual(vectors(PRODUCTION), {
            'creatures': 26, 'objects': 98, 'phases': [
                {'phase': 0, 'visual': ('emu', 'creature.bird.emu'),
                 'object': ('gold wand', 'item.wand', 'wand'),
                 'monster_sense': ('snake', 'creature.serpent.snake')},
                {'phase': 1, 'visual': ('bat', 'creature.bat'),
                 'object': ('cedar staff', 'item.wand', 'wand'),
                 'monster_sense': ('leprechaun', 'creature.fey.leprechaun')} ]})

if __name__ == '__main__':
    unittest.main()
