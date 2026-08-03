#ifndef TREE_SITTER_WASM_STDIO_H_
#define TREE_SITTER_WASM_STDIO_H_
/* Nothing linked here reaches a print. Declaring the type keeps a header that
 * mentions FILE compiling; a call would fail to link, which is the honest outcome. */
typedef struct _FR_FILE FILE;
#endif
