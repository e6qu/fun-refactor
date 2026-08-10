/* Everything the grammars call, declared before anything else is seen.
 *
 * Force-included by tools/build-wasm.sh under a name no other header uses. That
 * matters: tree-sitter-language ships its own minimal wasm headers and cc-rs puts
 * them on the include path with `-I`, which beats `-isystem`, so a file named
 * `ctype.h` here would simply never be read. Theirs declares `isprint` and nothing
 * else, while the markdown scanner calls `isdigit`, `towlower` and `strcmp` without
 * including any header at all — C99 removed implicit declarations and clang enforces
 * it.
 *
 * Implementations live in src/wasm_libc.rs.
 */
#ifndef FR_WASM_SHIM_H
#define FR_WASM_SHIM_H

typedef __SIZE_TYPE__ fr_size_t;

#ifndef NULL
#define NULL ((void *)0)
#endif

/* tree-sitter-language's own wasm headers already declare malloc/calloc/realloc/
 * free/abort, the `mem*` family, `strlen`, `strncmp`, and the four wide classes
 * iswalpha/iswalnum/iswdigit/iswspace — as `static inline`, so re-declaring any of
 * them is an error instead of a duplicate. Only what they leave out is here. */

_Noreturn void exit(int status);
int strcmp(const char *a, const char *b);
char *strncpy(char *dest, const char *src, fr_size_t n);

int isalpha(int c);
int isalnum(int c);
int isdigit(int c);
int isxdigit(int c);
int islower(int c);
int isupper(int c);
int isspace(int c);
int ispunct(int c);
int toupper(int c);
int tolower(int c);

int iswxdigit(int c);
int iswlower(int c);
int iswupper(int c);
int iswpunct(int c);
int iswblank(int c);
int towupper(int c);
int towlower(int c);

#endif /* FR_WASM_SHIM_H */
