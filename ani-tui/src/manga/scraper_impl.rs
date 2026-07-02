//! `HtmlParser` implementation over the `scraper` crate, ported verbatim from
//! manga-tui's `backend::html_parser::scraper`. Used by weebcentral + mangapill.
use scraper::{node, html, Selector};

use super::html_parser::{HtmlElement, HtmlParser};

#[derive(Debug)]
pub struct Scraper {
    document: html::Html,
}

impl Scraper {
    pub fn new(document: HtmlElement) -> Self {
        Self { document: html::Html::parse_document(&document) }
    }
}

pub trait AsSelector {
    fn as_selector(self) -> Selector;
}

impl AsSelector for &str {
    fn as_selector(self) -> Selector {
        Selector::parse(self).unwrap()
    }
}

impl HtmlParser for Scraper {
    fn get_inner_text(&self, document: &HtmlElement) -> String {
        let el = html::Html::parse_fragment(document);
        for e in el.tree {
            if let node::Node::Text(t) = e {
                return t.trim().to_string();
            }
        }
        String::new()
    }

    fn get_element_from(&self, from: &HtmlElement, class: &str) -> Option<HtmlElement> {
        html::Html::parse_fragment(from)
            .select(&class.as_selector())
            .next()
            .map(|el| HtmlElement::new(el.html()))
    }

    fn get_inner_html(&self, document: &HtmlElement) -> String {
        let el = html::Html::parse_fragment(document);
        el.select(&"*".as_selector()).nth(1).map(|el| el.inner_html()).unwrap_or_default()
    }

    fn get_element(&self, class: &str) -> Option<HtmlElement> {
        self.document.select(&class.as_selector()).next().map(|el| HtmlElement::new(el.html()))
    }

    fn get_element_attr(&self, element: &HtmlElement, attr_to_find: &str) -> Option<String> {
        let el = html::Html::parse_fragment(element);
        el.select(&"*".as_selector()).nth(1).and_then(|e| e.attr(attr_to_find).map(|a| a.to_string()))
    }

    fn get_element_children(&self, element: &HtmlElement) -> Vec<HtmlElement> {
        let doc = html::Html::parse_fragment(element);
        doc.select(&"*".as_selector()).skip(2).map(|el| HtmlElement::new(el.html())).collect()
    }

    fn get_matching_elements(&self, selector: &str) -> Vec<HtmlElement> {
        self.document.select(&selector.as_selector()).map(|el| HtmlElement::new(el.html())).collect()
    }

    fn get_matching_elements_from(&self, from: &HtmlElement, class: &str) -> Vec<HtmlElement> {
        html::Html::parse_fragment(from)
            .select(&class.as_selector())
            .map(|el| HtmlElement::new(el.html()))
            .collect()
    }
}
