#![doc = include_str!("../README.md")]
#![deny(unsafe_code, missing_docs, rustdoc::broken_intra_doc_links)]

// `doc`, `edit`, `layout` and `style` are public for the fuzz tests in
// `tests/`; hosts use `widget` (and `parse`, `fonts`).
#[doc(hidden)]
pub mod doc;
#[doc(hidden)]
pub mod edit;
pub mod fonts;
#[doc(hidden)]
pub mod layout;
pub mod parse;
#[doc(hidden)]
pub mod style;
pub mod widget;
