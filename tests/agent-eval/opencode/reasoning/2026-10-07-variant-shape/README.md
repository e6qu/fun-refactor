# Saved variants belong inside the model object

One guarded workstation control caught a validator error before any live review.
The client sent `reasoning_effort: low` and `max_tokens: 2048` in both loopback
requests, read the source and submitted one terminal answer. OpenCode stored the
variant in `user.info.model.variant`. The initial validator wrongly expected
`user.info.variant` and recorded a failed control.

The unchanged process limits held: 534 MiB sampled aggregate RSS, 1.94 CPU
seconds and 5.04 seconds elapsed. The client and public catalog already had
passing hosted and workstation memory controls. This was one model-free case;
the full transport matrix remained on GitHub. Its first queued run was cancelled
after this local finding, before it executed any job.

The complete capture, initial validator and invocation helper are hash-bound in
`manifest.json`. The original failed record is unchanged. The regression test
reproduces the old rejection and checks the corrected validator against the same
raw request, provider exchanges, events and independent export. It also rejects a
changed nested variant. This is transport evidence, not a successful live review.
