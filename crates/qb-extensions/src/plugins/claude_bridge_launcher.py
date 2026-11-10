# QB Gate — Copyright (C) 2026 smithtaylor7748-ops
# Licensed under AGPL-3.0-only with the additional terms permitted by its section 7.
"""Launch a user's Claude bridge with QB's invocation-only compatibility setting."""

from __future__ import annotations

import importlib.util
import inspect
import sys
from pathlib import Path
from types import ModuleType


PRIVACY_PLUGIN = "cc-plugin-privacy@builtin"
CHECK_SIGNAL = "QB_GATE_BRIDGE_CHECK"
CHECK_MISSING = 20
CHECK_INCOMPATIBLE = 21
CHECK_LOAD_FAILED = 22
MODULE_NAME = "_qbgate_external_claude_bridge"


class IncompatibleBridge(Exception):
    pass


def load_bridge(path: Path, arguments: list[str]) -> ModuleType:
    path = path.resolve(strict=True)
    if not path.is_file():
        raise FileNotFoundError
    # Match direct script execution: sibling imports, __file__, and argv[0]
    # continue to refer to the user's script. Do not write bytecode beside it.
    sys.dont_write_bytecode = True
    sys.path.insert(0, str(path.parent))
    sys.argv = [str(path), *arguments]
    spec = importlib.util.spec_from_file_location(MODULE_NAME, path)
    if spec is None or spec.loader is None:
        raise IncompatibleBridge
    module = importlib.util.module_from_spec(spec)
    # dataclasses and other import-time introspection require this registration.
    sys.modules[MODULE_NAME] = module
    spec.loader.exec_module(module)
    return module


def apply_compatibility(module: ModuleType, arguments: list[str]) -> None:
    plugins = getattr(module, "BUILTIN_PLUGINS_DISABLED_FOR_RUN", None)
    entry = getattr(module, "main", None)
    disabled_for_run = getattr(module, "plugins_disabled_for_run", None)
    if (
        not isinstance(plugins, (tuple, list))
        or any(not isinstance(item, str) or not item for item in plugins)
        or not callable(entry)
        or inspect.iscoroutinefunction(entry)
        or not callable(disabled_for_run)
    ):
        raise IncompatibleBridge
    try:
        inspect.signature(entry).bind(arguments)
        inspect.signature(disabled_for_run).bind(Path("unused-profile"))
    except (TypeError, ValueError) as exc:
        raise IncompatibleBridge from exc
    if PRIVACY_PLUGIN not in plugins:
        module.BUILTIN_PLUGINS_DISABLED_FOR_RUN = (
            (*plugins, PRIVACY_PLUGIN)
            if isinstance(plugins, tuple)
            else [*plugins, PRIVACY_PLUGIN]
        )


def main(arguments: list[str] | None = None) -> int:
    arguments = list(sys.argv[1:] if arguments is None else arguments)
    checking = bool(arguments and arguments[0] == "--check")
    if checking:
        arguments.pop(0)
        # The parent sends this only after assigning our process to its Job.
        # An interrupted parent closes stdin; no external module runs then.
        if sys.stdin.readline().rstrip("\r\n") != CHECK_SIGNAL:
            return CHECK_LOAD_FAILED
    if not arguments:
        return CHECK_MISSING
    path, bridge_arguments = Path(arguments[0]), arguments[1:]
    if not path.is_file():
        return CHECK_MISSING
    try:
        module = load_bridge(path, bridge_arguments)
        apply_compatibility(module, bridge_arguments)
    except IncompatibleBridge:
        return CHECK_INCOMPATIBLE
    except (Exception, SystemExit):
        # The panel maps exit codes to fixed messages; never relay exceptions
        # from user modules, which may contain configuration or credentials.
        return CHECK_LOAD_FAILED
    if checking:
        return 0
    return module.main(bridge_arguments)


if __name__ == "__main__":
    raise SystemExit(main())
