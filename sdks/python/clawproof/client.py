"""Synchronous and asynchronous clients for the ClawProof zkML API."""

from __future__ import annotations

import time
from typing import Any, Optional

import httpx

from .types import Model, Receipt

_DEFAULT_BASE_URL = "https://clawproof.onrender.com"
_DEFAULT_TIMEOUT = 30.0


class ClawProofError(Exception):
    """Raised when the ClawProof API returns an error response."""

    def __init__(self, status_code: int, message: str, hint: Optional[str] = None) -> None:
        self.status_code = status_code
        self.message = message
        self.hint = hint
        detail = f"[{status_code}] {message}"
        if hint:
            detail += f" (hint: {hint})"
        super().__init__(detail)


def _raise_for_error(response: httpx.Response) -> None:
    """Raise :class:`ClawProofError` for non-2xx responses."""
    if response.is_success:
        return
    try:
        body = response.json()
        message = body.get("error", response.text)
        hint = body.get("hint")
    except Exception:
        message = response.text
        hint = None
    raise ClawProofError(response.status_code, message, hint)


def _build_prove_payload(
    model_id: str,
    fields: Optional[dict[str, int]] = None,
    text: Optional[str] = None,
    raw: Optional[list[int]] = None,
    webhook_url: Optional[str] = None,
) -> dict[str, Any]:
    """Construct the JSON body for ``POST /prove``."""
    payload: dict[str, Any] = {"model_id": model_id, "input": {}}
    if fields is not None:
        payload["input"]["fields"] = fields
    if text is not None:
        payload["input"]["text"] = text
    if raw is not None:
        payload["input"]["raw"] = raw
    if webhook_url is not None:
        payload["webhook_url"] = webhook_url
    return payload


# ---------------------------------------------------------------------------
# Synchronous client
# ---------------------------------------------------------------------------


class ClawProof:
    """Synchronous Python client for the ClawProof zkML API.

    Parameters
    ----------
    base_url:
        Root URL of the ClawProof server.  Defaults to the hosted
        instance at ``https://clawproof.onrender.com``.
    timeout:
        Default request timeout in seconds.
    """

    def __init__(
        self,
        base_url: str = _DEFAULT_BASE_URL,
        timeout: float = _DEFAULT_TIMEOUT,
    ) -> None:
        self.base_url = base_url.rstrip("/")
        self._client = httpx.Client(
            base_url=self.base_url,
            timeout=timeout,
            headers={"Accept": "application/json"},
        )

    # -- lifecycle -----------------------------------------------------------

    def close(self) -> None:
        """Close the underlying HTTP connection pool."""
        self._client.close()

    def __enter__(self) -> ClawProof:
        return self

    def __exit__(self, *args: Any) -> None:
        self.close()

    # -- public API ----------------------------------------------------------

    def health(self) -> dict:
        """``GET /health`` -- server health and readiness status."""
        resp = self._client.get("/health")
        _raise_for_error(resp)
        return resp.json()

    def models(self) -> list[Model]:
        """``GET /models`` -- list all registered models."""
        resp = self._client.get("/models")
        _raise_for_error(resp)
        return [Model.from_dict(m) for m in resp.json()]

    def prove(
        self,
        model_id: str,
        *,
        fields: Optional[dict[str, int]] = None,
        text: Optional[str] = None,
        raw: Optional[list[int]] = None,
        webhook_url: Optional[str] = None,
    ) -> Receipt:
        """``POST /prove`` -- submit a proof request.

        Returns a :class:`Receipt` whose ``status`` will initially be
        ``"proving"``.  Use :meth:`receipt` or :meth:`prove_and_wait`
        to poll until completion.
        """
        payload = _build_prove_payload(model_id, fields=fields, text=text, raw=raw, webhook_url=webhook_url)
        resp = self._client.post("/prove", json=payload)
        _raise_for_error(resp)
        return Receipt.from_dict(resp.json())

    def prove_and_wait(
        self,
        model_id: str,
        *,
        timeout: float = 300,
        poll_interval: float = 3,
        fields: Optional[dict[str, int]] = None,
        text: Optional[str] = None,
        raw: Optional[list[int]] = None,
        webhook_url: Optional[str] = None,
    ) -> Receipt:
        """Submit a proof request and block until it reaches a terminal state.

        Polls ``GET /receipt/{id}`` every *poll_interval* seconds until
        the receipt status is no longer ``"proving"`` or until *timeout*
        seconds have elapsed.

        Raises :class:`TimeoutError` if the proof does not complete
        within the given timeout.
        """
        receipt = self.prove(
            model_id,
            fields=fields,
            text=text,
            raw=raw,
            webhook_url=webhook_url,
        )
        deadline = time.monotonic() + timeout
        while receipt.status == "proving":
            if time.monotonic() >= deadline:
                raise TimeoutError(
                    f"Proof for receipt {receipt.id} did not complete within {timeout}s"
                )
            time.sleep(poll_interval)
            receipt = self.receipt(receipt.id)
        return receipt

    def receipt(self, receipt_id: str) -> Receipt:
        """``GET /receipt/{id}`` -- fetch a proof receipt by ID."""
        resp = self._client.get(f"/receipt/{receipt_id}")
        _raise_for_error(resp)
        return Receipt.from_dict(resp.json())

    def verify(self, receipt_id: str) -> dict:
        """``POST /verify`` -- check whether a receipt's proof is valid.

        Returns the raw JSON response containing ``valid``,
        ``receipt_id``, and ``status`` keys.
        """
        resp = self._client.post("/verify", json={"receipt_id": receipt_id})
        _raise_for_error(resp)
        return resp.json()

    def batch_prove(
        self,
        requests: list[dict[str, Any]],
    ) -> list[Receipt]:
        """``POST /prove/batch`` -- submit up to 5 proof requests at once.

        Each element of *requests* should be a dict with at least
        ``model_id`` and an ``input`` dict, mirroring the single-prove
        payload::

            client.batch_prove([
                {"model_id": "authorization", "input": {"fields": {"budget": 10, "trust": 5, "amount": 8, "category": 2, "velocity": 3, "day": 1, "time": 2, "risk": 0}}},
                {"model_id": "authorization", "input": {"fields": {"budget": 15, "trust": 1, "amount": 12, "category": 3, "velocity": 5, "day": 0, "time": 3, "risk": 0}}},
            ])
        """
        resp = self._client.post("/prove/batch", json={"requests": requests})
        _raise_for_error(resp)
        data = resp.json()
        return [Receipt.from_dict(r) for r in data.get("receipts", [])]


# ---------------------------------------------------------------------------
# Asynchronous client
# ---------------------------------------------------------------------------


class AsyncClawProof:
    """Asynchronous Python client for the ClawProof zkML API.

    Parameters
    ----------
    base_url:
        Root URL of the ClawProof server.
    timeout:
        Default request timeout in seconds.
    """

    def __init__(
        self,
        base_url: str = _DEFAULT_BASE_URL,
        timeout: float = _DEFAULT_TIMEOUT,
    ) -> None:
        self.base_url = base_url.rstrip("/")
        self._client = httpx.AsyncClient(
            base_url=self.base_url,
            timeout=timeout,
            headers={"Accept": "application/json"},
        )

    # -- lifecycle -----------------------------------------------------------

    async def close(self) -> None:
        """Close the underlying HTTP connection pool."""
        await self._client.aclose()

    async def __aenter__(self) -> AsyncClawProof:
        return self

    async def __aexit__(self, *args: Any) -> None:
        await self.close()

    # -- public API ----------------------------------------------------------

    async def health(self) -> dict:
        """``GET /health`` -- server health and readiness status."""
        resp = await self._client.get("/health")
        _raise_for_error(resp)
        return resp.json()

    async def models(self) -> list[Model]:
        """``GET /models`` -- list all registered models."""
        resp = await self._client.get("/models")
        _raise_for_error(resp)
        return [Model.from_dict(m) for m in resp.json()]

    async def prove(
        self,
        model_id: str,
        *,
        fields: Optional[dict[str, int]] = None,
        text: Optional[str] = None,
        raw: Optional[list[int]] = None,
        webhook_url: Optional[str] = None,
    ) -> Receipt:
        """``POST /prove`` -- submit a proof request."""
        payload = _build_prove_payload(model_id, fields=fields, text=text, raw=raw, webhook_url=webhook_url)
        resp = await self._client.post("/prove", json=payload)
        _raise_for_error(resp)
        return Receipt.from_dict(resp.json())

    async def prove_and_wait(
        self,
        model_id: str,
        *,
        timeout: float = 300,
        poll_interval: float = 3,
        fields: Optional[dict[str, int]] = None,
        text: Optional[str] = None,
        raw: Optional[list[int]] = None,
        webhook_url: Optional[str] = None,
    ) -> Receipt:
        """Submit a proof request and poll until it reaches a terminal state.

        Raises :class:`TimeoutError` if the proof does not complete
        within the given timeout.
        """
        import asyncio

        receipt = await self.prove(
            model_id,
            fields=fields,
            text=text,
            raw=raw,
            webhook_url=webhook_url,
        )
        deadline = time.monotonic() + timeout
        while receipt.status == "proving":
            if time.monotonic() >= deadline:
                raise TimeoutError(
                    f"Proof for receipt {receipt.id} did not complete within {timeout}s"
                )
            await asyncio.sleep(poll_interval)
            receipt = await self.receipt(receipt.id)
        return receipt

    async def receipt(self, receipt_id: str) -> Receipt:
        """``GET /receipt/{id}`` -- fetch a proof receipt by ID."""
        resp = await self._client.get(f"/receipt/{receipt_id}")
        _raise_for_error(resp)
        return Receipt.from_dict(resp.json())

    async def verify(self, receipt_id: str) -> dict:
        """``POST /verify`` -- check whether a receipt's proof is valid."""
        resp = await self._client.post("/verify", json={"receipt_id": receipt_id})
        _raise_for_error(resp)
        return resp.json()

    async def batch_prove(
        self,
        requests: list[dict[str, Any]],
    ) -> list[Receipt]:
        """``POST /prove/batch`` -- submit up to 5 proof requests at once."""
        resp = await self._client.post("/prove/batch", json={"requests": requests})
        _raise_for_error(resp)
        data = resp.json()
        return [Receipt.from_dict(r) for r in data.get("receipts", [])]
