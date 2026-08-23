"""Authenticated compatibility proxy for Home Assistant and SceneTrove."""

from aiohttp import web
from homeassistant.components.http import HomeAssistantView
from homeassistant.core import HomeAssistant

from .auth import require_bridge_auth
from .client import EngineError
from .const import API_PREFIX, DOMAIN
from .runtime import BridgeRuntime


class BridgeView(HomeAssistantView):
    """Common standalone runtime and consumer authorization lookup."""

    requires_auth = False

    def runtime(self, request: web.Request) -> BridgeRuntime:
        runtime: BridgeRuntime = request.app["hass"].data[DOMAIN]["runtime"]
        require_bridge_auth(request, runtime.token)
        return runtime


class HealthView(BridgeView):
    url = f"{API_PREFIX}/healthz"
    name = f"api:{DOMAIN}:health"

    async def get(self, request: web.Request) -> web.Response:
        runtime = self.runtime(request)
        try:
            value = await runtime.client.get_json("/healthz")
        except EngineError as error:
            raise web.HTTPBadGateway(text="standalone provider unavailable") from error
        return self.json(value)


class CamerasView(BridgeView):
    url = f"{API_PREFIX}/v1/cameras"
    name = f"api:{DOMAIN}:cameras"

    async def get(self, request: web.Request) -> web.Response:
        return self.json({"cameras": self.runtime(request).cameras})


class SnapshotView(BridgeView):
    url = f"{API_PREFIX}/v1/cameras/{{alias}}/snapshot.jpg"
    name = f"api:{DOMAIN}:snapshot"

    async def get(self, request: web.Request, alias: str) -> web.Response:
        try:
            image = await self.runtime(request).client.bytes(f"/v1/cameras/{alias}/snapshot.jpg")
        except EngineError as error:
            raise web.HTTPServiceUnavailable(text="snapshot unavailable") from error
        return web.Response(body=image, content_type="image/jpeg")


class LiveView(BridgeView):
    url = f"{API_PREFIX}/v1/cameras/{{alias}}/{{stream_format:live\\.(?:ts|mpegts)}}"
    name = f"api:{DOMAIN}:live"

    async def get(
        self,
        request: web.Request,
        alias: str,
        stream_format: str,
    ) -> web.StreamResponse:
        del stream_format
        runtime = self.runtime(request)
        try:
            upstream = await runtime.client.stream(f"/v1/cameras/{alias}/live.ts")
        except EngineError as error:
            raise web.HTTPBadGateway(text="live stream unavailable") from error
        response = web.StreamResponse(
            status=200,
            headers={
                "Cache-Control": "no-store",
                "Content-Type": "video/mp2t",
                "X-Content-Type-Options": "nosniff",
            },
        )
        await response.prepare(request)
        try:
            async for chunk in upstream.content.iter_chunked(64 * 1024):
                await response.write(chunk)
        except (ConnectionError, RuntimeError):
            pass
        finally:
            upstream.close()
        return response


def register_views(hass: HomeAssistant) -> None:
    """Register the stable consumer API exactly once."""
    for view in (HealthView, CamerasView, SnapshotView, LiveView):
        hass.http.register_view(view)
