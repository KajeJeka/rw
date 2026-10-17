//! The `FO1_DATA` gate harness.
//!
//! A dev-dependency only. No runtime crate depends on it. It resolves the
//! player's install from the `FO1_DATA` environment variable, fingerprints it,
//! and hands gate tests byte slices. It never executes the game, a patcher or
//! an installer.
//!
//! Every function here is **fallible**. A gate test that needs the install
//! unwraps the result inside the test, where `clippy.toml` permits it. That
//! way this crate keeps the full deny list and a missing install still fails
//! loudly with a clear message instead of silently skipping.
//!
//! This path is PROTECTED (guide: Verification and anti faking).

/// Nothing is implemented yet. The harness and the independent reference
/// decoders come from the test-author role.
pub const ENV_INSTALL_ROOT: &str = "FO1_DATA";
