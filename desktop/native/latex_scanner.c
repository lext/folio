// Folio, MIT license. Stateless compatibility scanner for tree-sitter-latex 0.1.0.
// TSLexer is the public Tree-sitter 0.25 scanner ABI. Token order must match
// the pinned grammar's EXTERNAL_TOKEN_COUNT and ts_external_token_* constants.
#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>

typedef struct TSLexer TSLexer;
struct TSLexer {
    int32_t lookahead;
    uint16_t result_symbol;
    void (*advance)(TSLexer *, bool);
    void (*mark_end)(TSLexer *);
    uint32_t (*get_column)(TSLexer *);
    bool (*is_at_included_range_start)(const TSLexer *);
    bool (*eof)(const TSLexer *);
    void (*log)(const TSLexer *, const char *, ...);
};

static const char *const endings[] = {
    "\\fi", "\\end{comment}", "\\end{verbatim}", "\\end{lstlisting}",
    "\\end{minted}", "\\end{asy}", "\\end{asydef}", "\\end{pycode}",
    "\\end{luacode}", "\\end{luacode*}", "\\end{sagesilent}", "\\end{sageblock}"
};

static bool command_letter(int32_t c) {
    return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') ||
           c == '_' || c == ':' || c == '@';
}

void *tree_sitter_latex_external_scanner_create(void) { return NULL; }
void tree_sitter_latex_external_scanner_destroy(void *payload) { (void)payload; }
unsigned tree_sitter_latex_external_scanner_serialize(void *payload, char *buffer) {
    (void)payload; (void)buffer; return 0;
}
void tree_sitter_latex_external_scanner_deserialize(void *payload, const char *buffer, unsigned length) {
    (void)payload; (void)buffer; (void)length;
}

bool tree_sitter_latex_external_scanner_scan(void *payload, TSLexer *lexer, const bool *valid) {
    (void)payload;
    unsigned token = 0, count = 0;
    for (unsigned i = 0; i < sizeof(endings) / sizeof(endings[0]); ++i) {
        if (valid[i]) { token = i; ++count; }
    }
    // Multiple valid raw bodies mean error recovery. Do not swallow the document.
    if (count != 1) return false;
    const char *ending = endings[token];
    bool body = false;
    lexer->result_symbol = (uint16_t)token;
    while (!lexer->eof(lexer)) {
        // Keep the closing delimiter outside the raw token for the grammar.
        lexer->mark_end(lexer);
        size_t matched = 0;
        while (ending[matched] && lexer->lookahead == ending[matched] && !lexer->eof(lexer)) {
            lexer->advance(lexer, false);
            ++matched;
        }
        if (!ending[matched] && (token != 0 || !command_letter(lexer->lookahead))) {
            // This grammar requires a body token even for an empty environment.
            return true;
        }
        if (matched == 0) lexer->advance(lexer, false);
        body = true;
    }
    lexer->mark_end(lexer);
    return body;
}
