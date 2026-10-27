"""Real CPU-only construction producer; maintained in the independent 3D gate."""
import json
import os
from pathlib import Path
import sys

CLIENT = Path(__file__).resolve().parents[1]
ROOT = CLIENT.parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from process_harness import ProcessTestCase
from model_performance_report import validate
import model_generation


class ModelBenchmarkProcesses(ProcessTestCase):
    @model_generation.local
    def test_construction_producer_emits_complete_repeatable_samples(self):
        client = Path(os.environ.get("TOR_3D_TEST_BINARY", CLIENT / "target/debug/tor-client-3d"))
        executable = client.parent / "examples" / ("model_bench" + self.suffix)
        process = self.launch("model_bench", [], executable=executable,
                              extra_env={"TOR_3D_PROFILE_BUILD": "1"})
        self.assertEqual(process.child.wait(timeout=120), 0,
                         process.stdout_path.read_text(encoding="utf-8")[-2000:])
        rows = [json.loads(line) for line in process.stdout_path.read_text(encoding="utf-8").splitlines() if line.strip()]
        self.assertEqual(len(validate(rows)), 3)
        invalid = self.launch("model_bench", ["--unversioned-workload"], executable=executable)
        self.assertNotEqual(invalid.child.wait(timeout=30), 0)
