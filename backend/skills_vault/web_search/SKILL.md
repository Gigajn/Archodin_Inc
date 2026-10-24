# name: web_search
category: CORE
source_type: API_WRAPPER
install_command: pip install httpx

# web_search

Free web lookups through the DuckDuckGo Instant Answer API. No API key, no quota
counter, no billing — which makes it the default research tool for the trend
scouting and affiliate divisions.

## When to use

- Finding a product, company or concept summary before deeper crawling
- Cheap first pass before spending tokens on a full page crawl

## Usage

```bash
python tool_executor.py --query "shopee affiliate best selling categories"
python tool_executor.py --selftest
```

Output is JSON: `{"query": str, "abstract": str, "related_topics": [str, ...]}`.

## Limits

- Instant Answer API returns curated summaries, not full search results. For deep
  page content, pair it with a crawler skill.
