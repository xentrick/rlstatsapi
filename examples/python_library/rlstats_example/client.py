from __future__ import annotations

import json
from dataclasses import dataclass
from typing import Any, Iterator

import rlstatsapi


if not hasattr(rlstatsapi, "RocketLeagueStatsClient"):
    raise ImportError(
        "Expected local rlstatsapi PyO3 bindings with RocketLeagueStatsClient; "
        "ensure your environment resolves rlstatsapi from this repository."
    )


@dataclass(frozen=True)
class ParsedEvent:
    event: str
    data: dict[str, Any]


class RLStatsStream:
    def __init__(
        self,
        host: str = "127.0.0.1",
        port: int = 49123,
        ini_path: str | None = None,
    ) -> None:
        self._client = rlstatsapi.RocketLeagueStatsClient(
            host=host,
            port=port,
            ini_path=ini_path,
        )

    def connect(self) -> None:
        self._client.connect()

    def reconnect(self) -> None:
        self._client.reconnect()

    def close(self) -> None:
        self._client.close()

    def next_event(self) -> ParsedEvent | None:
        raw = self._client.next_event_json()
        if raw is None:
            return None
        return self.parse_event(raw)

    def iter_events(self, limit: int | None = None) -> Iterator[ParsedEvent]:
        seen = 0
        while True:
            event = self.next_event()
            if event is None:
                break
            yield event
            seen += 1
            if limit is not None and seen >= limit:
                break

    @staticmethod
    def parse_event(raw_json: str) -> ParsedEvent:
        normalized = rlstatsapi.parse_event_json(raw_json)
        payload = json.loads(normalized)
        return ParsedEvent(event=payload["event"], data=payload["data"])
