# Three scoped packaging reviews, two models each

This collection assigns eleven requirement areas to three focused questions:
fallback and bounds; explicit policies; objects, iteration and unchanged APIs.
Both configured models receive each question in frozen order, at `low` reasoning
effort and the existing 2,048-token output allowance. The six cells are separate
review attempts, not retries of earlier failures.

`prepare.py` reconstructs the design from the unchanged whole-task source archive.
Every question supplies the complete requirement and grader, plus selected function
signatures and executable bodies. Docstrings and other source remain available
through read-only tools. Prompts are 11,164–12,580 bytes; the narrower questions do
not establish lower total context cost. They request one concise structured result
without a prose preface, and explicit limitations for their assigned areas.

The plan must be committed before calls. It binds the question preparation,
coverage implementation, catalog and prior hosted/workstation admission evidence.
Both executables must match the admitted workstation control. Capture limits remain
120 seconds, 20 CPU seconds, 768 MiB sampled aggregate RSS, 16 MiB disk growth and
1 MiB transcript. Local calls additionally use the user's resource guard.

The collection stops permanently at its first resource-limit failure, at two
consecutive failed reviews, or after all six attempts. Old collections remain
stopped. No candidate code runs during review.

`tools/review-coverage.py` replays the collection and lists assignments, each
model's submission status, findings and limitations. It never marks the task
accepted, even when every submission is complete and all finding lists are empty.
Acceptance requires inspecting the limitations and independently checking claims.
