#include <assert.h>
#include <stdio.h>
#include <string.h>
#include "../src/scanner.c"

typedef struct {
  TSLexer lexer;
  const char *source;
  size_t offset, end;
  bool marked;
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
  input->marked = true;
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
  bool valid[DO_BODY_START + 1] = {false};
  valid[token] = true;
  size_t previous = input->offset;
  input->end = previous;
  input->marked = false;
  bool found = tree_sitter_lean_external_scanner_scan(scanner, &input->lexer, valid);
  input->offset = found ? (input->marked ? input->end : input->offset) : previous;
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

  Input negative_binding = input("    let n := value");
  assert(scan(scanner, &negative_binding, BINDING_START));
  Input subtraction = input("\n    - 1");
  assert(!scan(scanner, &subtraction, LAYOUT_END));
  Input after_subtraction = input("\n    n");
  assert(scan(scanner, &after_subtraction, LAYOUT_END));

  Input negative_block = input("\n    -n");
  assert(scan(scanner, &negative_block, LAYOUT_START));
  assert(negative_block.lexer.lookahead == '-');
  Input done = input("");
  assert(scan(scanner, &done, LAYOUT_END));

  const char *prefixes[] = {"elseValue", "catchValue", "finallyValue", "else_case", "catch2", "finally!", "else?"};
  for (size_t i = 0; i < sizeof(prefixes) / sizeof(prefixes[0]); i++) {
    Input prefix_binding = input("let n := value");
    assert(scan(scanner, &prefix_binding, BINDING_START));
    char line[64];
    snprintf(line, sizeof(line), "\n%s", prefixes[i]);
    Input prefix_body = input(line);
    assert(scan(scanner, &prefix_body, LAYOUT_END));
  }

  Input suffixed_name = input("let?");
  assert(!scan(scanner, &suffixed_name, BINDING_START));

  Input statement_body = input("do pure 1");
  assert(scan(scanner, &statement_body, DO_BODY_START));
  assert(statement_body.lexer.lookahead == 'd' && scanner->depth == 0);
  Input do_prefix = input("doSomething");
  assert(!scan(scanner, &do_prefix, DO_BODY_START));

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
