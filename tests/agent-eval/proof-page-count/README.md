# Can each agent deliver the complete page-count proof in 300 seconds?

GLM Flash and Kimi K3 each get two fresh sessions, four in total, on the unchanged
page-count theorem with fr. For example, eleven items at five per page need three pages.

## Why this study, and why 300 seconds

The [first page-count attempts](results-150s/diagnosis.md) all ran out of time at 150 seconds.
The [division-fact sessions](../proof-division-facts/local-results/summary.json) then showed
both agents finding and applying the needed facts. They lost on time instead:

- tools used only 6–33 seconds of each session; the rest was waiting on model replies;
- Kimi saved two correct proofs at about 112 seconds, then missed the deadline on separate
  build and diff turns;
- one small lemma used 55–75% of the old 150-second budget.

So this study changes two things, both decided before any calls:

1. **300 seconds** per complete session: preparation, replies, tools, checks, delivery
   and accounting. The 10-second finishing reserve and all other limits are unchanged.
   Earlier studies keep their 150-second outcomes.
2. **One-call delivery:** `fr proof save ... --deliver change.patch` saves the checked
   proof, runs the supplied package check from `.fr/checks.json` and writes the patch.

## Success and decision

A success is an independently accepted patch delivered within 300 seconds, with an
accepted fr proof write and a passing package check. A correct proof from a late session
stays failed.

| Result | Next step |
| --- | --- |
| Each agent has at least one success | Record which fr actions contributed; M3 still needs its other cases |
| An agent has none | Break down where its time went and find the first unresolved proof step |

Two sessions per agent establish no success rate. A service, collection or unexpected
resource failure blocks a conclusion for that session; no slot is replaced.

Publish only submitted patches and reviewed summaries. Prompts, transcripts, drafts
and session exports stay local.
