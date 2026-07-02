//! Manga backend: domain models, provider traits, cache, HTML parsing, and
//! concrete providers (mangadex / weebcentral / mangapill). Ported from
//! manga-tui's `backend` module, pruned to read-only (no downloads, no feed).
//!
//! Public entrypoint: `providers::MangaProviderInstance` — a concrete enum the
//! app holds instead of a trait object (the provider traits use `impl Future`
//! returns and are not dyn-safe).
pub mod cache;
pub mod html_parser;
pub mod models;
pub mod providers;
pub mod scraper_impl;

pub use cache::{CacheDuration, Cacher, EmptyCache, InMemoryCache, InsertEntry};
pub use html_parser::{HtmlElement, HtmlParser, ParseHtml};
pub use models::*;
pub use scraper_impl::{AsSelector, Scraper};
