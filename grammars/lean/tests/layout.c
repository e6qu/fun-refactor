#include <assert.h>
#include <stdio.h>
#include <string.h>
#include "../src/scanner.c"

typedef struct {
  TSLexer lexer;
  const char *source;
  size_t offset, end;
} Input;

static void advance(TSLexer *lexer, bool skip) {
  (void)skip;
  Input *input = (Input *)lexer;
  if (input->source[input->offset]) input->offset++;
  lexer->lookahead = input->source[input->offset];
}
static void mark_end(TSLexer *lexer) {
  Input *input = (Input *)lexer;
  input->end = input->offset;
}
static uint32_t column(TSLexer *lexer) {
  Input *input = (Input *)lexer;
  size_t start = input->offset;
  while (start && input->source[start - 1] != '\n') start--;
  return input->offset - start;
}
static bool eof(const TSLexer *lexer) {
  return lexer->lookahead == 0;
}
static Input input(const char *text) {
  Input result = {0};
  result.lexer.advance = advance;
  result.lexer.mark_end = mark_end;
  result.lexer.get_column = column;
  result.lexer.eof = eof;
  result.source = text;
  result.lexer.lookahead = text[0];
  return result;
}
static bool scan(Scanner *scanner, Input *input, enum TokenType token) {
  bool valid[6] = {false};
  valid[token] = true;
  size_t previous = input->offset;
  input->end = previous;
  bool found = tree_sitter_lean_external_scanner_scan(scanner, &input->lexer, valid);
  input->offset = found ? input->end : previous;
  input->lexer.lookahead = input->source[input->offset];
  return found;
}

int main(void) {
  Scanner *scanner = tree_sitter_lean_external_scanner_create();
  Input outer = input("  have bound := f");
  assert(scan(scanner, &outer, LAYOUT_START));
  outer.offset = 2; outer.lexer.lookahead = 'h';
  assert(scan(scanner, &outer, BINDING_START));
  assert(scanner->depth == 2 && scanner->bindings[1] == 1);
  Input wrapped = input("\n    (1)");
  assert(!scan(scanner, &wrapped, LAYOUT_END));
  Input next = input("\n  exact bound");
  assert(scan(scanner, &next, LAYOUT_END));
  assert(scanner->depth == 1);
  assert(scan(scanner, &next, LAYOUT_SEMICOLON));

  tree_sitter_lean_external_scanner_deserialize(scanner, "", 0);
  Input binding = input("    let n := f");
  assert(scan(scanner, &binding, BINDING_START));
  Input nested_body = input("\n    n + 1");
  assert(scan(scanner, &nested_body, LAYOUT_END));

  Input inline_binding = input("let n := 1; n");
  assert(scan(scanner, &inline_binding, BINDING_START));
  Input separator = input("; n");
  assert(scan(scanner, &separator, LAYOUT_END));
  assert(separator.offset == 0);

  Input name = input("letter");
  assert(!scan(scanner, &name, BINDING_START));
  Input have = input("have h := by rfl");
  assert(scan(scanner, &have, BINDING_START));
  char state[1024];
  unsigned length = tree_sitter_lean_external_scanner_serialize(scanner, state);
  Scanner *restored = tree_sitter_lean_external_scanner_create();
  tree_sitter_lean_external_scanner_deserialize(restored, state, length);
  assert(restored->depth == scanner->depth && restored->bindings[0] == 1);
  for (unsigned truncated = 0; truncated < length; truncated++) {
    tree_sitter_lean_external_scanner_deserialize(restored, state, truncated);
    assert(restored->depth == 0);
  }
  tree_sitter_lean_external_scanner_destroy(restored);
  tree_sitter_lean_external_scanner_destroy(scanner);
  puts("Wrapped arguments, following steps, nested term bodies, explicit separators and scanner state: passed");
}
