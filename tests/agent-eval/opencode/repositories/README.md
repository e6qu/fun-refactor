# Pinned repository inputs

These archives retain complete upstream repositories, including their licenses.
`provenance.json` records the Git revision, archive digest, file count and expanded bytes.
Download the same revision with `gh api repos/OWNER/REPO/tarball/REVISION`.
The rehearsal removes the archive's top-level directory and validates every remaining path.
It refuses links, duplicate files, mixed roots and archive expansion beyond its fixed limits.

The three projects exercise expiration, signer fallback and retry control flow.
The two initial development rubrics cover cachetools and itsdangerous.
The third archive's inventory was checked before the implementation commit;
its code and factual rubric provide a subsequent integration check.
This selection does not establish independence from model training data.

The original tenacity archive contains a README symlink and fails the runner's regular-file rule.
`tenacity-regular.tar.gz` replaces only that link with the existing `doc/source/index.rst` contents.
The provenance file records both archive hashes and the transformation's source, size and digest.
All runtime source bytes remain unchanged. The original archive remains available for comparison.

Archives keep upstream text out of fr's own source tree and avoid duplicating expanded snapshots.
No upstream source executes in local explanation grades. CI runs the reviewed behavior controls.
