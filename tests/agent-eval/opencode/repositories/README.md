# Pinned repository inputs

These archives retain complete upstream repositories, including their licenses.
`provenance.json` records the Git revision, archive digest, file count and expanded bytes.
Download the same revision with `gh api repos/OWNER/REPO/tarball/REVISION`.
The rehearsal removes the archive's top-level directory and validates every remaining path.
It refuses links, duplicate files, mixed roots and archive expansion beyond its fixed limits.

The three projects exercise expiration, signer fallback and retry control flow.
The two initial development rubrics cover cachetools and itsdangerous.
The third archive's inventory was checked before the implementation commit;
its code and factual rubric will provide a subsequent integration check.
This selection does not establish independence from model training data.

Archives keep upstream text out of fr's own source tree and avoid duplicating expanded snapshots.
No upstream source executes in local explanation grades. CI runs the reviewed behavior controls.
