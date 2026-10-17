//! Gate-test host.
//!
//! This package carries no library code. It exists so that `tests/gates/` is a
//! directory in the repository rather than a directory inside some subsystem
//! crate, because `tests/` is a protected path owned by the test-author role.
//!
//! Nothing here is implemented yet. The gate tests arrive from the test-author
//! role after the specs, and they are expected to **fail** until the
//! implementers land the code (guide: Fail before, pass after).
