fn main() {
    // tree-sitter-latex 0.1.0 publishes parser.c but omits its external scanner.
    // Keep the compatibility scanner in our source tree, never in Cargo's cache.
    println!("cargo:rerun-if-changed=native/latex_scanner.c");
    cc::Build::new()
        .file("native/latex_scanner.c")
        .std("c11")
        .warnings(true)
        // tree-sitter-latex's parser references the scanner but is linked after
        // it; GNU ld (Linux arm64) drops archive members nothing has asked for
        // yet. Keep the whole archive so link order does not matter.
        .link_lib_modifier("+whole-archive")
        .compile("folio_latex_scanner");
}
