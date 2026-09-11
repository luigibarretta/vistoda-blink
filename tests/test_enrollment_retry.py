"""Exercise the actual flow methods without requiring a running HA install."""

import ast
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock

import pytest


class EnrollmentError(Exception):
    """A consumed or otherwise unusable provider challenge."""


def flow_class():
    path = Path(__file__).parents[1] / "custom_components/blink_live_bridge/config_flow.py"
    tree = ast.parse(path.read_text())
    flow = next(node for node in tree.body if isinstance(node, ast.ClassDef))
    flow.bases = []
    flow.keywords = []
    module = ast.Module(
        body=[
            ast.ImportFrom(module="__future__", names=[ast.alias(name="annotations")], level=0),
            flow,
        ],
        type_ignores=[],
    )
    namespace = {
        "EngineError": EnrollmentError,
        "vol": SimpleNamespace(Schema=lambda data: data, Required=lambda name: name),
    }
    exec(compile(ast.fix_missing_locations(module), str(path), "exec"), namespace)
    return namespace["ConfigFlow"]


@pytest.mark.parametrize("reauth", [False, True])
async def test_failed_code_requires_fresh_challenge_and_preserves_reauth(reauth):
    flow = flow_class()()
    flow.context = {"entry_id": "existing-entry"} if reauth else {}
    flow.async_show_form = lambda **kwargs: kwargs
    client = SimpleNamespace(
        post=AsyncMock(side_effect=[EnrollmentError(), {"enrollment_id": "fresh"}, {}]),
        get_json=AsyncMock(),
    )
    flow._client = client
    flow._enrollment_id = "consumed"
    failed = await flow.async_step_two_factor({"code": "000000"})
    assert failed["step_id"] == ("reauth_confirm" if reauth else "user")
    assert failed["errors"] == {"base": "two_factor_restart"}
    assert flow._enrollment_id is None
    client.get_json.assert_not_awaited()

    fresh = await flow._credentials_form(
        {"username": "user@example.invalid", "password": "ephemeral"}, reauth=reauth
    )
    assert fresh["step_id"] == "two_factor"
    assert flow._enrollment_id == "fresh"
    assert "password" not in flow._entry_data
    flow._finish = lambda: "completed"
    assert await flow.async_step_two_factor({"code": "123456"}) == "completed"
    assert [call.args[0] for call in client.post.await_args_list] == [
        "/v1/enrollment/consumed/complete",
        "/v1/enrollment/start",
        "/v1/enrollment/fresh/complete",
    ]
