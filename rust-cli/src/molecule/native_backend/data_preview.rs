//! Data preview: Stata-metadata-driven typing + temporary `export delimited`
//! CSV extraction.
//!
//! The previous pystata backend exported data through pandas, which preserved
//! Stata storage types. A CSV round-trip cannot infer types reliably (leading
//! zeros, value labels, all-missing columns, ...), so this module reads the
//! storage type from `describe` and converts CSV fields according to the
//! source type. `export delimited` is always run with `nolabel` so numeric
//! codes come back, matching the old `pdataframe_from_data(valuelabel=False)`.

use crate::atom::csv_table::{infer_dtype, parse_csv};
use crate::atom::stata_engine::{StataEngine, StataOutput};
use crate::atom::stata_syntax::stata_quote_path;
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub(crate) struct ColumnMeta {
    pub(crate) name: String,
    pub(crate) storage: String,
}

impl ColumnMeta {
    fn dtype(&self) -> &'static str {
        match self.storage.as_str() {
            "byte" | "int" | "long" => "int64",
            "float" | "double" => "float64",
            _ => "object",
        }
    }

    fn is_numeric(&self) -> bool {
        matches!(
            self.storage.as_str(),
            "byte" | "int" | "long" | "float" | "double"
        )
    }
}

pub(crate) struct DataTable {
    pub(crate) columns: Vec<String>,
    pub(crate) dtypes: serde_json::Map<String, Value>,
    pub(crate) rows: Vec<Vec<Value>>,
    pub(crate) index: Vec<Value>,
    pub(crate) total_rows: i64,
    pub(crate) displayed_rows: i64,
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn parse_obs_count(output: &StataOutput) -> Result<i64> {
    output
        .output
        .lines()
        .find_map(|line| line.trim().parse::<i64>().ok())
        .ok_or_else(|| anyhow::anyhow!("Failed to parse observation count from Stata output"))
}

/// Parse the variable table of `describe` output:
///
/// ```text
/// Variable      Storage   Display    Value
///     name         type    format    label      Variable label
/// ------------------------------------------------------------------
/// make            str18   %-18s                 Make and model
/// price           int     %8.0gc                Price
/// ```
fn parse_describe(output: &str) -> Vec<ColumnMeta> {
    let mut columns = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("name") && line.contains("Storage") {
            continue; // header line
        }
        let mut parts = trimmed.split_whitespace();
        let name = parts.next().unwrap_or_default();
        let storage = parts.next().unwrap_or_default();
        if !is_valid_variable_name(name) || !is_valid_storage(storage) {
            continue;
        }
        if trimmed.contains("complex") {
            columns.push(ColumnMeta {
                name: name.to_string(),
                storage: "complex".to_string(),
            });
            continue;
        }
        columns.push(ColumnMeta {
            name: name.to_string(),
            storage: storage.to_string(),
        });
    }
    columns
}

fn is_valid_variable_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c == '_' || c.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

fn is_valid_storage(storage: &str) -> bool {
    storage == "byte"
        || storage == "int"
        || storage == "long"
        || storage == "float"
        || storage == "double"
        || storage == "strL"
        || (storage.starts_with("str") && storage[3..].parse::<usize>().is_ok())
}

/// Non-destructive variable-name listing for REPL completion
/// (`describe, simple` prints one space-separated list of names).
pub(crate) fn simple_variable_names(engine: &StataEngine) -> Vec<String> {
    let result = engine.execute("quietly describe, simple");
    if result.rc != 0 {
        return Vec::new();
    }
    result
        .output
        .split_whitespace()
        .filter(|token| is_valid_variable_name(token))
        .map(ToOwned::to_owned)
        .collect()
}

fn describe_columns(engine: &StataEngine) -> Result<Vec<ColumnMeta>> {
    let result = engine.execute("describe");
    if result.rc != 0 {
        anyhow::bail!(
            "describe failed (rc={}): {}",
            result.rc,
            result.output.trim()
        );
    }
    Ok(parse_describe(&result.output))
}

fn export_csv(engine: &StataEngine, in_clause: Option<&str>) -> Result<PathBuf> {
    let csv_path = engine.temp_dir().join(format!(
        "stata_data_{}_{}.csv",
        std::process::id(),
        now_nanos()
    ));
    let command = match in_clause {
        Some(clause) => format!(
            "quietly export delimited {clause} using {}, replace nolabel",
            stata_quote_path(&csv_path.display().to_string())?
        ),
        None => format!(
            "quietly export delimited using {}, replace nolabel",
            stata_quote_path(&csv_path.display().to_string())?
        ),
    };
    let result = engine.execute(&command);
    if result.rc != 0 {
        anyhow::bail!(
            "export delimited failed (rc={}): {}",
            result.rc,
            result.output.trim()
        );
    }
    if !csv_path.is_file() {
        anyhow::bail!("export delimited did not create {}", csv_path.display());
    }
    Ok(csv_path)
}

fn is_missing_number(text: &str) -> bool {
    text == "."
        || (text.len() == 2
            && text.starts_with('.')
            && text.ends_with(|ch: char| ch.is_ascii_lowercase()))
}

fn to_json_value(meta: &ColumnMeta, text: Option<&str>) -> Value {
    let Some(text) = text else {
        return Value::Null;
    };
    let text = text.trim();
    if text.is_empty() {
        return Value::Null;
    }
    if meta.is_numeric() {
        if is_missing_number(text) {
            return Value::Null;
        }
        match meta.storage.as_str() {
            "byte" | "int" | "long" => text
                .parse::<i64>()
                .map(Value::from)
                .unwrap_or_else(|_| Value::String(text.to_string())),
            _ => text
                .parse::<f64>()
                .map(Value::from)
                .unwrap_or_else(|_| Value::String(text.to_string())),
        }
    } else {
        Value::String(text.to_string())
    }
}

fn read_table(
    csv_path: &PathBuf,
    meta: &[ColumnMeta],
    drop_column: Option<&str>,
) -> Result<DataTable> {
    let text = fs::read_to_string(csv_path)
        .with_context(|| format!("Failed to read {}", csv_path.display()))?;
    let rows = parse_csv(&text);
    if rows.is_empty() {
        return Ok(DataTable {
            columns: Vec::new(),
            dtypes: serde_json::Map::new(),
            rows: Vec::new(),
            index: Vec::new(),
            total_rows: 0,
            displayed_rows: 0,
        });
    }
    let mut columns: Vec<String> = rows[0]
        .iter()
        .map(|field| field.clone().unwrap_or_default())
        .collect();
    let drop_index = drop_column.and_then(|name| columns.iter().position(|c| c == name));

    let mut data_rows: Vec<Vec<Option<String>>> = Vec::new();
    let mut index_values: Vec<Value> = Vec::new();
    for raw_row in rows.iter().skip(1) {
        let mut row: Vec<Option<String>> = raw_row.clone();
        row.resize(columns.len(), None);
        if let Some(drop) = drop_index {
            let obs = row[drop]
                .as_ref()
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or_default();
            index_values.push(Value::from(obs));
        }
        data_rows.push(row);
    }

    let mut dtypes = serde_json::Map::new();
    let mut json_rows: Vec<Vec<Value>> = Vec::new();
    for (col_index, column) in columns.iter().enumerate() {
        if Some(col_index) == drop_index {
            continue;
        }
        let meta_for_column = meta.iter().find(|meta| meta.name == *column);
        let dtype = meta_for_column.map(ColumnMeta::dtype).unwrap_or_else(|| {
            let values: Vec<Option<String>> =
                data_rows.iter().map(|row| row[col_index].clone()).collect();
            infer_dtype(&values)
        });
        dtypes.insert(column.clone(), json!(dtype));
    }
    for row in &data_rows {
        let mut json_row = Vec::new();
        for (col_index, column) in columns.iter().enumerate() {
            if Some(col_index) == drop_index {
                continue;
            }
            let meta_for_column = meta.iter().find(|meta| meta.name == *column);
            let value = if let Some(meta) = meta_for_column {
                to_json_value(meta, row[col_index].as_deref())
            } else {
                let dtype = dtypes
                    .get(column)
                    .and_then(Value::as_str)
                    .unwrap_or("object");
                convert_inferred(dtype, row[col_index].as_deref())
            };
            json_row.push(value);
        }
        json_rows.push(json_row);
    }
    if drop_index.is_none() {
        index_values = (0..json_rows.len()).map(Value::from).collect();
    }
    if let Some(drop) = drop_index {
        columns.remove(drop);
    }
    Ok(DataTable {
        columns,
        dtypes,
        rows: json_rows,
        index: index_values,
        total_rows: 0,
        displayed_rows: 0,
    })
}

fn convert_inferred(dtype: &str, text: Option<&str>) -> Value {
    let Some(text) = text else {
        return Value::Null;
    };
    match dtype {
        "int64" => text.parse::<i64>().map(Value::from).unwrap_or(Value::Null),
        "float64" => text.parse::<f64>().map(Value::from).unwrap_or(Value::Null),
        _ => Value::String(text.to_string()),
    }
}

/// RAII guard that restores the dataset even when the filtered preview fails.
struct PreserveGuard<'a> {
    engine: &'a StataEngine,
    active: bool,
}

impl<'a> PreserveGuard<'a> {
    fn begin(engine: &'a StataEngine) -> Result<Self, String> {
        let result = engine.execute("preserve");
        if result.rc != 0 {
            return Err(format!(
                "preserve failed (rc={}): {}",
                result.rc,
                result.output.trim()
            ));
        }
        Ok(Self {
            engine,
            active: true,
        })
    }

    fn restore(&mut self) -> Result<(), String> {
        if !self.active {
            return Ok(());
        }
        let result = self.engine.execute("restore");
        self.active = false;
        if result.rc != 0 {
            return Err(format!(
                "restore failed (rc={}): {}",
                result.rc,
                result.output.trim()
            ));
        }
        Ok(())
    }
}

impl Drop for PreserveGuard<'_> {
    fn drop(&mut self) {
        if self.active {
            let _ = self.engine.execute("restore");
        }
    }
}

/// Read the current dataset as a preview table. Never leaves the dataset
/// modified: the unfiltered path uses `export delimited in 1/N` (no mutation),
/// and the filtered path runs inside a checked preserve/restore guard.
pub(crate) fn get_data(
    engine: &StataEngine,
    if_condition: Option<&str>,
    max_rows: u32,
) -> Result<DataTable, String> {
    let max_rows = i64::from(max_rows.max(1));
    let total_obs = parse_obs_count(&engine.execute("display _N")).map_err(|e| e.to_string())?;
    if total_obs == 0 {
        return Ok(DataTable {
            columns: Vec::new(),
            dtypes: serde_json::Map::new(),
            rows: Vec::new(),
            index: Vec::new(),
            total_rows: 0,
            displayed_rows: 0,
        });
    }
    let meta = describe_columns(engine).map_err(|e| format!("describe failed: {e}"))?;

    if let Some(condition) = if_condition {
        let mut guard = PreserveGuard::begin(engine)?;
        let result = (|| -> Result<(PathBuf, i64)> {
            let gen = engine.execute("quietly gen long _stata_cli_orig_obs = _n - 1");
            if gen.rc != 0 {
                anyhow::bail!(
                    "failed to generate observation index: {}",
                    gen.output.trim()
                );
            }
            let keep = engine.execute(&format!("quietly keep if {condition}"));
            if keep.rc != 0 {
                anyhow::bail!("invalid if-condition: {}", keep.output.trim());
            }
            let filtered_obs =
                parse_obs_count(&engine.execute("display _N")).context("count after keep")?;
            if filtered_obs > max_rows {
                let limit = engine.execute(&format!("quietly keep in 1/{max_rows}"));
                if limit.rc != 0 {
                    anyhow::bail!("row limit failed: {}", limit.output.trim());
                }
            }
            let csv = export_csv(engine, None)?;
            Ok((csv, filtered_obs))
        })();
        match result {
            Ok((csv, filtered_obs)) => {
                let mut table = read_table(&csv, &meta, Some("_stata_cli_orig_obs"))
                    .map_err(|e| format!("parse preview CSV: {e}"))?;
                let _ = fs::remove_file(&csv);
                guard.restore()?;
                table.total_rows = filtered_obs;
                table.displayed_rows = filtered_obs.min(max_rows);
                Ok(table)
            }
            Err(error) => Err(format!("Filter error: {error:#}")),
        }
    } else {
        let csv = if total_obs > max_rows {
            let in_clause = format!("in 1/{max_rows}");
            export_csv(engine, Some(&in_clause)).map_err(|e| e.to_string())?
        } else {
            export_csv(engine, None).map_err(|e| e.to_string())?
        };
        let mut table = read_table(&csv, &meta, None).map_err(|e| e.to_string())?;
        let _ = fs::remove_file(&csv);
        table.total_rows = total_obs;
        table.displayed_rows = total_obs.min(max_rows);
        Ok(table)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_describe_variable_table() {
        let output = "\
Contains data from /x.dta
 Observations:            3
    Variables:            4
------------------------------------------------------------------------------
Variable      Storage   Display    Value
    name         type    format    label      Variable label
------------------------------------------------------------------------------
make            str18   %-18s                 Make
code            byte    %8.0g      origin     Origin code
score           double  %10.0g                Score
note            strL    %9s                   Note
";
        let columns = parse_describe(output);
        let names: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["make", "code", "score", "note"]);
        assert_eq!(columns[0].dtype(), "object");
        assert_eq!(columns[1].dtype(), "int64");
        assert_eq!(columns[2].dtype(), "float64");
        assert_eq!(columns[3].dtype(), "object");
    }

    #[test]
    fn parses_describe_skips_header_and_complex_types() {
        let output = "\
name            type    format    label
z               float complex  %9s
plain           int     %8.0g
";
        let columns = parse_describe(output);
        assert_eq!(columns.len(), 2);
        assert_eq!(columns[0].storage, "complex");
        assert_eq!(columns[0].dtype(), "object");
    }

    #[test]
    fn converts_values_by_source_storage_type() {
        let str_meta = ColumnMeta {
            name: "s".into(),
            storage: "str6".into(),
        };
        let int_meta = ColumnMeta {
            name: "i".into(),
            storage: "int".into(),
        };
        let double_meta = ColumnMeta {
            name: "d".into(),
            storage: "double".into(),
        };

        // Leading zeros stay strings for str columns.
        assert_eq!(to_json_value(&str_meta, Some("00123")), json!("00123"));
        // Numeric columns treat missing/empty as null.
        assert_eq!(to_json_value(&int_meta, None), json!(null));
        assert_eq!(to_json_value(&int_meta, Some(".")), json!(null));
        assert_eq!(to_json_value(&double_meta, Some(".a")), json!(null));
        assert_eq!(to_json_value(&int_meta, Some("42")), json!(42));
        assert_eq!(to_json_value(&double_meta, Some("1.5")), json!(1.5));
        // A non-numeric value in a numeric column degrades to a string rather
        // than silently losing data.
        assert_eq!(to_json_value(&int_meta, Some("n/a")), json!("n/a"));
    }

    #[test]
    fn names_from_describe_simple_are_whitespace_tokens() {
        let output = "make price mpg\nweight length";
        // simple_variable_names filters through StataOutput; here we only
        // verify the token-level filtering used for names.
        let names: Vec<&str> = output
            .split_whitespace()
            .filter(|token| is_valid_variable_name(token))
            .collect();
        assert_eq!(names, vec!["make", "price", "mpg", "weight", "length"]);
    }
}
