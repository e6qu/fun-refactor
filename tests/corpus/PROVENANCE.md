# Vendored test corpus

Unmodified source files from public repositories, kept here so the translation tests
run against code somebody actually shipped rather than against fixtures written to
pass. They are **test data only**: nothing here is compiled, linted, packaged or
shipped, and no build target references this directory.

Both projects are MIT-licensed, which permits redistribution with the notice below.

## fastapi/ — `fastapi/full-stack-fastapi-template`

- Source: https://github.com/fastapi/full-stack-fastapi-template
- Commit: `750d3d0bc6dfece4dec2d6ef8c3ff7e64f72545d`
- License: MIT — Copyright (c) 2019 Sebastián Ramírez
- Path in source: `backend/app/`

| File | SHA-256 | Origin |
| --- | --- | --- |
| `crud.py` | `f69d79e858ee22cee7792494b0df0370afaed20439ed16bbf6f70767926254d1` | `backend/app/crud.py` |
| `models.py` | `1b7384d0dc779cca9ebc9d2b05ccd1864988c2043a3692384ff7a731819050f0` | `backend/app/models.py` |
| `security.py` | `adb90882929ab33c238e2e9fa81b03fa0364c1a8cce0e9e70b98c68c321f468f` | `backend/app/core/security.py` |

Chosen for what they contain: `crud.py` for keyword-only parameters (`def f(*, session, …)`),
`models.py` for nineteen typed SQLModel records, `security.py` for typed helpers over
foreign library types.

## nextjs/ — `shadcn-ui/taxonomy`

- Source: https://github.com/shadcn-ui/taxonomy
- Commit: `298a8857c7128a0d121e7f699dfd729f23b3966d`
- License: MIT — Copyright (c) 2022 shadcn
- Path in source: `app/api/`

| File | SHA-256 | Origin |
| --- | --- | --- |
| `app/api/posts/[postId]/route.ts` | `0793f8b58bd592378756a6763caf791d336d3232a13ad157dfa56b14f5e1a2e3` | same |
| `app/api/posts/route.ts` | `31f98edf51e763bfcd3f119a82593536f8895d54320aebcbdc1212502f798edf` | same |
| `app/api/webhooks/stripe/route.ts` | `c6984f90fdd2aeac6edb262f7dbead6502566fdeb563901168890b846226edd6` | same |

The directory layout is reproduced exactly, because a Next.js route's URL **is** its
path: `app/api/posts/[postId]/route.ts` serves `/posts/{post_id}` and nothing inside
the file says so. Flattening these into a fixtures directory would delete the thing
under test.

## Refreshing

Re-copy from the pinned commit and update both the commit and the checksums above:

```sh
git clone --depth 1 <url> /tmp/src && git -C /tmp/src rev-parse HEAD
shasum -a 256 tests/corpus/**/*
```
