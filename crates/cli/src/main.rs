//! `eng` command-line entry point.

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--version") {
        println!("eng {}", env!("CARGO_PKG_VERSION"));
    }
}
