//! Page standard compliance validator.
//!
//! Walks the live DOM tree looking for `app-tabs__item` wrappers and validates
//! that each one contains a page root element that meets the standard:
//!   1. Has an HTML `id` in format `{entity}--{category}`.
//!   2. Has a `data-page-category` attribute with a known category.
//!   3. For standard categories, contains `page__header` and `page__content` children.

use crate::shared::page_standard::{
    is_known_category, is_valid_page_id, PAGE_CAT_CUSTOM, PAGE_CAT_LEGACY, STANDARD_CATEGORIES,
};
use wasm_bindgen::JsCast;
use web_sys::Element;

// ── Result types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone)]
pub struct ValidationIssue {
    pub severity: Severity,
    pub tab_key: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct ValidationReport {
    pub issues: Vec<ValidationIssue>,
    pub total_tabs: usize,
    pub ok_count: usize,
    pub legacy_count: usize,
}

impl ValidationReport {
    pub fn error_count(&self) -> usize {
        self.issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .count()
    }

    pub fn warning_count(&self) -> usize {
        self.issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
            .count()
    }
}

// ── Validator ─────────────────────────────────────────────────────────────────

/// Run validation against the live DOM and return a report.
pub fn validate_pages() -> ValidationReport {
    let mut issues = Vec::new();
    let mut total_tabs = 0;
    let mut ok_count = 0;
    let mut legacy_count = 0;

    let Some(window) = web_sys::window() else {
        return ValidationReport {
            issues,
            total_tabs,
            ok_count,
            legacy_count,
        };
    };
    let Some(document) = window.document() else {
        return ValidationReport {
            issues,
            total_tabs,
            ok_count,
            legacy_count,
        };
    };

    // All app-tabs__item wrappers (one per open tab).
    // A failed selector means no tabs are mounted — return an empty report rather
    // than falling back to "*", which would validate every element on the page.
    let Ok(tab_items) = document.query_selector_all(".app-tabs__item") else {
        return ValidationReport {
            issues,
            total_tabs,
            ok_count,
            legacy_count,
        };
    };

    for i in 0..tab_items.length() {
        let Some(node) = tab_items.get(i) else {
            continue;
        };
        let Some(tab_el) = node.dyn_ref::<Element>() else {
            continue;
        };

        let tab_key = tab_el
            .get_attribute("data-tab-key")
            .unwrap_or_else(|| format!("tab[{}]", i));

        total_tabs += 1;

        // Find the first child element — the page root
        let page_root = tab_el.first_element_child();

        match page_root {
            None => {
                issues.push(ValidationIssue {
                    severity: Severity::Error,
                    tab_key: tab_key.clone(),
                    message: "Tab has no child element (page root missing)".to_string(),
                });
            }
            Some(root) => {
                let page_id = root.get_attribute("id");
                let category = root.get_attribute("data-page-category");

                let mut tab_ok = true;

                // 1. Check id presence and format
                match &page_id {
                    None => {
                        issues.push(ValidationIssue {
                            severity: Severity::Error,
                            tab_key: tab_key.clone(),
                            message:
                                "Missing HTML id attribute (expected format: entity--category)"
                                    .to_string(),
                        });
                        tab_ok = false;
                    }
                    Some(id) if !is_valid_page_id(id) => {
                        issues.push(ValidationIssue {
                            severity: Severity::Error,
                            tab_key: tab_key.clone(),
                            message: format!(
                                "HTML id `{id}` does not match `{{entity}}--{{category}}` format"
                            ),
                        });
                        tab_ok = false;
                    }
                    _ => {}
                }

                // 2. Check data-page-category
                match &category {
                    None => {
                        issues.push(ValidationIssue {
                            severity: Severity::Error,
                            tab_key: tab_key.clone(),
                            message: "Missing data-page-category attribute".to_string(),
                        });
                        tab_ok = false;
                    }
                    Some(cat) if !is_known_category(cat) => {
                        issues.push(ValidationIssue {
                            severity: Severity::Error,
                            tab_key: tab_key.clone(),
                            message: format!("Unknown category `{cat}`"),
                        });
                        tab_ok = false;
                    }
                    Some(cat) if cat == PAGE_CAT_LEGACY => {
                        legacy_count += 1;
                        issues.push(ValidationIssue {
                            severity: Severity::Info,
                            tab_key: tab_key.clone(),
                            message: format!(
                                "Legacy page `{}` — queued for refactoring",
                                page_id.as_deref().unwrap_or("?")
                            ),
                        });
                    }
                    _ => {}
                }

                // 3. For standard categories check structure
                if let Some(cat) = &category {
                    if STANDARD_CATEGORIES.contains(&cat.as_str())
                        && cat != PAGE_CAT_LEGACY
                        && cat != PAGE_CAT_CUSTOM
                    {
                        if !has_class_child(&root, "page__header") {
                            issues.push(ValidationIssue {
                                severity: Severity::Warning,
                                tab_key: tab_key.clone(),
                                message: "Missing .page__header child element".to_string(),
                            });
                            tab_ok = false;
                        }
                        if !has_class_child(&root, "page__content") {
                            issues.push(ValidationIssue {
                                severity: Severity::Warning,
                                tab_key: tab_key.clone(),
                                message: "Missing .page__content child element".to_string(),
                            });
                            tab_ok = false;
                        }
                    }
                }

                if tab_ok {
                    ok_count += 1;
                }
            }
        }
    }

    ValidationReport {
        issues,
        total_tabs,
        ok_count,
        legacy_count,
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn has_class_child(el: &Element, class: &str) -> bool {
    let children = el.children();
    for i in 0..children.length() {
        if let Some(child) = children.get_with_index(i) {
            if child.class_list().contains(class) {
                return true;
            }
        }
    }
    false
}
