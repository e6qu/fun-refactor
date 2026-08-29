# API contracts, the invariant when the language changes

A refactoring preserves *behaviour*. A translation preserves a *signature*. A rewrite
into another language must preserve a third thing, the one thing anyone outside the
repository can see: the **contract**.

```
                 what must not change              what is free to change
refactoring      what the code does                how the code reads
translation      the function's signature          the language
contract rewrite the HTTP contract                 the language, the framework,
                                                   the internal structure, the
                                                   function signatures, all of it
```

That last row matters most and tooling serves it least. A caller never imports your
functions. It sends `PATCH /posts/42` with a JSON body and expects `204`. A refactoring
tool guards call sites, imports and symbol resolution, and none of that matters across
this crossing. Neither language's type system holds what does.

## What an HTTP contract holds

An HTTP contract holds six things, and they do not all travel together:

| | Example | Where it lives |
| --- | --- | --- |
| **URL template** | `/posts/{post_id}` | the *file path*, in Next.js |
| **Method** | `PATCH` | the exported function's name |
| **Path parameters** | `post_id: str` | the bracketed path segment |
| **Request body schema** | `{ title?: string, content?: string }` | a zod schema, an `interface`, or nothing |
| **Response body schema** | the post, or an error | usually nowhere |
| **Status codes** | `204`, `403`, `422` | the returned object |

The first three make up the *addressing* half and the last three the *shape* half. The
tool carries addressing exactly and shape only partly. This document says which is
which, because a rewrite that gets addressing right and shape wrong looks finished.

## Why OpenAPI is the pivot, and why it is asymmetric

**FastAPI derives OpenAPI from the code.** The path decorator gives the URL and the
method. The annotated parameters give the path and query parameters. A Pydantic model
gives the body schema, and `/openapi.json` falls out. Nobody maintains that contract as
a document; FastAPI projects it from the types.

**Next.js does not.** The App Router ships no equivalent. `route.ts` implies the
contract without stating it. Where a team writes it down at all, they write it in a zod
schema, a `next-swagger-doc` annotation, or a hand-kept YAML file that drifts.

So the two directions work differently:

- **Next.js → FastAPI** *gains* a machine-readable contract where none existed. The gain
  is large and it is also the trap. The generated OpenAPI covers what carried across and
  no more, and it never looks incomplete.
- **FastAPI → Next.js** *loses* one. Nothing in the target regenerates it, so you would
  export it before the rewrite and assert against it afterwards. This tool does not do
  that direction at all.

## The five frameworks beside Next.js

`fr openapi` reads a Next.js `app/api` tree and a FastAPI router. It also reads five
more, because a service written in any of them declares the same thing.

| Framework | How it says it |
|---|---|
| Express | `app.get("/pets", listPets)`, a method on a router |
| Flask | `@app.route("/pets", methods=["GET"])`, a decorator |
| axum | `Router::new().route("/pets", get(list_pets))`, a chain |
| gin | `r.GET("/pets", listPets)`, a method on a router or a group |
| Spring | `@GetMapping("/pets")`, an annotation, under a class-level prefix |

What they agree about is the pair that matters: a method and a URL, answered by a named
function. Every one of them writes both down, so both are exact.

Path parameters are the one place they diverge in spelling. Express, gin and axum write
`:id`. Flask writes `<int:id>`, where the part before the colon is a converter rather
than the name. Spring writes `{id}`, which is also what a contract writes. Every reader
spells its own into that last form.

What none of them declares is the response schema. A handler returns whatever it
returns, and no annotation says what. The document lists that as undeclared rather than
inventing it.

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
file says so.** No content-only translation recovers it, however well it reads
TypeScript. This one part of the job demands reading the tree instead of the text, and
it carries most of the value.

`[...path]` is a catch-all: it matches across slashes. FastAPI spells that
`{path:path}`. A translation emitting `{path}` builds a service that answers a strictly
smaller set of URLs than the one it replaced. It fails silently, and only for the
requests with a slash in them.

### The status codes: right behaviour, wrong document

The tool now reports this sharp edge. The taxonomy route returns `204`, `403`, `422`
and `500`. All four carry into the Python and the endpoint behaves correctly. But
FastAPI builds its OpenAPI from the **decorator**:

```python
@router.delete("/posts/{post_id}")          # documents 200
async def delete(post_id: str, req: Request):
    return Response(status_code=204)        # returns 204
```

A `Response` with its own status changes what the endpoint *does* without changing what
it *says it does*. The rewrite preserves behaviour and shrinks the contract at once,
and every test you have passes. Declare the statuses to fix it:

```python
@router.delete("/posts/{post_id}", status_code=204,
               responses={403: {"description": "not yours"},
                          422: {"description": "invalid"}})
```

The tool does not write that. Picking the *success* status judges the endpoint rather
than reading its syntax. The tool reports every status it saw and says what happens if
you leave them where they are.

### Reading zod

Most Next.js applications declare their shapes with zod rather than with `interface`. A
zod schema is a *runtime value* and not a type declaration. A reader that walks
declarations finds nothing. Left alone, the schema arrives as an ordinary constant, and
the published contract then carries no request body at all.

The tool reads the builder chain instead:

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

A chain nests to the left: `z.string().min(3).optional()` is
`optional(max(min(string)))`. The reader walks it down to the base call and collects
the modifiers on the way past. `.optional()` and `.nullable()` become `Optional`;
`.int()` picks `int` over `float`.

**The tool drops the constraints deliberately.** `.min(3)` validates, Pydantic spells
it `Field(min_length=3)`, and the two rules diverge in some cases. Guessing one from the
other guesses at the part of a contract that least tolerates a guess. A nested
`z.object` becomes `dict` for the same reason: Python wants its own model there, and
naming one would invent a name.

## How you would check a rewrite

The tool preserves what it can see and reports the rest. It does **not** verify the
contract, and no amount of reading one side ever will. Compare the two sides instead:

1. Export the contract from the original. For a Next.js app, that means writing the
   OpenAPI document by hand or from its zod schemas, work most teams have already
   skipped.
2. Rewrite. Read the report: what carried, what did not, and which status codes the
   code returns without declaring.
3. Export the contract from the result: `curl localhost:8000/openapi.json`.
4. **Diff them**, and treat every difference as a defect until argued otherwise.

`fr openapi` does step 1. It walks the tree, finds every API route, and emits an
OpenAPI 3.1 document from what the source *declares*. It writes JSON, or YAML with
`--yaml`, the form teams usually use for a contract kept beside the code:

```sh
fr openapi --yaml > before.yaml   # from the Next.js tree
# … rewrite, finish the handlers, run it …
curl -s localhost:8000/openapi.json > after.json
diff <(yq -P -S . before.yaml) <(yq -P -S . after.json)
```

Paths, methods and path parameters come out exact, because the tree supplies them.
Schemas go only as far as the declarations went. **Responses are `default` only**: an
endpoint's status lives in its code rather than in its declaration. Writing `200` for
everything puts fiction into the file you are about to diff against. An empty entry
puts none.

The tool prints everything it could not settle beside the document rather than guessing
at it. A baseline that quietly invents an entry is the worst outcome available. The
diff comes out clean and the contract still shrank.

## A worked example: the pet store

`tests/petstore/` holds a Next.js App Router API with eight route files and thirteen
operations. Run it rather than read about it. Every figure below comes from running the
tool over it, and the same run generates the page `docs/contract.html`.

It holds one of every shape a CRUD API has, because the shapes carry the difficulty:

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

Teams skip step 1, and skipping it makes the rest unfalsifiable. Nobody can show that
a rewrite with no baseline preserved anything.

### What the reader has to infer

Next.js declares none of the contract. The tool infers every element from somewhere
else, and each element takes a different kind of reading:

- **The URL is the file's path.** `app/api/pets/[petId]/route.ts` serves
  `/pets/{pet_id}`, and **nothing inside the file says so**. No content-only translation
  recovers it, however well it reads TypeScript. It carries most of the value.
- **`[...path]` is a catch-all**, matching across slashes. FastAPI spells that
  `{path:path}`. Emitting `{path}` produces a service that answers a strictly smaller
  set of URLs than the one it replaced, and it does so silently. The failure shows only
  on requests with a slash in them.
- **The method is the exported function's name.** `export async function PATCH` is
  `@router.patch`.
- **The request body is a zod schema in another module.** `lib/schemas.ts` holds them
  here, where a real application keeps them. Reading only the route file finds nothing,
  so the tool collects schemas from anywhere in the tree. The
  `petCreateSchema.parse(json)` call inside the handler supplies the *link* between an
  operation and its body. Without that link, a `components` section refers to nothing
  and the document says every endpoint takes no body.
- **The handler reads the query parameters out of the URL by hand.**
  `req.nextUrl.searchParams.get("species")` is the only declaration there is, so the
  tool reads that. Where the tool could not read a handler's statement, the document
  says so. A query parameter inside a statement the tool carried verbatim goes missing.
  A gap nothing mentions is the failure this whole document guards against.

### What changes

A Next.js handler receives `(request, context)` and digs the path parameter out of
`context.params.petId`. FastAPI passes it as an argument. The value therefore arrives
by a different route, and **every use of it moves with the parameter**:

```ts
const pet = await db.pet.findUnique({ where: { id: context.params.petId } })
```
```python
pet = await db.pet.findUnique({"where": {"id": pet_id}})
```

The behaviour moves while the URL it answers stays the same. Rewrite the declaration,
leave `context.params.petId` in the body, and the file parses, imports and starts, then
answers every request with a `NameError`.

### What the contract comes out as

The document holds thirteen operations, five schemas, every path parameter, the
catch-all converter, and the query parameters the handlers read. It deliberately leaves
out three things:

- **Response bodies.** Next.js does not declare one and neither does the output.
- **Status codes.** They carry into the *code*, and the tool reports them for the
  *contract*, see below, because they are the sharp edge.
- **Required-ness of a query parameter.** A handler that defaults it and a handler that
  rejects the request without it read the same way. So the baseline marks every query
  parameter optional, and the diff tells you which ones the handlers require.

### Checking the crossing without running anything

Step 4 tells you to run the finished service and diff its `/openapi.json` against the
baseline. Half of that check needs no server: **`fr openapi` reads a FastAPI router
too**, off the decorators and the signatures. The same command therefore answers the
same question about the code the rewrite produced.

```sh
fr openapi --yaml > before.yaml      # the Next.js tree
# … translate every route …
fr openapi --yaml > after.yaml       # the FastAPI router it became
diff before.yaml after.yaml
```

Run it over the pet store: thirteen operations go in and thirteen come out, with every
URL, method, path parameter and query parameter identical. The test suite asserts that,
so a translation that started dropping endpoints would fail the build.

**That the two documents agree does not mean the contract is complete**. Watch the
difference rather than the agreement: both sides can miss the same thing and agree
perfectly. So the baseline says what it could not read:

```
<route file>: N statement(s) did not read; any query parameter
read inside one of them is missing from this document
```

A statement the tool carries whole is a statement it never looked inside, so watch that
count. You can close a gap that announces itself. A silent gap is the one this document
exists to prevent.

The pet store's count ran at two and now reads zero. The first was
`const limit = Number(… ?? "50")`, where `??` had no counterpart in the IR. The second
was `{ where: species ? { species } : {} }`. Every modern TypeScript file uses that
`{ species }` shorthand. Refusing it refused the whole object, and with it the
statement the object sat in.

The tool reads what the source writes down, never what will happen at run time. Three
things escape it: a router mounted under a prefix, a route added at run time, a
dependency that rejects the request. Those need the server, so step 4 stays step 4.

## What this is not

**Not a proof.** Preserving the addressing half of a contract is a syntactic property,
so you can hold the tool to it. Preserving the shape half requires knowing what the
handlers do. The tool carries the handlers into the output as comments for a person to
finish.

**Not a migration.** Authentication, database access, middleware ordering and every
library the route imported have no counterpart. The tool reports them and translates
none of them. It does the mechanical, error-prone half. A mistyped path segment costs
you a week there, and a missing `:path` costs you the requests nobody reports.

## See also

- `docs/contract.html`, the pet store worked through, with every figure coming from a
  run of the tool.
- `tests/petstore/`, the source it works from.
- `CROSS_LANGUAGE.md`, what crosses between languages and what does not.
- `src/transpile/nextjs.rs` and `src/transpile/routes.rs`, the implementation,
  including what each refuses.
- `CLI.md`, and `fr openapi` in it.
- `tests/nextjs.rs` and `tests/corpus.rs`. They cover the refusal for a `.tsx` file
  containing JSX. A React component renders a user interface and a FastAPI endpoint
  answers HTTP, and no translation joins them.
