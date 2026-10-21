"""Pin the Supervisor discovery payload published by the app runner."""

import json
import shutil
import subprocess
from pathlib import Path

import pytest

ROOT = Path(__file__).parents[1]
APP = ROOT / "addon/vistoda_blink_engine/rootfs"
TOKEN = "f" * 64


def _discovery_block() -> str:
    runner = (APP / "run.sh").read_text()
    start = runner.index("# discovery-begin")
    end = runner.index("# discovery-end")
    return runner[start:end]


@pytest.mark.skipif(shutil.which("jq") is None, reason="jq is required")
def test_discovery_payload_is_unchanged(tmp_path: Path) -> None:
    """The kit helper must publish exactly the 0.23 keys and values."""
    token_file = tmp_path / "workload-token"
    token_file.write_text(f"{TOKEN}\n")
    capture = tmp_path / "discovery.json"
    script = f"""
set -eu
. "{APP / "vistoda-app-bootstrap.sh"}"
export SUPERVISOR_TOKEN=test-token
token_file="{token_file}"
curl() {{
    case "$*" in
        *addons/self/info*) printf '%s' '{{"data":{{"hostname":"10aad50a-vistoda-blink"}}}}' ;;
        *supervisor/discovery*) cat >"{capture}" ;;
        *) return 1 ;;
    esac
}}
{_discovery_block()}
"""
    subprocess.run(["sh", "-c", script], check=True, timeout=30)
    assert json.loads(capture.read_text()) == {
        "service": "blink_live_bridge",
        "config": {
            "url": "http://10aad50a-vistoda-blink:8099",
            "token": TOKEN,
            "managed_app": True,
        },
    }
