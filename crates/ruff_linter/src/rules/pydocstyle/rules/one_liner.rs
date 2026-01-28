use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_semantic::Definition;
use ruff_source_file::{LineRanges, NewlineWithTrailingNewline};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;
use crate::rules::pydocstyle::settings::OneLineDocstringStyle;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for docstrings that don't match the configured one-line docstring
/// style.
///
/// ## Why is this bad?
/// [PEP 257] recommends that docstrings that _can_ fit on one line should be
/// formatted on a single line, for consistency and readability. Some projects
/// prefer to use multi-line docstrings even when a summary would fit on one
/// line, to reduce churn as docstrings evolve.
///
/// ## Example
/// ```python
/// def average(values: list[float]) -> float:
///     """
///     Return the mean of the given values.
///     """
/// ```
///
/// Use instead (with `one-line-docstring-style = "single"`):
/// ```python
/// def average(values: list[float]) -> float:
///     """Return the mean of the given values."""
/// ```
///
/// Use instead (with `one-line-docstring-style = "multi"`):
/// ```python
/// def average(values: list[float]) -> float:
///     """
///     Return the mean of the given values.
///     """
/// ```
///
/// ## Fix safety
/// The fix is marked as unsafe because it could affect tools that parse docstrings,
/// documentation generators, or custom introspection utilities that rely on
/// specific docstring formatting.
///
/// ## Options
///
/// - `lint.pydocstyle.ignore-decorators`
/// - `lint.pydocstyle.one-line-docstring-style`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
///
/// [PEP 257]: https://peps.python.org/pep-0257/
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.68")]
pub(crate) struct UnnecessaryMultilineDocstring {
    style: OneLineDocstringStyle,
}

impl Violation for UnnecessaryMultilineDocstring {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        match self.style {
            OneLineDocstringStyle::Single => {
                "One-line docstring should fit on one line".to_string()
            }
            OneLineDocstringStyle::Multi => {
                "One-line docstring should use multi-line quotes".to_string()
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        match self.style {
            OneLineDocstringStyle::Single => Some("Reformat to one line".to_string()),
            OneLineDocstringStyle::Multi => Some("Reformat to multi-line".to_string()),
        }
    }
}

/// D200
pub(crate) fn one_liner(checker: &Checker, docstring: &Docstring) {
    let style = checker.settings().pydocstyle.one_line_docstring_style();
    match style {
        OneLineDocstringStyle::Single => {
            let mut line_count = 0;
            let mut non_empty_line_count = 0;
            for line in NewlineWithTrailingNewline::from(docstring.body().as_str()) {
                line_count += 1;
                if !line.trim().is_empty() {
                    non_empty_line_count += 1;
                }
                if non_empty_line_count > 1 {
                    return;
                }
            }

            if non_empty_line_count == 1 && line_count > 1 {
                let mut diagnostic = checker.report_diagnostic(
                    UnnecessaryMultilineDocstring { style },
                    docstring.range(),
                );

                // If removing whitespace would lead to an invalid string of quote
                // characters, avoid applying the fix.
                let body = docstring.body();
                let trimmed = body.trim();
                let quote_char = docstring.quote_style().as_char();
                if trimmed.chars().rev().take_while(|c| *c == '\\').count() % 2 == 0
                    && !trimmed.ends_with(quote_char)
                    && !trimmed.starts_with(quote_char)
                {
                    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                        format!(
                            "{leading}{trimmed}{trailing}",
                            leading = docstring.opener(),
                            trailing = docstring.closer()
                        ),
                        docstring.range(),
                    )));
                }
            }
        }
        OneLineDocstringStyle::Multi => {
            if !docstring.is_triple_quoted() {
                return;
            }

            if NewlineWithTrailingNewline::from(docstring.contents())
                .nth(1)
                .is_some()
            {
                return;
            }

            let body = docstring.body();
            if body.is_empty() {
                return;
            }

            let mut diagnostic = checker.report_diagnostic(
                UnnecessaryMultilineDocstring { style },
                docstring.range(),
            );

            let mut indentation = String::from(docstring.compute_indentation());
            let mut fixable = true;
            if !indentation.chars().all(char::is_whitespace) {
                fixable = false;

                // If the docstring isn't on its own line, look at the statement indentation,
                // and add the default indentation to get the "right" level.
                if let Definition::Member(member) = &docstring.definition {
                    let stmt_line_start = checker.locator().line_start(member.start());
                    let stmt_indentation = checker
                        .locator()
                        .slice(TextRange::new(stmt_line_start, member.start()));

                    if stmt_indentation.chars().all(char::is_whitespace) {
                        indentation.clear();
                        indentation.push_str(stmt_indentation);
                        indentation.push_str(checker.stylist().indentation());
                        fixable = true;
                    }
                }
            }

            if fixable {
                let line_ending = checker.stylist().line_ending().as_str();
                // Prefer the D213 layout when enabled; otherwise default to the
                // first-line summary style (D212-compatible) even if D212 is not
                // explicitly enabled.
                let use_second_line_summary = checker
                    .is_rule_enabled(crate::registry::Rule::MultiLineSummarySecondLine);
                let replacement = if use_second_line_summary {
                    format!(
                        "{}{}{}{}{}{}",
                        docstring.opener(),
                        line_ending,
                        indentation,
                        body.as_str(),
                        line_ending,
                        format!("{}{}", indentation, docstring.closer())
                    )
                } else {
                    format!(
                        "{}{}{}{}{}",
                        docstring.opener(),
                        body.as_str(),
                        line_ending,
                        indentation,
                        docstring.closer()
                    )
                };

                diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                    replacement,
                    docstring.range(),
                )));
            }
        }
    }
}
