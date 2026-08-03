#ifndef TREE_SITTER_WASM_ASSERT_H_
#define TREE_SITTER_WASM_ASSERT_H_
/* Built with -DNDEBUG, so this is what upstream's header also reduces to. Defining
 * it here as well avoids the duplicate `__assert_fail` its non-NDEBUG branch emits
 * into every translation unit that includes it. */
#define assert(e) ((void)0)
#endif
