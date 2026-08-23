#include "tree_sitter/parser.h"

#include <wctype.h>

enum TokenType {
    DESCENDANT_OP,
    PSEUDO_CLASS_SELECTOR_COLON,
    ERROR_RECOVERY,
    CONCAT,
    MAP_OPEN,
    MODULO,
};

static inline void advance(TSLexer *lexer) { lexer->advance(lexer, false); }

static inline void skip(TSLexer *lexer) { lexer->advance(lexer, true); }

// Read past an interpolation, from its opening brace to the one that closes it. The
// braces inside one belong to it, and neither ends the statement it sits in.
static void skip_interpolation(TSLexer *lexer) {
    unsigned depth = 0;
    do {
        if (lexer->lookahead == '{') {
            depth++;
        } else if (lexer->lookahead == '}') {
            depth--;
        }
        advance(lexer);
    } while (depth > 0 && !lexer->eof(lexer));
}

// Whether the bracket just consumed opens a map. A map holds `key: value` pairs and a
// list holds values, so a colon of the bracket's own tells them apart. Sass reads that
// far before it decides, and so does this.
static bool opens_a_map(TSLexer *lexer) {
    unsigned depth = 0;
    for (;;) {
        if (lexer->eof(lexer)) {
            return false;
        }
        switch (lexer->lookahead) {
            case ':':
                if (depth == 0) {
                    return true;
                }
                break;
            case '(':
            case '[':
                depth++;
                break;
            case ']':
                depth--;
                break;
            case ')':
                if (depth == 0) {
                    return false;
                }
                depth--;
                break;
            case '\'':
            case '"': {
                int32_t quote = lexer->lookahead;
                advance(lexer);
                while (!lexer->eof(lexer) && lexer->lookahead != quote) {
                    if (lexer->lookahead == '\\') {
                        advance(lexer);
                    }
                    advance(lexer);
                }
                break;
            }
            case '#':
                advance(lexer);
                if (lexer->lookahead == '{') {
                    skip_interpolation(lexer);
                }
                continue;
            case ';':
            case '{':
            case '}':
                return false;
            default:
                break;
        }
        advance(lexer);
    }
}

// Whether the statement that starts here reaches a block. A `{` makes the colon before
// it the one that opens a pseudo class, and a `;` or a `}` makes it a declaration's.
static bool reaches_a_block(TSLexer *lexer) {
    int32_t previous = 0;
    for (;;) {
        if (lexer->lookahead == ';' || lexer->lookahead == '}' || lexer->eof(lexer)) {
            return false;
        }
        if (lexer->lookahead == '{') {
            if (previous != '#') {
                return true;
            }
            skip_interpolation(lexer);
            previous = 0;
            continue;
        }
        previous = lexer->lookahead;
        advance(lexer);
    }
}

void *tree_sitter_scss_external_scanner_create() { return NULL; }

void tree_sitter_scss_external_scanner_destroy(void *payload) {}

void tree_sitter_scss_external_scanner_reset(void *payload) {}

unsigned tree_sitter_scss_external_scanner_serialize(void *payload, char *buffer) { return 0; }

void tree_sitter_scss_external_scanner_deserialize(void *payload, const char *buffer, unsigned length) {}

bool tree_sitter_scss_external_scanner_scan(void *payload, TSLexer *lexer, const bool *valid_symbols) {
    if (valid_symbols[ERROR_RECOVERY]) {
        return false;
    }

    if (valid_symbols[CONCAT]) {
        if (iswalnum(lexer->lookahead) || lexer->lookahead == '#' || lexer->lookahead == '-' ||
            lexer->lookahead == '\\') {
            lexer->result_symbol = CONCAT;
            if (lexer->lookahead == '#') {
                lexer->mark_end(lexer);
                advance(lexer);
                return lexer->lookahead == '{';
            }
            return true;
        }
    }

    if (iswspace(lexer->lookahead) && valid_symbols[DESCENDANT_OP]) {
        lexer->result_symbol = DESCENDANT_OP;

        skip(lexer);
        while (iswspace(lexer->lookahead)) {
            skip(lexer);
        }
        lexer->mark_end(lexer);

        if (lexer->lookahead == '#' || lexer->lookahead == '.' || lexer->lookahead == '[' || lexer->lookahead == '-' ||
            lexer->lookahead == '*' || lexer->lookahead == '&' || iswalnum(lexer->lookahead)) {
            return true;
        }

        if (lexer->lookahead == ':') {
            advance(lexer);
            if (iswspace(lexer->lookahead)) {
                return false;
            }
            return reaches_a_block(lexer);
        }
    }

    if (valid_symbols[PSEUDO_CLASS_SELECTOR_COLON]) {
        while (iswspace(lexer->lookahead)) {
            skip(lexer);
        }
        if (lexer->lookahead == ':') {
            advance(lexer);
            if (lexer->lookahead == ':') {
                return false;
            }
            lexer->mark_end(lexer);
            if (!reaches_a_block(lexer)) {
                return false;
            }
            lexer->result_symbol = PSEUDO_CLASS_SELECTOR_COLON;
            return true;
        }
    }

    if (valid_symbols[MAP_OPEN] || valid_symbols[MODULO]) {
        bool separated = false;
        while (iswspace(lexer->lookahead)) {
            separated = true;
            skip(lexer);
        }
        if (separated && valid_symbols[MODULO] && lexer->lookahead == '%') {
            advance(lexer);
            lexer->mark_end(lexer);
            lexer->result_symbol = MODULO;
            return true;
        }
        if (separated && valid_symbols[MAP_OPEN] && lexer->lookahead == '(') {
            advance(lexer);
            lexer->mark_end(lexer);
            if (opens_a_map(lexer)) {
                lexer->result_symbol = MAP_OPEN;
                return true;
            }
        }
    }

    return false;
}
