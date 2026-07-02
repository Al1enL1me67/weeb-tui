//! HTML parsing facade ported from manga-tui's `backend::html_parser`.
//!
//! `HtmlElement` signals "this is HTML", `HtmlParser` abstracts querying, and
//! `ParseHtml` constructs typed DTOs from HTML. Keeps scraping decoupled from
//! the `scraper` crate.
use std::error::Error;
use std::fmt;
use std::ops::Deref;

/// Wrapper type that represents HTML, not just an arbitrary `String`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlElement(String);

impl Deref for HtmlElement {
    type Target = String;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl HtmlElement {
    pub fn new<T: Into<String>>(raw: T) -> Self {
        Self(raw.into())
    }
}

/// Simple parse error wrapper implementing `Error`.
#[derive(Debug, Clone)]
pub struct ParseErr(pub String);

impl fmt::Display for ParseErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for ParseErr {}

impl From<String> for ParseErr {
    fn from(s: String) -> Self { ParseErr(s) }
}

impl From<&str> for ParseErr {
    fn from(s: &str) -> Self { ParseErr(s.to_string()) }
}

pub trait ParseHtml: Sized {
    type ParseError: Error;
    fn parse_html(html: HtmlElement) -> Result<Self, Self::ParseError>;
}

pub trait HtmlParser {
    fn get_element_children(&self, element: &HtmlElement) -> Vec<HtmlElement>;
    fn get_element(&self, class: &str) -> Option<HtmlElement>;
    fn get_element_from(&self, from: &HtmlElement, class: &str) -> Option<HtmlElement>;
    fn get_matching_elements_from(&self, from: &HtmlElement, class: &str) -> Vec<HtmlElement>;
    fn get_matching_elements(&self, selector: &str) -> Vec<HtmlElement>;
    fn get_inner_html(&self, document: &HtmlElement) -> String;
    fn get_inner_text(&self, document: &HtmlElement) -> String;
    fn get_element_attr(&self, element: &HtmlElement, attr_to_find: &str) -> Option<String>;
}
