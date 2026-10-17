"""Run with: python3 -m unittest discover -s tests -p 'test_*.py'."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


class InstallConfigTest(unittest.TestCase):
    def test_upgrade_preserves_personal_configs_and_permissions(self):
        self.check_install(upgrade=True)

    def test_fresh_install_includes_templates_without_creating_personal_config(self):
        self.check_install(upgrade=False)

    def check_install(self, upgrade):
        # Exercise the real install function without downloading a release.
        script = (Path(__file__).resolve().parents[1] / "install.sh").read_text()
        definitions = script.split('\nif [[ -z "$TARGET" ]]; then', 1)[0]
        with tempfile.TemporaryDirectory(prefix="benchmark-install-test-") as temp:
            root = Path(temp)
            package = root / "package"
            package.mkdir()
            (package / ".grpc-benchmark-install").touch()
            (package / "binary").write_text("new version")
            (package / "configs").mkdir()
            (package / "configs" / "example").write_text("template")
            install = root / "installed"
            if upgrade:
                install.mkdir()
                (install / ".grpc-benchmark-install").touch()
                for name in (".env", "shred-comparison.env"):
                    config = install / name
                    config.write_text("PRIVATE_SETTING=personal-value\n")
                    config.chmod(0o600)
                (install / "binary").write_text("old version")
            env = dict(os.environ, INSTALL_DIR=str(install))
            subprocess.run(
                ["bash", "-c", definitions + '\ninstall_package "$1"', "test", str(package)],
                env=env, check=True, capture_output=True,
            )
            self.assertEqual((install / "binary").read_text(), "new version")
            self.assertTrue((install / "configs" / "example").is_file())
            for name in (".env", "shred-comparison.env"):
                config = install / name
                self.assertEqual(config.exists(), upgrade)
                if upgrade:
                    self.assertEqual(config.read_text(), "PRIVATE_SETTING=personal-value\n")
                    self.assertEqual(config.stat().st_mode & 0o777, 0o600)


if __name__ == "__main__":
    unittest.main()
