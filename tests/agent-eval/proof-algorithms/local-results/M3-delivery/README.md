# Can the agents deliver two supplied proofs?

Compare the same agent with the shared skill alone against the shared skill plus the finish card. Both receive identical correct drafts, source, tools and checks. Include setup, instructions, tool use, package checks, patch delivery, accounting and failed attempts. This comparison does not measure fr overhead against ordinary Lean.

Both correct proofs are supplied. This tests saving and delivery, not proof discovery.
Each complete attempt has 150 seconds including tools, checks, patch delivery and session accounting.
These small development comparisons do not close M3 or measure fr overhead against ordinary tools.
Two repetitions cannot establish a general success rate. Keep every failure in the total work.

| Agent | shared-skill: accepted / 2 | finish-card: accepted / 2 | Total seconds, including failures |
| --- | ---: | ---: | ---: |
| codex-luna-fast | 2 | 2 | 145.106 |
| opencode2-glm-flash | 1 | 0 | 354.358 |
| opencode2-kimi-k3 | 0 | 0 | 383.704 |

codex-luna-fast: For each profile, finish-card 2/2 versus shared-skill 0/2 warrants a different delivery task before adoption. The reverse warrants inspecting harm. Every other result is inconclusive about a success advantage. Never replace or rerun failed slots to seek success.
opencode2-glm-flash: Execution is invalid or unverified in at least one attempt. Review its recorded cause before drawing conclusions about guidance or agent ability. Preserve every original outcome; do not replace failed slots.
opencode2-kimi-k3: Execution is invalid or unverified in at least one attempt. Review its recorded cause before drawing conclusions about guidance or agent ability. Preserve every original outcome; do not replace failed slots.

Supplied file sizes in UTF-8 bytes (not model tokens or evidence that the agent read them):

| Condition | File | Bytes |
| --- | --- | ---: |
| shared-skill | `.agents/skills/fr/SKILL.md` | 841 |
| shared-skill | `.agents/skills/fr/references/application.md` | 2974 |
| shared-skill | `.agents/skills/fr/references/author.md` | 2601 |
| shared-skill | `.agents/skills/fr/references/batch.md` | 2165 |
| shared-skill | `.agents/skills/fr/references/change.md` | 2573 |
| shared-skill | `.agents/skills/fr/references/checks.md` | 1169 |
| shared-skill | `.agents/skills/fr/references/compiler.md` | 1222 |
| shared-skill | `.agents/skills/fr/references/disclosure.md` | 3030 |
| shared-skill | `.agents/skills/fr/references/explore.md` | 4076 |
| shared-skill | `.agents/skills/fr/references/first-change.md` | 2802 |
| shared-skill | `.agents/skills/fr/references/git-admin.md` | 3585 |
| shared-skill | `.agents/skills/fr/references/git.md` | 733 |
| shared-skill | `.agents/skills/fr/references/guide.md` | 2786 |
| shared-skill | `.agents/skills/fr/references/history.md` | 917 |
| shared-skill | `.agents/skills/fr/references/intents.md` | 3408 |
| shared-skill | `.agents/skills/fr/references/investigations.md` | 3185 |
| shared-skill | `.agents/skills/fr/references/lean-algorithms.md` | 2651 |
| shared-skill | `.agents/skills/fr/references/lean-arithmetic.md` | 2651 |
| shared-skill | `.agents/skills/fr/references/lean-claims.md` | 577 |
| shared-skill | `.agents/skills/fr/references/lean.md` | 3587 |
| shared-skill | `.agents/skills/fr/references/recipes.md` | 912 |
| shared-skill | `.agents/skills/fr/references/recovery.md` | 848 |
| shared-skill | `.agents/skills/fr/references/runtime.md` | 2888 |
| shared-skill | `.agents/skills/fr/references/semantic-change.md` | 3027 |
| shared-skill | `.agents/skills/fr/references/semantic-intent.md` | 3718 |
| shared-skill | `.agents/skills/fr/references/semantic.md` | 3260 |
| shared-skill | `.agents/skills/fr/references/surfaces.md` | 1951 |
| shared-skill | `.agents/skills/fr/references/task.md` | 4087 |
| shared-skill | `.agents/skills/fr/references/workflow.md` | 2791 |
| shared-skill | `.fr/checks.json` | 373 |
| shared-skill | `.fr/euclid_correct.tactics` | 254 |
| shared-skill | `.fr/euclid_decreases.tactics` | 43 |
| finish-card | `.agents/skills/fr/SKILL.md` | 841 |
| finish-card | `.agents/skills/fr/references/application.md` | 2974 |
| finish-card | `.agents/skills/fr/references/author.md` | 2601 |
| finish-card | `.agents/skills/fr/references/batch.md` | 2165 |
| finish-card | `.agents/skills/fr/references/change.md` | 2573 |
| finish-card | `.agents/skills/fr/references/checks.md` | 1169 |
| finish-card | `.agents/skills/fr/references/compiler.md` | 1222 |
| finish-card | `.agents/skills/fr/references/disclosure.md` | 3030 |
| finish-card | `.agents/skills/fr/references/explore.md` | 4076 |
| finish-card | `.agents/skills/fr/references/first-change.md` | 2802 |
| finish-card | `.agents/skills/fr/references/git-admin.md` | 3585 |
| finish-card | `.agents/skills/fr/references/git.md` | 733 |
| finish-card | `.agents/skills/fr/references/guide.md` | 2786 |
| finish-card | `.agents/skills/fr/references/history.md` | 917 |
| finish-card | `.agents/skills/fr/references/intents.md` | 3408 |
| finish-card | `.agents/skills/fr/references/investigations.md` | 3185 |
| finish-card | `.agents/skills/fr/references/lean-algorithms.md` | 2651 |
| finish-card | `.agents/skills/fr/references/lean-arithmetic.md` | 2651 |
| finish-card | `.agents/skills/fr/references/lean-claims.md` | 577 |
| finish-card | `.agents/skills/fr/references/lean.md` | 3587 |
| finish-card | `.agents/skills/fr/references/recipes.md` | 912 |
| finish-card | `.agents/skills/fr/references/recovery.md` | 848 |
| finish-card | `.agents/skills/fr/references/runtime.md` | 2888 |
| finish-card | `.agents/skills/fr/references/semantic-change.md` | 3027 |
| finish-card | `.agents/skills/fr/references/semantic-intent.md` | 3718 |
| finish-card | `.agents/skills/fr/references/semantic.md` | 3260 |
| finish-card | `.agents/skills/fr/references/surfaces.md` | 1951 |
| finish-card | `.agents/skills/fr/references/task.md` | 4087 |
| finish-card | `.agents/skills/fr/references/workflow.md` | 2791 |
| finish-card | `.fr/checks.json` | 373 |
| finish-card | `.fr/euclid_correct.tactics` | 254 |
| finish-card | `.fr/euclid_decreases.tactics` | 43 |
| finish-card | `.fr/finish-card.txt` | 676 |

Only reviewed summaries and submitted patches are public. Raw local records remain private.

| Attempt | Complete outcome | Seconds | Failure or missing result | Patch |
| --- | --- | ---: | --- | --- |
| codex-luna-fast-delivery-shared-skill-1 | passed | 51.495 |  | [patch](patches/codex-luna-fast-delivery-shared-skill-1.patch) |
| codex-luna-fast-delivery-finish-card-1 | passed | 30.063 |  | [patch](patches/codex-luna-fast-delivery-finish-card-1.patch) |
| opencode2-glm-flash-delivery-finish-card-1 | failed | 63.803 | Patch delivered within 150 seconds, but the native JSON stream omitted the final response token record present in the saved session. Complete usage is unknown. | [patch](patches/opencode2-glm-flash-delivery-finish-card-1.patch) |
| opencode2-glm-flash-delivery-shared-skill-1 | passed | 105.329 |  | [patch](patches/opencode2-glm-flash-delivery-shared-skill-1.patch) |
| opencode2-kimi-k3-delivery-shared-skill-1 | failed | 150.019 | Both proofs were saved and the package passed; unsupported history --limit commands consumed remaining time and no patch was delivered before the 150-second limit. | none |
| opencode2-kimi-k3-delivery-finish-card-1 | failed | 113.280 | Patch delivered within 150 seconds, but the native JSON stream omitted the final response token record present in the saved session. Complete usage is unknown. | [patch](patches/opencode2-kimi-k3-delivery-finish-card-1.patch) |
| codex-luna-fast-delivery-finish-card-2 | passed | 35.035 |  | [patch](patches/codex-luna-fast-delivery-finish-card-2.patch) |
| codex-luna-fast-delivery-shared-skill-2 | passed | 28.513 |  | [patch](patches/codex-luna-fast-delivery-shared-skill-2.patch) |
| opencode2-glm-flash-delivery-shared-skill-2 | failed | 65.931 | Patch delivered within 150 seconds, but the native JSON stream omitted the final response token record present in the saved session. Complete usage is unknown. | [patch](patches/opencode2-glm-flash-delivery-shared-skill-2.patch) |
| opencode2-glm-flash-delivery-finish-card-2 | failed | 119.294 | Patch delivered within 150 seconds, but the native JSON stream omitted the final response token record present in the saved session. Complete usage is unknown. | [patch](patches/opencode2-glm-flash-delivery-finish-card-2.patch) |
| opencode2-kimi-k3-delivery-finish-card-2 | failed | 53.029 | Patch delivered within 150 seconds, but the native JSON stream omitted the final response token record present in the saved session. Complete usage is unknown. | [patch](patches/opencode2-kimi-k3-delivery-finish-card-2.patch) |
| opencode2-kimi-k3-delivery-shared-skill-2 | failed | 67.375 | Patch delivered within 150 seconds, but the native JSON stream omitted the final response token record present in the saved session. Complete usage is unknown. | [patch](patches/opencode2-kimi-k3-delivery-shared-skill-2.patch) |
