#ifndef TREE_SITTER_WASM_CTYPE_H_
#define TREE_SITTER_WASM_CTYPE_H_
static inline int isprint(int c) { return c >= 0x20 && c <= 0x7E; }
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
#endif
