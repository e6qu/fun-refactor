# New questions for terminal source review

This is a checked design, not a completed review or an executable frozen collection.
No model calls have been made for it. It covers three remaining assertions:

| Question | Exact scope | Still outside this review |
|---|---|---|
| dotenv disabled interpolation | One `dotenv_values(..., interpolate=False)` input retains its literal expression | Enabled interpolation, other inputs and APIs |
| packaging one-shot iterable | The first matching-finals input preserves both final items and consumes the generator once | Other ranges, policies and inputs |
| platformdirs site fallback | `XDG_CONFIG_DIRS=':relative::'` returns the default config path with app/version suffixes | Data directories, wrappers, creation and other values |

Each question explicitly discloses the proposed reference and limits the requested judgment.
The packets include requirements, the selected grader code, relevant source and checked before/after
file identities. Additional frozen source remains available through read-only tools. A no-finding
answer will not establish whole-task acceptance.

The design refers to the unchanged source archive from the earlier single-assertion collection.
It pins the archive and every selected file slice by SHA-256. Preparation checks the recorded
file comparisons against the source bytes. It does not execute candidate code or copy the archive
into the repository again. All previous stopped cells remain stopped.

The model IDs and endpoints match cached OpenCode metadata inspected on October 6, 2026.
Both use the OpenAI-compatible adapter. The design caps configured context at 32,768 and output
at 2,048 tokens; these are evaluation settings, not advertised model capacities. Cached metadata
does not establish current service access or live compatibility. GitHub has no provider secret.

Check the design without launching OpenCode:

```sh
python3 -B tools/prepare-terminal-reviews.py check \
  tests/agent-eval/opencode/reviews/2026-10-06-terminal-design/design.json
```

On a hosted runner with pinned binaries, freeze before any model call:

```sh
python3 -B tools/prepare-terminal-reviews.py freeze \
  tests/agent-eval/opencode/reviews/2026-10-06-terminal-design/design.json \
  --output target/review-inputs --fr /path/to/fr --opencode /path/to/opencode
```

Retain the resulting plan and source archive. Review its returned SHA-256, then follow the
[collection commands](../../../../../docs/opencode-native-tools.md#collect-a-frozen-review-in-order).
The collector needs distinct credentials for both providers. Never place them in this design,
the plan, a command argument or retained evidence. Do not reset the output directory or rerun a
hosted job to restart attempted cells. An interrupted unsealed cell requires investigation, not a retry.

There are six ordered cells, one per question/model pair. Each keeps the existing 120-second,
20-CPU-second, 768-MiB sampled RSS and 16-MiB disk-growth limits. Two consecutive failures stop
the remaining cells. Scripted protocol controls do not establish that either live model will finish.
