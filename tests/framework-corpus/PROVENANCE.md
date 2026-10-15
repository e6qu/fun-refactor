# Framework source corpus

For “which URL serves this item request?”, reading an individual handler can miss the router prefix. These real source files test that reading step. Recognizing those declarations does not establish the endpoint’s runtime behavior.

A framework supplies conventions such as routing requests to handlers.
This corpus collects source examples for recognizing those conventions.
For an item URL, inspect both the router prefix and handler path in the original source.
The provenance below identifies that source, rather than certifying the endpoint's runtime behavior.

This directory contains unmodified source for framework semantic-model tests.
The files serve as source evidence and do not enter the cross-language translation corpus.

## fastapi/ - `fastapi/full-stack-fastapi-template`

- Source: https://github.com/fastapi/full-stack-fastapi-template
- Commit: `750d3d0bc6dfece4dec2d6ef8c3ff7e64f72545d`
- License: MIT, Copyright (c) 2019 Sebastián Ramírez

| File | SHA-256 | Origin |
| --- | --- | --- |
| `items.py` | `7f0fa55d1f7b02188c4abd4f88aa6db92fe03de38258b765ed050ec72032b410` | `backend/app/api/routes/items.py` |

The file supplies a prefixed `APIRouter` and five real operations.
