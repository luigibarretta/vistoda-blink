"""Private HTTP API shared by Home Assistant and SceneTrove."""

from aiohttp import web

from homeassistant.components.http import HomeAssistantView
from homeassistant.core import HomeAssistant

from .auth import require_bridge_auth
from .const import API_PREFIX, DOMAIN, POWERED_CAMERA_TYPES
from .runtime import BridgeRuntime


class BridgeView(HomeAssistantView):
    """Common runtime and authorization lookup."""

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
        return self.json({"status": "ok", "cameras": len(runtime.cameras)})


class CamerasView(BridgeView):
    url = f"{API_PREFIX}/v1/cameras"
    name = f"api:{DOMAIN}:cameras"

    async def get(self, request: web.Request) -> web.Response:
        runtime = self.runtime(request)
        return self.json(
            {
                "cameras": [
                    {
                        "alias": alias,
                        "powered": str(camera.camera_type).casefold()
                        in POWERED_CAMERA_TYPES,
                        "live_mpegts": True,
                        "snapshot": camera.image_from_cache is not None,
                    }
                    for alias, camera in runtime.cameras.items()
                ]
            }
        )


class SnapshotView(BridgeView):
    url = f"{API_PREFIX}/v1/cameras/{{alias}}/snapshot.jpg"
    name = f"api:{DOMAIN}:snapshot"

    async def get(self, request: web.Request, alias: str) -> web.Response:
        runtime = self.runtime(request)
        camera = runtime.cameras.get(alias)
        if camera is None:
            raise web.HTTPNotFound()
        image = camera.image_from_cache
        if not image:
            raise web.HTTPServiceUnavailable(text="snapshot unavailable")
        return web.Response(body=image, content_type="image/jpeg")


class LiveView(BridgeView):
    url = f"{API_PREFIX}/v1/cameras/{{alias}}/{{format:live\\.(?:ts|mpegts)}}"
    name = f"api:{DOMAIN}:live"

    async def get(
        self, request: web.Request, alias: str, format: str
    ) -> web.StreamResponse:
        runtime = self.runtime(request)
        relay = runtime.relays.relays.get(alias)
        if relay is None:
            raise web.HTTPNotFound()
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
            async for chunk in relay.subscribe():
                await response.write(chunk)
        except (ConnectionError, RuntimeError):
            pass
        return response


def register_views(hass: HomeAssistant) -> None:
    """Register the bridge API exactly once."""
    for view in (HealthView, CamerasView, SnapshotView, LiveView):
        hass.http.register_view(view)
