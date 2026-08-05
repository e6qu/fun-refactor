# API contracts — the invariant when the language changes

A refactoring preserves *behaviour*. A translation preserves a *signature*. Rewriting a
service in another language has to preserve something else again, and it is the only
thing anyone outside the repository can see: the **contract**.

```
                 what must not change              what is free to change
refactoring      what the code does                how it is written
translation      the function's signature          the language
contract rewrite the HTTP contract                 the language, the framework,
                                                   the internal structure, the
                                                   function signatures, all of it
```

That last row is the useful one and the least served by tooling. A caller does not
import your functions. It sends `PATCH /posts/42` with a JSON body and expects `204`.
Everything a refactoring tool normally protects — call sites, imports, symbol
resolution — is irrelevant, and the one thing that matters is not in the type system of
either language.

## What an HTTP contract is made of

Six things, and they do not all travel together:

| | Example | Where it lives |
| --- | --- | --- |
| **URL template** | `/posts/{post_id}` | the *file path*, in Next.js |
| **Method** | `PATCH` | the exported function's name |
| **Path parameters** | `post_id: str` | the bracketed path segment |
| **Request body schema** | `{ title?: string, content?: string }` | a zod schema, an `interface`, or nothing |
| **Response body schema** | the post, or an error | usually nowhere |
| **Status codes** | `204`, `403`, `422` | the returned object |

The first three are *addressing* and the last three are *shape*. This tool carries the
addressing half exactly and the shape half only partly, and the whole point of this
document is to say which is which — because a rewrite that gets addressing right and
shape wrong looks finished.

## Why OpenAPI is the pivot, and why it is asymmetric

**FastAPI derives OpenAPI from the code.** The path decorator gives the URL and method,
the annotated parameters give the path and query parameters, a Pydantic model gives the
body schema, and `/openapi.json` falls out. The contract is not a document somebody
maintains; it is a projection of the types.

**Next.js does not.** The App Router has no built-in equivalent. The contract is
implicit in `route.ts`, and where it is written down at all it is in a zod schema, a
`next-swagger-doc` annotation, or a hand-kept YAML file that drifts.

So the two directions are not mirror images:

- **Next.js → FastAPI** *gains* a machine-readable contract where none existed. That is
  worth a great deal, and it is also the trap: the generated OpenAPI is exactly as
  complete as what carried across, and it does not look incomplete.
- **FastAPI → Next.js** *loses* one. Nothing in the target will regenerate it, so it
  would have to be exported before the rewrite and asserted against afterwards. This
  tool does not do that direction at all.

## What `fr translate <route> fastapi` preserves

Run against `app/api/posts/[postId]/route.ts` from
[shadcn-ui/taxonomy](https://github.com/shadcn-ui/taxonomy) (see
`tests/corpus/PROVENANCE.md`):

| Contract element | Carried? | How |
| --- | --- | --- |
| URL template | **yes** | `app/api/posts/[postId]/` → `/posts/{post_id}` |
| Catch-all segment | **yes** | `[...path]` → `{path:path}`, which matches slashes |
| Method | **yes** | `export async function PATCH` → `@router.patch` |
| Path parameters | **yes** | typed `str`, and the name takes Python's convention |
| Request body schema | **yes** | an exported `interface` **or a zod schema** becomes a Pydantic `BaseModel` |
| Response body schema | **no** | Next.js does not declare one and neither does the output |
| Status codes | **carried into the code, not into the contract** | see below |

### The URL is the path, and that is the whole trick

`app/api/posts/[postId]/route.ts` serves `/posts/{post_id}`, and **nothing inside the
file says so.** No content-only translation can recover it, however well it reads
TypeScript. This is the one part of the job that requires reading the tree rather than
the text, and it is most of the value.

`[...path]` is a catch-all: it matches across slashes. FastAPI spells that
`{path:path}`, and a translation emitting `{path}` would produce a service that answers
a strictly smaller set of URLs than the one it replaced — silently, and only for the
requests with a slash in them.

### The status codes: right behaviour, wrong document

This is the sharp edge, and the tool now reports it. The taxonomy route returns `204`,
`403`, `422` and `500`. All four carry into the Python and the endpoint behaves
correctly. But FastAPI builds its OpenAPI from the **decorator**:

```python
@router.delete("/posts/{post_id}")          # documents 200
async def delete(post_id: str, req: Request):
    return Response(status_code=204)        # returns 204
```

A `Response` with its own status changes what the endpoint *does* without changing what
it *says it does*. The rewrite is behaviour-preserving and contract-shrinking at the
same time, and every test you have will pass. The fix is to declare it:

```python
@router.delete("/posts/{post_id}", status_code=204,
               responses={403: {"description": "not yours"},
                          422: {"description": "invalid"}})
```

The tool does not write that, because which status is the *success* one is a judgement
about the endpoint rather than a fact about the syntax. It reports every status it saw
and says what will happen if you leave them where they are.

### Reading zod

Most Next.js applications declare their shapes with zod, not with `interface`. A zod
schema is a *runtime value*, not a type declaration, so nothing that reads declarations
finds it — and left alone it arrives as an ordinary constant, producing a service whose
published contract has no request body in it at all.

The builder chain is read instead:

```ts
const postCreateSchema = z.object({
  title: z.string().min(3).max(128),
  content: z.string().optional(),
  views: z.number().int(),
  tags: z.array(z.string()),
  publishedAt: z.date().nullable(),
})
```

```python
class PostCreate(BaseModel):
    title: str
    content: str | None
    views: int
    tags: list[str]
    published_at: datetime | None
```

A chain is left-nested — `z.string().min(3).optional()` is
`optional(max(min(string)))` — so it is walked to the base call with the modifiers
collected on the way past. `.optional()` and `.nullable()` become `Optional`; `.int()`
picks `int` over `float`.

**The constraints are deliberately dropped.** `.min(3)` is validation, Pydantic spells
it `Field(min_length=3)`, and the two are not the same rule in every case. Guessing one
from the other would be guessing at the part of a contract it is least safe to guess at.
A nested `z.object` is `dict` for the same reason: Python wants it to be its own model,
and naming one would be inventing a name.

## How you would actually check a rewrite

The tool preserves what it can see and reports the rest. It does **not** verify the
contract, and no amount of reading one side can. The check is a comparison:

1. Export the contract from the original — for a Next.js app, that means writing the
   OpenAPI document by hand or from its zod schemas, which is the work most teams have
   already skipped.
2. Rewrite. Read the report: what carried, what did not, and which status codes are
   returned but not declared.
3. Export the contract from the result: `curl localhost:8000/openapi.json`.
4. **Diff them**, and treat every difference as a defect until argued otherwise.

Step 1 is `fr openapi`. It walks the tree, finds every API route, and emits an
OpenAPI 3.1 document from what the source *declares* — as JSON, or as YAML with
`--yaml`, which is what a contract kept beside the code is usually written in:

```sh
fr openapi --yaml > before.yaml   # from the Next.js tree
# … rewrite, finish the handlers, run it …
curl -s localhost:8000/openapi.json > after.json
diff <(yq -P -S . before.yaml) <(yq -P -S . after.json)
```

Paths, methods and path parameters are exact, because they come from the tree. Schemas
are as good as what was declared. **Responses are `default` only** — which status an
endpoint returns is a fact about its code rather than its declaration, and writing
`200` for everything would be putting fiction into the file you are about to diff
against, which is worse than an empty entry.

Everything it could not settle is printed beside the document rather than guessed at,
because a baseline that quietly invents an entry is the worst possible outcome: the
diff comes out clean and the contract still shrank.

## A worked example: the pet store

`tests/petstore/` is a Next.js App Router API with eight route files and thirteen
operations, and it is there to be run rather than read about. Every figure below comes
from running the tool over it; the generated page is `docs/contract.html`.

It has one of every shape a CRUD API has, because the shapes are where the difficulty
is:

| Route file | URL | What it is |
| --- | --- | --- |
| `app/api/pets/route.ts` | `/pets` | a **collection**: `GET` with a query, `POST` with a body |
| `app/api/pets/[petId]/route.ts` | `/pets/{pet_id}` | a **member**: `GET`, `PATCH`, `DELETE` |
| `app/api/pets/[petId]/photos/route.ts` | `/pets/{pet_id}/photos` | a **sub-collection** |
| `app/api/pets/[petId]/photos/[photoId]/route.ts` | `/pets/{pet_id}/photos/{photo_id}` | a **sub-member**: two path parameters |
| `app/api/pets/[petId]/status/route.ts` | `/pets/{pet_id}/status` | a sub-resource **replaced whole**: `PUT` |
| `app/api/pets/search/route.ts` | `/pets/search` | an **action**, which is not CRUD |
| `app/api/stores/[storeId]/inventory/route.ts` | `/stores/{store_id}/inventory` | an **aggregate**, under a second root |
| `app/api/files/[...path]/route.ts` | `/files/{path:path}` | a **catch-all** |

### The steps, in order

```sh
fr openapi --yaml > contract.yaml          # 1. the baseline, before touching anything
fr translate app/api/pets/route.ts fastapi # 2. one route at a time, reading each report
#                                            3. finish the handlers by hand
curl -s localhost:8000/openapi.json > after.json
#                                            4. diff, and argue about every difference
```

Step 1 is the one teams skip, and skipping it is what makes the rest unfalsifiable. A
rewrite with no baseline cannot be shown to have preserved anything.

### What has to be read that nobody declared

Next.js declares none of the contract. Every element is inferred from somewhere else,
and each one is a different kind of reading:

- **The URL is the file's path.** `app/api/pets/[petId]/route.ts` serves
  `/pets/{pet_id}`, and **nothing inside the file says so**. No content-only translation
  can recover it, however well it reads TypeScript. This is most of the value.
- **`[...path]` is a catch-all**, matching across slashes. FastAPI spells that
  `{path:path}`; emitting `{path}` produces a service that answers a strictly smaller
  set of URLs than the one it replaced — silently, and only for the requests with a
  slash in them.
- **The method is the exported function's name.** `export async function PATCH` is
  `@router.patch`.
- **The request body is a zod schema in another module.** `lib/schemas.ts` here, which
  is where a real application keeps them. Reading only the route file finds nothing, so
  the schemas are collected from anywhere in the tree — and the *link* between an
  operation and its body comes from the `petCreateSchema.parse(json)` call inside the
  handler. A `components` section nothing refers to is not a contract; it says every
  endpoint takes no body.
- **The query parameters are read out of the URL by hand.**
  `req.nextUrl.searchParams.get("species")` is the only declaration there is, so that
  is what is read. Where a handler's statement could not be read at all, the document
  says so: a query parameter inside a statement this tool carried verbatim is missing,
  and a missing one that nothing mentions is the failure this whole document is about.

### The one thing that moves

A Next.js handler receives `(request, context)` and digs the path parameter out of
`context.params.petId`. FastAPI passes it as an argument. So the value arrives by a
different route, and **every use of it moves with the parameter**:

```ts
const pet = await db.pet.findUnique({ where: { id: context.params.petId } })
```
```python
pet = await db.pet.findUnique({"where": {"id": pet_id}})
```

That is the behaviour being redistributed while the URL it answers stays exactly the
same. Rewriting the declaration and leaving `context.params.petId` in the body produces
a file that parses, imports and starts — and answers every request with a `NameError`.

### What the contract comes out as

Thirteen operations, five schemas, every path parameter, the catch-all converter, and
the query parameters the handlers read. What it deliberately does *not* have:

- **Response bodies.** Next.js does not declare one and neither does the output.
- **Status codes.** They carry into the *code* and are reported for the *contract* —
  see below, because this is the sharp edge.
- **Required-ness of a query parameter.** A handler that defaults it and a handler that
  rejects the request without it read the same way, so every query parameter is
  optional in the baseline and the diff will tell you which ones are not.

## What this is not

**Not a proof.** Preserving the addressing half of a contract is a syntactic property
and the tool can be held to it. Preserving the shape half requires knowing what the
handlers do, and the handlers are the part that is carried into the output as comments
for a person to finish.

**Not a migration.** Authentication, database access, middleware ordering and every
library the route imported have no counterpart and are reported, not translated. What
the tool does is the mechanical, error-prone half — the half where a mistyped path
segment costs you a week and a missing `:path` costs you the requests nobody reports.

## See also

- `docs/contract.html` — the pet store, worked, with every figure generated by running
  the tool
- `tests/petstore/` — the source it is worked from
- `CROSS_LANGUAGE.md` — what crosses between languages and what does not
- `src/transpile/nextjs.rs` — the implementation, and what it refuses
- `tests/nextjs.rs`, `tests/corpus.rs` — including the refusal for a `.tsx` file
  containing JSX, because a React component renders a user interface and a FastAPI
  endpoint answers HTTP, and there is no translation between them
