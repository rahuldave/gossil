//! Development conformance tool, separate from the Gossil engine.

/// Execute one explicit inherited test configuration.
fn main() {
    std::process::exit(gossil::cli(&std::env::args().skip(1).collect::<Vec<_>>()));
}
