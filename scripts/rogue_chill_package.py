"""New reconstruction: exact production ice recipe in a controlled encounter."""
from rogue_roster_package import roster_package

ICE = "rogue.ice_monster"
CHILL = "rogue.freezing_touch"
FROZEN = "rogue.frozen"


def chill_package(test, *, character=2, autonomous=False):
    return roster_package(test, "ice_monster", character=character, autonomous=autonomous)
