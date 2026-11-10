"""Exercise the shipped launcher with independent, temporary bridge programs."""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
ADAPTER = ROOT / "crates/qb-extensions/src/plugins/claude_bridge_launcher.py"
ADAPTER_SOURCE = ADAPTER.read_text(encoding="utf-8")
PRIVACY = "cc-plugin-privacy@builtin"

# This small protocol fixture is authored for the launcher. No external bridge
# implementation, user configuration, or private conversation is copied here.
FAKE_BRIDGE = '''\
import argparse
from dataclasses import dataclass
import json
from pathlib import Path
import sys
from bridge_helper import HELPER_VALUE

BUILTIN_PLUGINS_DISABLED_FOR_RUN = PLUGIN_LIST

@dataclass
class Sample:
    value: str

def plugins_disabled_for_run(profile):
    return dict.fromkeys(BUILTIN_PLUGINS_DISABLED_FOR_RUN, False)

def main(arguments):
    parser = argparse.ArgumentParser()
    parser.add_argument("--claude", required=True)
    parser.add_argument("--data-dir", required=True)
    parser.add_argument("--port", required=True)
    parser.add_argument("--unknown-plugin")
    args = parser.parse_args(arguments)
    settings = json.loads((Path(args.data_dir) / "settings.json").read_text())
    if args.unknown_plugin:
        print(json.dumps({"error": "runtime_policy_mismatch", "plugin": args.unknown_plugin}))
        return 31
    cli_settings = {"enabledPlugins": plugins_disabled_for_run(Path(args.data_dir))}
    sdk_settings = {"enabledPlugins": plugins_disabled_for_run(Path(args.data_dir))}
    print(json.dumps({
        "cli": cli_settings,
        "sdk": sdk_settings,
        "plugins": list(BUILTIN_PLUGINS_DISABLED_FOR_RUN),
        "container": type(BUILTIN_PLUGINS_DISABLED_FOR_RUN).__name__,
        "arguments": arguments,
        "argv": sys.argv,
        "file": __file__,
        "helper": Sample(HELPER_VALUE).value,
        "model": settings["model"],
    }))
    return 0
'''


class BridgeLauncherTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="qb-bridge-launcher-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.bridge_root = self.root / "bridge 中文 with space and ' quote"
        self.bridge_root.mkdir()
        self.bridge = self.bridge_root / "bridge.py"
        self.helper = self.bridge_root / "bridge_helper.py"
        self.helper.write_text("HELPER_VALUE = 'sibling import works'\n", encoding="utf-8")
        self.data = self.root / "profile 中文 ' quoted"
        self.data.mkdir()
        self.settings = self.data / "settings.json"
        self.settings.write_text(json.dumps({"model": "user-selected-model", "enabledPlugins": {PRIVACY: True}}), encoding="utf-8")
        self.args = ["--claude", str(self.root / "CLI ' 中文" / "claude.exe"), "--data-dir", str(self.data), "--port", "5123"]

    def fixture(self, plugins):
        self.bridge.write_text(FAKE_BRIDGE.replace("PLUGIN_LIST", repr(plugins)), encoding="utf-8")

    def run_adapter(self, *extra, check=False, handshake=True):
        arguments = [*(["--check"] if check else []), str(self.bridge), *self.args, *extra]
        return subprocess.run(
            [sys.executable, "-u", "-B", "-c", ADAPTER_SOURCE, *arguments],
            input="QB_GATE_BRIDGE_CHECK\n" if check and handshake else "",
            capture_output=True,
            text=True,
            encoding="utf-8",
            env={**os.environ, "PYTHONIOENCODING": "utf-8"},
            cwd=self.root,
            timeout=15,
            check=False,
        )

    def fingerprints(self):
        return {p: hashlib.sha256(p.read_bytes()).hexdigest() for p in [self.bridge, self.helper, self.settings]}

    def test_old_bridge_gets_privacy_only_for_cli_and_sdk_invocations(self):
        self.fixture(("existing-plugin@builtin",))
        before = self.fingerprints()
        result = self.run_adapter()
        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(result.stdout)
        expected = {"existing-plugin@builtin": False, PRIVACY: False}
        self.assertEqual(payload["cli"]["enabledPlugins"], expected)
        self.assertEqual(payload["sdk"]["enabledPlugins"], expected)
        self.assertEqual(payload["plugins"], ["existing-plugin@builtin", PRIVACY])
        self.assertEqual(payload["model"], "user-selected-model")
        self.assertEqual(self.fingerprints(), before)
        self.assertFalse((self.bridge_root / "__pycache__").exists())

    def test_already_fixed_tuple_and_list_keep_their_contents_and_type(self):
        for plugins in [(PRIVACY, "keep@builtin"), ["keep@builtin", PRIVACY]]:
            with self.subTest(container=type(plugins).__name__):
                self.fixture(plugins)
                result = self.run_adapter()
                self.assertEqual(result.returncode, 0, result.stderr)
                payload = json.loads(result.stdout)
                self.assertEqual(payload["plugins"], list(plugins))
                self.assertEqual(payload["container"], type(plugins).__name__)

    def test_argv_file_sibling_imports_and_dataclasses_survive_special_paths(self):
        self.fixture([])
        result = self.run_adapter()
        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(result.stdout)
        self.assertEqual(payload["arguments"], self.args)
        self.assertEqual(payload["argv"], [str(self.bridge.resolve()), *self.args])
        self.assertEqual(payload["file"], str(self.bridge.resolve()))
        self.assertEqual(payload["helper"], "sibling import works")

    def test_unknown_plugin_rejection_is_still_the_bridge_decision(self):
        self.fixture(())
        result = self.run_adapter("--unknown-plugin", "future-unreviewed@builtin")
        self.assertEqual(result.returncode, 31)
        self.assertEqual(json.loads(result.stdout), {"error": "runtime_policy_mismatch", "plugin": "future-unreviewed@builtin"})

    def test_check_loads_capabilities_without_calling_main_or_changing_files(self):
        self.fixture(("existing@builtin",))
        before = self.fingerprints()
        # main prints a payload; checking must stop after loading capabilities.
        result = self.run_adapter(check=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "")
        self.assertEqual(self.fingerprints(), before)

    def test_unsupported_capabilities_have_a_distinct_exit_code(self):
        variants = [
            "BUILTIN_PLUGINS_DISABLED_FOR_RUN = {}\ndef main(argv): return 0\ndef plugins_disabled_for_run(profile): return {}\n",
            "BUILTIN_PLUGINS_DISABLED_FOR_RUN = [7]\ndef main(argv): return 0\ndef plugins_disabled_for_run(profile): return {}\n",
            "BUILTIN_PLUGINS_DISABLED_FOR_RUN = []\ndef main(): return 0\ndef plugins_disabled_for_run(profile): return {}\n",
            "BUILTIN_PLUGINS_DISABLED_FOR_RUN = []\ndef main(argv): return 0\n",
        ]
        for source in variants:
            with self.subTest(source=source):
                self.bridge.write_text(source, encoding="utf-8")
                result = self.run_adapter(check=True)
                self.assertEqual(result.returncode, 21)
                self.assertNotIn("Traceback", result.stderr)

    def test_missing_or_unloadable_bridges_are_distinguished(self):
        self.assertEqual(self.run_adapter(check=True).returncode, 20)
        self.bridge.write_text("raise RuntimeError('must-not-be-echoed')\n", encoding="utf-8")
        result = self.run_adapter(check=True)
        self.assertEqual(result.returncode, 22)
        self.assertNotIn("must-not-be-echoed", result.stdout + result.stderr)

    def test_missing_resource_during_import_is_not_reported_as_missing_bridge(self):
        self.bridge.write_text("raise FileNotFoundError('private-resource-name')\n", encoding="utf-8")
        result = self.run_adapter(check=True)
        self.assertEqual(result.returncode, 22)
        self.assertNotIn("private-resource-name", result.stdout + result.stderr)

    def test_probe_does_not_load_external_module_without_parent_handshake(self):
        marker = self.root / "imported.txt"
        self.bridge.write_text(f"from pathlib import Path\nPath({str(marker)!r}).write_text('imported')\n", encoding="utf-8")
        result = self.run_adapter(check=True, handshake=False)
        self.assertEqual(result.returncode, 22)
        self.assertFalse(marker.exists())


if __name__ == "__main__":
    unittest.main()
