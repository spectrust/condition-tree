// ::START CLIPPY LINTS::
// This block is auto-generated. Please do not edit it directly! If you would
// like to make changes, make them in `scripts/insert-clippy-lints.sh`.
// -------------------------------------------------------------------
// Non-default Lints
// -------------------------------------------------------------------
// Warn for any non-Send futures. These are fine for single-threaded
// executors, but we use a multi-threaded one usually, and having
// non-Send futures can lead to really obscure type errors
#![deny(clippy::future_not_send)]
// Deny direct comparison of floating point numbers
#![deny(clippy::float_cmp)]
// Deny direct comparison of const floating point numbers
#![deny(clippy::float_cmp_const)]
// Deny comparing floats for equality without using epsilon comparison
#![deny(clippy::float_equality_without_abs)]
// Deny defining float literals that are beyond the type's precision
#![deny(clippy::lossy_float_literal)]
// We need to be careful about printing anything to stdout/err in prod,
// because it can be quite expensive in DD. All production output should
// go through a logging macro, except potentially in cases where we
// know it's not in a loop, e.g. service startup
// Allow in debug builds, with a warning, but not in release builds
#![cfg_attr(not(debug_assertions), deny(clippy::dbg_macro))]
#![cfg_attr(not(debug_assertions), deny(clippy::print_stdout))]
#![cfg_attr(not(debug_assertions), deny(clippy::print_stderr))]
#![cfg_attr(debug_assertions, warn(clippy::dbg_macro))]
#![cfg_attr(debug_assertions, warn(clippy::print_stdout))]
#![cfg_attr(debug_assertions, warn(clippy::print_stderr))]
// Deny todo!() in release builds
#![cfg_attr(not(debug_assertions), deny(clippy::todo))]
// But allow (with a warning so you remember it's there) in debug builds
#![cfg_attr(debug_assertions, warn(clippy::todo))]
// Warn on uninlined format args
#![warn(clippy::uninlined_format_args)]
// ::END CLIPPY LINTS::
pub mod conditions;

pub use conditions::*;
