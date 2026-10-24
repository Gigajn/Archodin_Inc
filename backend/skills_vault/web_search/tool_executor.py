#!/usr/bin/env python3
"""web_search skill: free DuckDuckGo Instant Answer lookup (no API key required).

Usage:
    python tool_executor.py --selftest
    python tool_executor.py --query "bitcoin halving"
"""

from __future__ import annotations

import argparse
import json
import sys

import httpx

API_URL = "https://api.duckduckgo.com/"


def search(query: str, timeout: float = 20.0) -> dict[str, object]:
    params = {"q": query, "format": "json", "no_html": 1, "skip_disambig": 1}
    with httpx.Client(timeout=timeout) as client:
        response = client.get(API_URL, params=params)
        response.raise_for_status()
        data = response.json()

    related: list[str] = []
    for item in data.get("RelatedTopics") or []:
        if isinstance(item, dict) and item.get("Text"):
            related.append(str(item["Text"]))
        if len(related) >= 5:
            break

    return {
        "query": query,
        "abstract": data.get("AbstractText") or "",
        "related_topics": related,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="DuckDuckGo Instant Answer search")
    parser.add_argument("--selftest", action="store_true", help="print OK without touching the network")
    parser.add_argument("--query", default=None, help="search query")
    args = parser.parse_args(argv)

    if args.selftest:
        print("OK")
        return 0

    if not args.query:
        parser.error("--query is required unless --selftest is used")

    print(json.dumps(search(args.query), ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
