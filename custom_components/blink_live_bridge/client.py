"""Private client for the local Vistoda Blink Rust engine."""

from typing import Any

from aiohttp import ClientError, ClientResponse, ClientTimeout
from homeassistant.core import HomeAssistant
from homeassistant.helpers.aiohttp_client import async_get_clientsession

from .const import ENGINE_URL


class EngineError(Exception):
    """Safe provider failure without response-body secret leakage."""

    def __init__(self, message: str, status: int | None = None) -> None:
        super().__init__(message)
        self.status = status


class EngineClient:
    """Bounded HTTP facade used by config flow, entities and proxy views."""

    def __init__(self, hass: HomeAssistant, token: str, base_url: str = ENGINE_URL) -> None:
        self._session = async_get_clientsession(hass)
        self._headers = {"Authorization": f"Bearer {token}"}
        self._base_url = base_url.rstrip("/")

    async def get_json(self, path: str) -> dict[str, Any]:
        response = await self._request("GET", path)
        try:
            return await response.json()
        finally:
            response.release()

    async def post(self, path: str, payload: dict[str, Any] | None = None) -> dict[str, Any]:
        response = await self._request("POST", path, payload)
        try:
            return await response.json() if response.status != 204 else {}
        finally:
            response.release()

    async def stream(self, path: str) -> ClientResponse:
        return await self._request("GET", path, request_timeout=None)

    async def bytes(self, path: str) -> bytes:
        response = await self._request("GET", path, request_timeout=90)
        try:
            return await response.read()
        finally:
            response.release()

    async def _request(
        self,
        method: str,
        path: str,
        payload: dict[str, Any] | None = None,
        request_timeout: float | None = 30,
    ) -> ClientResponse:
        try:
            response = await self._session.request(
                method,
                f"{self._base_url}{path}",
                headers=self._headers,
                json=payload,
                timeout=ClientTimeout(total=request_timeout),
            )
            if response.status >= 400:
                status = response.status
                response.release()
                raise EngineError(f"provider returned HTTP {status}", status)
            return response
        except (ClientError, TimeoutError) as error:
            raise EngineError("standalone provider request failed") from error
