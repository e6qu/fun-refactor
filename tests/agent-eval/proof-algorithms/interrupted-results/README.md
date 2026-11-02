# Why the first algorithm batch stopped

Five fact-comparison sessions started. Three timed out without a patch.
Kimi delivered one patch in 104.242 seconds after checking and saving both proofs
and passing the Lean package build. The fifth session hit the workstation's
1 GiB memory limit. The remaining three fact slots and all twelve presentation
slots stayed unstarted. The [summary](summary.json) preserves all these states.

The inner monitor missed OpenCode's private server because it used a separate
process group. A non-model health request reproduced that separation.
The outer guard still enforced the workstation limit, but the recorded inner
memory and CPU totals are incomplete even for the earlier finished sessions.
We cannot identify the exact allocation behind the peak from the retained data.

The interrupted session also exposed two repairable defects:

- Buffered output disappeared when the outer guard killed the controller.
  Reading the existing local conversation recovered the last commands without
  resuming the agent. The runner now flushes each received block.
- Fr rejected induction over two inputs before Lean ran. A small example using
  `induction a, b using euclid.induct` passes pinned Lean. The reader repair and
  its following-declaration checks are being qualified on GitHub.

The repaired monitor counts detached descendants, stops observed workers when
their CLI exits, and checks process start identities before cleanup. Focused
tests also check departed-worker CPU accounting and retained partial output.

The next hosted qualification independently replays the original Kimi patch.
Proof acceptance cannot repair the batch's resource accounting or establish
that supplying declarations helped. The original patch stays unchanged.

After qualification, freeze a fresh complete comparison once. Preserve this
batch separately; do not replace its failures or treat its unstarted slots as
proof failures. The claims, agents, 150-second deadline and resource limits stay
fixed. No graph comparison or automatic-backtracking result exists yet.

Only this reviewed summary and the submitted patch are public. Raw local
records, source snapshots, drafts and the recovered conversation remain private.
