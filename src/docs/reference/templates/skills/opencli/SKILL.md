---
name: opencli
description: "Reference for all 25+ opencli-rs dynamic tools (news, social, search, web). Use when user asks about trending topics, news, social media, jobs, or web search. (/opencli, opencli tools, news, trending)"
---

# opencli-rs Dynamic Tools Reference

25+ dynamic tools powered by `opencli-rs` registered in `tools.toml`. Use them when the user asks about news, social media, search, or web content.

## When to use

- User asks "what's trending on HN/Twitter/Reddit" → `hn_top`, `twitter_trending`, `reddit_hot`
- User asks to search news/papers/articles → `arxiv_search`, `bbc_top`, `reuters_top`, `medium_search`, `devto_top`
- User asks about a Twitter user or wants to post/reply/like → `twitter_profile`, `twitter_post`, `twitter_reply`, `twitter_like`
- User asks to search the web → `google_search`, `wikipedia_search`
- User asks about jobs → `linkedin_jobs`
- User asks about Stack Overflow → `stackoverflow_hot`

## Write actions require approval

`twitter_post`, `twitter_reply`, `twitter_follow`, `twitter_like`, `twitter_dm`, `reddit_comment` — always confirm with the user first.

## Requirements

Requires daemon running (`opencli-rs --daemon` on port 19825) + Brave extension for browser-based tools. API-only tools (`hn_top`, `hn_search`) work without it.

## Full tool definitions

`~/.opencrabs/tools.toml` — check this file for exact command syntax, parameters, and defaults.
