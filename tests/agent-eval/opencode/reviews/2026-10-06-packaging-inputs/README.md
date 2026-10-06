# Complete packaging task review inputs

This bundle prepares two new reviews: one by configured Kimi K3 and one by
configured GLM 5.3 Flash. Neither review is frozen for execution or started.
The earlier collection remains stopped after its memory failure.

The initial packet contains the complete requirement, all eight private grader
cases, the reference `SpecifierSet.filter` declaration, and before/after file
identities. It provides 7,525 source bytes in an 11,382-byte prompt. Both original
and reference source trees remain available through ordinary reads or `fr`.
The reference patch and public test are also available. No source is executed.

Run `prepare.py check` to reconstruct these inputs from the current candidate
pack and verify every retained byte. The design check in
`tools/prepare-terminal-reviews.py` validates source identities and prompt limits.
The packet reuses a previously selected `fr` declaration only while the complete
source file still matches its recorded hash.

Before these reviews can support a code-change pilot:

- Hosted memory controls and one guarded workstation control must establish
  room below the unchanged capture limit. No live calls precede that check.
- Freeze the actual runtime and executable identities in a new collection and
  commit its plan before either model runs. Use the configured client locally;
  do not copy credentials into GitHub or repository artifacts.
- Retain each result, including failures. Stop at the first resource-limit
  failure on this workstation; do not resume an older stopped collection.
- Check each reviewer’s stated coverage against the whole requirement below.
  An empty findings list alone does not accept the task.
- Verify concrete findings by executing distinguishing repairs and inputs on
  GitHub. Keep unsupported claims separate from demonstrated failures.
- Require the reference to pass every private case and the public example.
  The unchanged baseline must fail the public example. Each retained incorrect
  repair must fail its declared private cases.

The acceptance record must address each area, with evidence or an explicit gap:

| Requirement area | Current grader cases |
| --- | --- |
| Complete intersection and out-of-range finals | `fallback` |
| Matching final and postrelease suppression | `matching-finals` |
| Constructor policy and call override precedence | `explicit` |
| Prerelease bounds and exclusions | `bounds-and-exclusions` |
| Empty sets, empty inputs and Version instances | `empty-and-objects` |
| Original identity, order and repeated occurrences | `duplicate-final-identity`, shared `check` |
| One-shot input consumption | Shared `check`, `duplicate-final-identity` |
| Unchanged containment policy | `contains-policy` |
| Unchanged individual Specifier behavior | `individual-filter` |

These cases and reviews do not prove all possible inputs correct. The task is
author-selected, and prior exposure to these repositories is unknown.

If accepted, the first code-change pilot has four attempts: two models crossed
with ordinary tools or ordinary tools plus `fr`. Both arms receive the same
public test and baseline failure. They cannot execute tests during the attempt;
GitHub grades their exact submissions afterward. Using `fr` is optional.
Report correctness, tool adoption, calls, disclosed context, elapsed time and
every failure. Four attempts cannot establish general efficiency gains.
