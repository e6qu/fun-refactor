"""Small HTTP adapter for the content-addressed object-store protocol."""

from __future__ import annotations

import json
from http.client import HTTPMessage
import re
from typing import Any, IO, Mapping
from urllib.error import HTTPError, URLError
from urllib.parse import urlsplit, urlunsplit
from urllib.request import HTTPRedirectHandler, Request, build_opener

from .context import _RECORD_MAX_BYTES, _checked_digest
from .runtime import FrRuntimeError, _copy_json


class _NoRedirect(HTTPRedirectHandler):
    def redirect_request(
        self,
        req: Request,
        fp: IO[bytes],
        code: int,
        msg: str,
        headers: HTTPMessage,
        newurl: str,
    ) -> Request | None:
        return None


class HttpObjectStore:
    """A bounded ``GET``/``PUT`` adapter for ``<base>/<sha256>`` object endpoints."""

    def __init__(
        self,
        base_url: str,
        *,
        headers: Mapping[str, str] | None = None,
        timeout: float = 20.0,
    ) -> None:
        parsed = urlsplit(base_url)
        if (parsed.scheme not in ("http", "https") or not parsed.netloc
                or parsed.username is not None or parsed.password is not None
                or parsed.query or parsed.fragment):
            raise FrRuntimeError("HTTP object-store URL must be an HTTP(S) base without credentials, query or fragment")
        if (not isinstance(timeout, (int, float)) or isinstance(timeout, bool)
                or not 0 < timeout <= 120):
            raise FrRuntimeError("HTTP object-store timeout must be within 0 and 120 seconds")
        supplied = dict(headers or {})
        malformed = any(
            not isinstance(name, str) or not isinstance(value, str)
            or not re.fullmatch(r"[!#$%&'*+.^_`|~0-9A-Za-z-]+", name)
            or "\r" in value or "\n" in value
            for name, value in supplied.items()
        )
        if (malformed
                or sum(len(name) + len(value) for name, value in supplied.items()) > 8_192):
            raise FrRuntimeError("HTTP object-store headers are malformed or exceed 8 KiB")
        self.base_url = urlunsplit((
            parsed.scheme, parsed.netloc, parsed.path.rstrip("/"), "", "",
        ))
        self.headers = supplied
        self.timeout = float(timeout)
        self._opener = build_opener(_NoRedirect)

    def _url(self, digest: str) -> str:
        return f"{self.base_url}/{_checked_digest(digest)}"

    def get(self, digest: str) -> Mapping[str, Any] | None:
        request = Request(self._url(digest), headers=self.headers, method="GET")
        try:
            with self._opener.open(request, timeout=self.timeout) as response:
                length = response.headers.get("Content-Length")
                if length is not None and (not length.isascii() or not length.isdigit()
                                           or int(length) > _RECORD_MAX_BYTES):
                    raise FrRuntimeError("HTTP object-store record exceeds 1 MiB")
                encoded = response.read(_RECORD_MAX_BYTES + 1)
        except HTTPError as error:
            if error.code == 404:
                return None
            raise FrRuntimeError(f"HTTP object-store GET failed with status {error.code}") from error
        except (OSError, URLError) as error:
            raise FrRuntimeError("HTTP object-store GET could not complete") from error
        if len(encoded) > _RECORD_MAX_BYTES:
            raise FrRuntimeError("HTTP object-store record exceeds 1 MiB")
        try:
            value = json.loads(encoded)
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            raise FrRuntimeError("HTTP object-store record is not JSON") from error
        if not isinstance(value, dict):
            raise FrRuntimeError("HTTP object-store record is malformed")
        return _copy_json(value)

    def put(self, digest: str, record: Mapping[str, Any]) -> None:
        digest = _checked_digest(digest)
        try:
            encoded = json.dumps(
                record, ensure_ascii=False, sort_keys=True,
                separators=(",", ":"), allow_nan=False,
            ).encode("utf-8")
        except (TypeError, ValueError) as error:
            raise FrRuntimeError("HTTP object-store record is not canonical JSON") from error
        if len(encoded) > _RECORD_MAX_BYTES:
            raise FrRuntimeError("HTTP object-store record exceeds 1 MiB")
        headers = {**self.headers, "Content-Type": "application/json"}
        request = Request(self._url(digest), data=encoded, headers=headers, method="PUT")
        try:
            with self._opener.open(request, timeout=self.timeout) as response:
                if response.status not in (200, 201, 204):
                    raise FrRuntimeError(
                        f"HTTP object-store PUT failed with status {response.status}"
                    )
                response.read(_RECORD_MAX_BYTES + 1)
        except HTTPError as error:
            raise FrRuntimeError(f"HTTP object-store PUT failed with status {error.code}") from error
        except (OSError, URLError) as error:
            raise FrRuntimeError("HTTP object-store PUT could not complete") from error
