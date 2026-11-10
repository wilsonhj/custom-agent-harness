//! The `eng-shim` gate, built inside this crate (only with the `failpoints` feature) so the
//! engine's integration tests can locate it through `CARGO_BIN_EXE_eng-test-shim`.

fn main() {
    harness_exec::shim::run()
}
