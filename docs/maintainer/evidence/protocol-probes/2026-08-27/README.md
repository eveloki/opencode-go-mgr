# Protocol probe snapshot — 2026-08-27

This directory preserves the sanitized evidence behind the 2026-08-27 built-in protocol snapshot. It is maintenance evidence, not runtime state and not an instruction to probe models during normal routing.

## Scope and method

- Current persisted catalogs: OpenCode Go 31 models, Zen Free 7 models, Command Code GOAT 58 models.
- Every model was sent a minimal Chat Completions, Responses, and Messages request in both non-streaming and streaming form.
- Go and GOAT each used one enabled test account; Zen Free was anonymous. Account labels and credentials are intentionally omitted from this artifact.
- Outer concurrency was 4 for Go and GOAT and 1 for Zen Free.
- The initial sweep used an 8-token output limit. Pairs that explicitly required at least 16 tokens, plus two transient pairs, were rerun at 16 tokens. `merged-latest.jsonl` contains the replacement observations.
- No automatic retries were performed. The twelve targeted replacement requests are retained in `all-attempts.jsonl`.

## Files

- `all-attempts.jsonl`: all 588 requests, including the initial sweep and targeted reruns.
- `merged-latest.jsonl`: the latest observation for each provider/model/protocol/stream tuple, 576 rows.
- `classified-pairs.json`: 288 provider/model/protocol pairs with both stream observations and the derived classification.

## Classification rules

- `live_supported`: both streaming and non-streaming requests returned a usable protocol-shaped 2xx response.
- `protocol_confirmed_plan_denied`: GOAT recognized the model on the expected protocol path in both modes but returned `MODEL_NOT_IN_PLAN`. This confirms protocol shape, but the channel was not usable in this run, so it is excluded from static support and defaults off.
- `explicit_unsupported`: the upstream explicitly rejected the protocol shape/model pairing or reported that the route does not exist.
- `model_unavailable`, `rate_limited`, and `transient_inconclusive`: evidence about current availability, not proof that a protocol shape is unsupported.
- `failed_unclassified`: neither success nor a sufficiently specific negative result. Static reset treats it as absent/default-off while retaining the uncertainty here.

The normalized snapshot contains 23 Go Chat, 7 Go Responses, 12 Go Messages; 2 Zen Free Chat and 1 Zen Free Responses; and 39 GOAT Chat pairs. GOAT has no currently usable Messages or Responses pair in this run. `stealth/ox-alpha` was explicitly rejected on all three GOAT paths.
