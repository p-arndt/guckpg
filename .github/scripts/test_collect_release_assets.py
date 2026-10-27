import importlib.util
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("collect", Path(__file__).with_name("collect-release-assets.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class CollectReleaseAssetsTest(unittest.TestCase):
    def test_each_target_collects_distinct_expected_files(self):
        targets = {
            "aarch64-apple-darwin": ("guckpg.app.tar.gz", "guckpg_1.2.3_aarch64.dmg", "guckpg_1.2.3_aarch64.app.tar.gz"),
            "x86_64-apple-darwin": ("guckpg.app.tar.gz", "guckpg_1.2.3_x64.dmg", "guckpg_1.2.3_x64.app.tar.gz"),
            "x86_64-pc-windows-msvc": ("guckpg_1.2.3_x64-setup.exe", "guckpg_1.2.3_x64_en-US.msi", "guckpg_1.2.3_x64-setup.exe"),
        }
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for target, (first, second, expected) in targets.items():
                bundle = root / target / "bundle"
                bundle.mkdir(parents=True)
                (bundle / first).write_text("first")
                (bundle / second).write_text("second")
                output = root / target / "output"
                module.collect(bundle, output, target, "1.2.3")
                self.assertEqual((output / expected).read_text(), "first")
                self.assertEqual(len(list(output.iterdir())), 2)

    def test_missing_or_duplicate_bundle_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "one.dmg").touch()
            with self.assertRaises(ValueError):
                module.collect(root, root / "output", "aarch64-apple-darwin", "1.2.3")
            (root / "one.app.tar.gz").touch()
            (root / "two.app.tar.gz").touch()
            with self.assertRaises(ValueError):
                module.collect(root, root / "output", "aarch64-apple-darwin", "1.2.3")


if __name__ == "__main__":
    unittest.main()
