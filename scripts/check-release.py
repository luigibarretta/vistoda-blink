#!/usr/bin/env python3
"""Keep every provider package identity aligned before signing a release."""

import json
import re
import tomllib
from pathlib import Path

root = Path(__file__).resolve().parents[1]
blink = (root / "addon/vistoda_blink_engine/Cargo.toml").is_file()
cargo = root / ("addon/vistoda_blink_engine/Cargo.toml" if blink else "Cargo.toml")
version = tomllib.loads(cargo.read_text())["package"]["version"]
if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version):
    raise SystemExit("Provider version must be exact SemVer")
checks = []
if blink:
    checks.extend(
        [
            (
                "pyproject.toml",
                tomllib.loads((root / "pyproject.toml").read_text())["project"]["version"],
            ),
            (
                "manifest.json",
                json.loads(
                    (root / "custom_components/blink_live_bridge/manifest.json").read_text()
                )["version"],
            ),
            (
                "app config",
                re.search(
                    r"^version: (\S+)$",
                    (root / "addon/vistoda_blink_engine/config.yaml").read_text(),
                    re.MULTILINE,
                ).group(1),
            ),
        ]
    )
else:
    for path, argument in [
        ("Dockerfile", "VERSION"),
        ("packaging/home-assistant/Dockerfile", "BUILD_VERSION"),
    ]:
        match = re.search(rf"^ARG {argument}=(\S+)$", (root / path).read_text(), re.MULTILINE)
        checks.append((path, match.group(1) if match else None))
for label, actual in checks:
    if actual != version:
        raise SystemExit(f"{label}: version {actual!r} differs from Cargo {version}")
print(f"Provider release identity verified: {version}")
