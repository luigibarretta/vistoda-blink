"""Prevent mutable image publication and treating transport errors as absence."""

import io
import json
import runpy
import urllib.error
from pathlib import Path
from unittest.mock import patch

import pytest

SCRIPT = Path(__file__).parents[1] / "scripts/assert-image-absent.py"


@pytest.mark.parametrize("status", [200, 401, 403, 404, 503])
def test_only_a_registry_404_allows_a_new_image(status, monkeypatch):
    monkeypatch.setenv("GITHUB_ACTOR", "fixture")
    monkeypatch.setenv("GH_TOKEN", "fixture-token")
    token = io.BytesIO(json.dumps({"token": "fixture-bearer"}).encode())
    response = (
        io.BytesIO()
        if status == 200
        else urllib.error.HTTPError(
            "https://ghcr.io/v2/example/provider/manifests/1.2.3", status, "fixture", {}, None
        )
    )
    with (
        patch("sys.argv", [str(SCRIPT), "ghcr.io/example/provider", "1.2.3"]),
        patch("urllib.request.urlopen", side_effect=[token, response]) as get,
    ):
        if status == 404:
            runpy.run_path(str(SCRIPT))
        else:
            with pytest.raises(SystemExit):
                runpy.run_path(str(SCRIPT))
    request = get.call_args.args[0]
    assert request.get_method() == "HEAD"
    assert "application/vnd.oci.image.manifest.v1+json" in request.get_header("Accept")


def test_network_timeout_cannot_allow_publication(monkeypatch):
    monkeypatch.setenv("GITHUB_ACTOR", "fixture")
    monkeypatch.setenv("GH_TOKEN", "fixture-token")
    with (
        patch("sys.argv", [str(SCRIPT), "ghcr.io/example/provider", "1.2.3"]),
        patch("urllib.request.urlopen", side_effect=TimeoutError()),
        pytest.raises(TimeoutError),
    ):
        runpy.run_path(str(SCRIPT))
