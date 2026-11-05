"""Behavior of the bridge HTTP authentication and the camera stream URL."""

import ast
import base64
import sys
from pathlib import Path
from types import ModuleType, SimpleNamespace
from unittest.mock import MagicMock

import pytest
from aiohttp import web
from aiohttp.test_utils import make_mocked_request

COMPONENT = Path(__file__).parents[1] / "custom_components/blink_live_bridge"
KEY_AUTHENTICATED = "ha_authenticated"
TOKEN = "a" * 64


@pytest.fixture(scope="module")
def auth():
    http = ModuleType("homeassistant.components.http")
    http.KEY_AUTHENTICATED = KEY_AUTHENTICATED
    modules = {
        "homeassistant": ModuleType("homeassistant"),
        "homeassistant.components": ModuleType("homeassistant.components"),
        "homeassistant.components.http": http,
    }
    saved = {name: sys.modules.get(name) for name in modules}
    sys.modules.update(modules)
    try:
        namespace: dict = {}
        source = (COMPONENT / "auth.py").read_text()
        exec(compile(source, str(COMPONENT / "auth.py"), "exec"), namespace)
        yield SimpleNamespace(**namespace)
    finally:
        for name, module in saved.items():
            if module is None:
                sys.modules.pop(name, None)
            else:
                sys.modules[name] = module


def request(path="/live.ts", headers=None, peer=("127.0.0.1", 50000), authenticated=False):
    transport = MagicMock()
    transport.get_extra_info.side_effect = lambda name, default=None: (
        peer if name == "peername" else default
    )
    req = make_mocked_request("GET", path, headers=headers or {}, transport=transport)
    req[KEY_AUTHENTICATED] = authenticated
    return req


@pytest.mark.parametrize("peer", [("127.0.0.1", 1), ("::1", 1, 0, 0), ("192.0.2.4", 1)])
def test_no_credential_is_rejected_even_from_loopback(auth, peer) -> None:
    with pytest.raises(web.HTTPUnauthorized):
        auth.require_bridge_auth(request(peer=peer), TOKEN, "stream-secret")


def test_home_assistant_authentication_is_accepted(auth) -> None:
    auth.require_bridge_auth(request(authenticated=True), TOKEN)


@pytest.mark.parametrize(
    "header",
    [f"Bearer {TOKEN}", "Basic " + base64.b64encode(f"scenetrove:{TOKEN}".encode()).decode()],
)
def test_bridge_token_is_accepted(auth, header) -> None:
    auth.require_bridge_auth(request(headers={"Authorization": header}), TOKEN)


@pytest.mark.parametrize(
    "header",
    [
        "Bearer wrong",
        "Basic !!!",
        "Token " + TOKEN,
        "Bearer ",
        "Bearer \u00e9",
        "Basic " + base64.b64encode("u:\u00e9".encode()).decode(),
    ],
)
def test_wrong_bridge_token_is_rejected(auth, header) -> None:
    with pytest.raises(web.HTTPUnauthorized):
        auth.require_bridge_auth(request(headers={"Authorization": header}), TOKEN)


def test_stream_token_is_accepted_only_for_its_camera(auth) -> None:
    path = f"/live.ts?{auth.STREAM_TOKEN_PARAM}=stream-secret"
    auth.require_bridge_auth(request(path), TOKEN, "stream-secret")
    with pytest.raises(web.HTTPUnauthorized):
        auth.require_bridge_auth(request("/live.ts?auth=%C3%A9"), TOKEN, "stream-secret")
    for other in (None, "", "other-camera-secret"):
        with pytest.raises(web.HTTPUnauthorized):
            auth.require_bridge_auth(request(path), TOKEN, other)


def test_stream_token_param_is_redacted_by_home_assistant(auth) -> None:
    # homeassistant.components.stream.redact_credentials masks "auth".
    assert auth.STREAM_TOKEN_PARAM == "auth"


def test_bridge_views_have_no_peer_address_bypass() -> None:
    source = (COMPONENT / "auth.py").read_text() + (COMPONENT / "http.py").read_text()
    assert "peername" not in source
    assert "127.0.0.1" not in source
    assert "self.runtime(request, alias)" in (COMPONENT / "http.py").read_text()


def _camera_method(name: str):
    tree = ast.parse((COMPONENT / "camera.py").read_text())
    cls = next(n for n in tree.body if isinstance(n, ast.ClassDef) and n.name == "BlinkLiveCamera")
    method = next(n for n in cls.body if getattr(n, "name", None) == name)
    method.decorator_list = []
    return method


@pytest.mark.parametrize(("use_ssl", "scheme"), [(False, "http"), (True, "https")])
async def test_stream_source_uses_configured_port_scheme_and_entity_token(use_ssl, scheme):
    from yarl import URL

    namespace = {
        "URL": URL,
        "API_PREFIX": "/api/blink_live_bridge",
        "secrets": __import__("secrets"),
    }
    namespace["STREAM_TOKEN_PARAM"] = "auth"
    module = ast.Module(body=[_camera_method("stream_source")], type_ignores=[])
    exec(compile(ast.fix_missing_locations(module), "camera.py", "exec"), namespace)
    camera = SimpleNamespace(
        alias="cucina",
        runtime=SimpleNamespace(stream_tokens={}),
        hass=SimpleNamespace(
            config=SimpleNamespace(api=SimpleNamespace(use_ssl=use_ssl, port=8443))
        ),
    )
    first = URL(await namespace["stream_source"](camera))
    second = URL(await namespace["stream_source"](camera))
    assert (first.scheme, first.host, first.port) == (scheme, "127.0.0.1", 8443)
    assert first.path == "/api/blink_live_bridge/v1/cameras/cucina/live.ts"
    assert len(first.query["auth"]) >= 40
    # The stream worker reuses one URL; the token must stay stable for the entity.
    assert first == second
    assert camera.runtime.stream_tokens == {"cucina": first.query["auth"]}


async def test_allowed_path_check_runs_in_the_executor():
    executed = []

    async def executor(func, *args):
        executed.append(func)
        return func(*args)

    namespace = {"ServiceValidationError": ValueError}
    module = ast.Module(body=[_camera_method("_validate_path")], type_ignores=[])
    exec(compile(ast.fix_missing_locations(module), "camera.py", "exec"), namespace)
    config = SimpleNamespace(is_allowed_path=lambda path: path.startswith("/media/"))
    camera = SimpleNamespace(hass=SimpleNamespace(config=config, async_add_executor_job=executor))
    await namespace["_validate_path"](camera, "/media/clip.mp4")
    with pytest.raises(ValueError):
        await namespace["_validate_path"](camera, "/etc/passwd")
    assert executed == [config.is_allowed_path, config.is_allowed_path]
