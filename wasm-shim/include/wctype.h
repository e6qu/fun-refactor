#ifndef TREE_SITTER_WASM_WCTYPE_H_
#define TREE_SITTER_WASM_WCTYPE_H_
typedef __WCHAR_TYPE__ wchar_t;
typedef int wint_t;
/* Declarations, not definitions: the Rust shim supplies the bodies, so a translation
 * unit that reaches this file instead of tree-sitter-language's still links. */
int iswalpha(int c);
int iswalnum(int c);
int iswdigit(int c);
int iswxdigit(int c);
int iswlower(int c);
int iswupper(int c);
int iswspace(int c);
int iswpunct(int c);
int iswblank(int c);
int towupper(int c);
int towlower(int c);
#endif
