"""Model generation and art review tests run on the developer's machine.

They build and animate procedural bodies and capture what a real GPU presents.
GitHub's runners have no GPU, and there these tests run 10 to 17 times slower
than locally, so the maintainer chose not to gate CI on model generation.
CI still runs every gameplay, launch and input-rejection process test.
Set TOR_3D_MODEL_TESTS=1 to run these on CI anyway.
"""
import os
import unittest


def enabled():
    return not os.environ.get("CI") or os.environ.get("TOR_3D_MODEL_TESTS") == "1"


def local(test):
    """Skip a model generation test on CI unless TOR_3D_MODEL_TESTS=1."""
    return unittest.skipUnless(enabled(), "model generation is verified locally, not on CI")(test)
