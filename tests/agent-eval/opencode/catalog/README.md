# Public metadata for the two configured review models

The empty catalog lowered memory but removed the Kimi endpoint. The retained
review failed before a model response. Model listing alone was insufficient.

This replacement contains only `kimi-code-plan-global/k3` and
`zai-coding-plan/glm-5.3-flash`. It preserves provider endpoints and adapters,
capabilities, reasoning options, interleaved reasoning, subscription cost
metadata and model limits. The frozen review settings still select a 32,768-token
context and 2,048-token output; public catalog limits do not enlarge the budget.

Six public TOML files come from
[models.dev revision d2f7be0a](https://github.com/anomalyco/models.dev/tree/d2f7be0a875417914f280086eccae15930f2341e).
Their hashes are retained in `2026-10-07/manifest.json`. `prepare.py check`
reconstructs the 1,309-byte catalog offline from these sources. It selects the
fields consumed by the pinned OpenCode catalog schema; benchmark claims are not
used. These are evaluation model choices, not repository-specific `fr` behavior.

`OpenCode memory controls` tests this exact file on Linux and macOS with the
scripted provider and unchanged limits. Passing measurement CI alone does not
grant admission. All three candidate captures must establish headroom, followed
by one guarded workstation control. Actual provider compatibility still needs a
fresh frozen review. Prior failed collections remain stopped.

No credentials are included. `env` contains public variable names only. OpenCode
continues to resolve its existing configured authentication itself.
