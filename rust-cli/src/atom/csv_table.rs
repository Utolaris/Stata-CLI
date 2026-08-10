//! Minimal CSV reader used for `data view` previews.
//!
//! Stata's `export delimited` output is expected: UTF-8 text, comma
//! separated, optional quoted fields, doubled quotes (`""`) inside quoted
//! fields, CRLF or LF line endings. Rows may be ragged; short rows are padded
//! with `None`.

/// Parse CSV text into rows of optional string fields.
pub(crate) fn parse_csv(text: &str) -> Vec<Vec<Option<String>>> {
    let mut rows: Vec<Vec<Option<String>>> = Vec::new();
    let mut row: Vec<Option<String>> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(ch);
            }
            continue;
        }
        match ch {
            '"' => in_quotes = true,
            ',' => {
                row.push(optional_field(&field));
                field.clear();
            }
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                push_row(&mut rows, &mut row, &mut field);
            }
            '\n' => push_row(&mut rows, &mut row, &mut field),
            _ => field.push(ch),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(optional_field(&field));
        rows.push(row);
    }
    rows
}

fn optional_field(field: &str) -> Option<String> {
    if field.is_empty() {
        None
    } else {
        Some(field.to_string())
    }
}

fn push_row(
    rows: &mut Vec<Vec<Option<String>>>,
    row: &mut Vec<Option<String>>,
    field: &mut String,
) {
    row.push(optional_field(field));
    rows.push(std::mem::take(row));
    field.clear();
}

/// Column dtype inference matching pandas conventions used by the previous
/// backend: `int64`, `float64`, or `object`.
pub(crate) fn infer_dtype(values: &[Option<String>]) -> &'static str {
    if values.iter().any(Option::is_some)
        && values.iter().all(|value| {
            value
                .as_deref()
                .map(|s| s.parse::<i64>().is_ok())
                .unwrap_or(true)
        })
    {
        "int64"
    } else if values.iter().any(Option::is_some)
        && values.iter().all(|value| {
            value
                .as_deref()
                .map(|s| s.parse::<f64>().is_ok())
                .unwrap_or(true)
        })
    {
        "float64"
    } else {
        "object"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_rows() {
        let rows = parse_csv("a,b,c\n1,2,3\n");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1][0].as_deref(), Some("1"));
        assert_eq!(rows[1][2].as_deref(), Some("3"));
    }

    #[test]
    fn parses_quoted_fields_and_escaped_quotes() {
        let rows = parse_csv("name,note\n\"Smith, John\",\"said \"\"hi\"\"\"\n");
        assert_eq!(rows[1][0].as_deref(), Some("Smith, John"));
        assert_eq!(rows[1][1].as_deref(), Some("said \"hi\""));
    }

    #[test]
    fn treats_empty_fields_as_missing() {
        let rows = parse_csv("a,b,c\n1,,3\n");
        assert_eq!(rows[1][0].as_deref(), Some("1"));
        assert_eq!(rows[1][1], None);
        assert_eq!(rows[1][2].as_deref(), Some("3"));
    }

    #[test]
    fn infers_dtypes() {
        assert_eq!(
            infer_dtype(&[Some("1".into()), Some("2".into()), None]),
            "int64"
        );
        assert_eq!(
            infer_dtype(&[Some("1.5".into()), Some("2".into()), None]),
            "float64"
        );
        assert_eq!(infer_dtype(&[Some("x".into()), Some("2".into())]), "object");
        assert_eq!(infer_dtype(&[None, None]), "object");
    }
}
