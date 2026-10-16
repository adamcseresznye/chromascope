//! Text cells for the CSV files Chromascope writes.
//!
//! Spreadsheet programs run a cell that begins with `=`, `+`, `-` or `@` (or a tab or carriage
//! return) as a formula, and sample, source and analyte names come from files and from AI
//! clients. [`cell`] quotes every text cell and neutralises such a cell by prefixing an
//! apostrophe, the remedy OWASP recommends. A cell that is just a number, for example the
//! negative area `-5.2`, is left alone: it is data, not a formula.

/// Quotes `text` as a CSV cell and neutralises a leading formula character.
pub fn cell(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 3);
    out.push('"');
    if starts_like_a_formula(text) {
        out.push('\'');
    }
    out.push_str(&text.replace('"', "\"\""));
    out.push('"');
    out
}

fn starts_like_a_formula(text: &str) -> bool {
    text.starts_with(['=', '+', '-', '@', '\t', '\r']) && text.parse::<f64>().is_err()
}

#[cfg(test)]
mod tests {
    use super::cell;

    #[test]
    fn plain_text_is_only_quoted() {
        assert_eq!(cell("Sample 1"), "\"Sample 1\"");
        assert_eq!(cell(""), "\"\"");
        assert_eq!(cell("a, \"b\""), "\"a, \"\"b\"\"\"");
        assert_eq!(
            cell("ng=ml"),
            "\"ng=ml\"",
            "only a leading character matters"
        );
    }

    #[test]
    fn a_leading_formula_character_is_neutralised() {
        for hostile in [
            "=1+1",
            "=HYPERLINK(\"http://example.invalid\",\"x\")",
            "+cmd|' /C calc'!A0",
            "-2+3",
            "@SUM(A1:A9)",
            "\t=1+1",
            "\r=1+1",
            "-",
            "+",
            "=",
        ] {
            let quoted = cell(hostile);
            assert!(
                quoted.starts_with("\"'"),
                "{hostile:?} must get an apostrophe: {quoted}"
            );
            assert!(quoted.contains(&hostile.replace('"', "\"\"")));
        }
    }

    #[test]
    fn numbers_stay_numbers() {
        for number in ["-5.2", "+3", "-0.000125", "1e-9", "-1e9", "0", "12.5"] {
            assert_eq!(cell(number), format!("\"{number}\""), "{number}");
        }
    }
}
