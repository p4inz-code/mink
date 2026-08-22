//! Option<T> — the standard MINK type for nullable values.
//!
//! `Option<T>` represents a value that may or may not be present.
//! It is the V1 foundation for null safety.
//!
//! # Variants
//! - `Some(T)` — a value of type T is present.
//! - `None` — no value is present.
//!
//! # Usage
//! ```
//! enum Option<T> { Some(T), None }
//!
//! fn find_value(x: Int) -> Option<Int> {
//!     if x == 0 { return Option::None; }
//!     return Option::Some(x);
//! }
//! ```
//!
//! # V1 Limitations
//! Pattern matching on generic enum variants is not yet supported.
//! Use construction and return types. Full match support is a V2 feature.

// The actual type definition is a MINK source file that users can import.
// The MINK source definition is:
//
// enum Option<T> {
//     Some(T),
//     None
// }
