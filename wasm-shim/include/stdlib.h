/* Shares tree-sitter-language's include guard: whichever of the two is reached first
 * wins and the other is skipped. Both declare the same functions compatibly, and the
 * definitions are in the Rust shim either way. See fr_shim.h. */
#ifndef TREE_SITTER_WASM_STDLIB_H_
#define TREE_SITTER_WASM_STDLIB_H_
typedef __SIZE_TYPE__ size_t;
#ifndef NULL
#define NULL ((void *)0)
#endif
void *malloc(size_t size);
void *calloc(size_t count, size_t size);
void *realloc(void *ptr, size_t size);
void free(void *ptr);
_Noreturn void abort(void);
_Noreturn void exit(int status);
#endif
