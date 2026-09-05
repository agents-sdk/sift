//! CSV/TSV/Markdown 表格压缩器。
//!
//! 将表格严格解析成对象数组，再复用 SmartCrusher 的结构分析与代表行选择。
//! 列数不齐、重复/空表头、未闭合引号都会回退，避免把值映射到错误字段。

use std::collections::BTreeSet;

use serde_json::{Map, Value};

use crate::content::{detect_tabular_format, ContentType, TabularFormat};
use crate::stash;
use crate::transforms::smart_crusher::{SmartCrusher, SmartCrusherConfig};
use crate::transforms::{CompressionContext, OffloadOutput, OffloadTransform, TransformError};

#[derive(Debug, Clone)]
pub struct TabularCompressorConfig {
    pub min_savings_bytes: usize,
}

impl Default for TabularCompressorConfig {
    fn default() -> Self {
        Self {
            min_savings_bytes: 1,
        }
    }
}

pub struct TabularCompressor {
    config: TabularCompressorConfig,
}

impl TabularCompressor {
    pub fn new(config: TabularCompressorConfig) -> Self {
        Self { config }
    }

    fn compress(
        &self,
        input: &str,
        ctx: &CompressionContext,
    ) -> Result<OffloadOutput, TransformError> {
        let format = detect_tabular_format(input).ok_or(TransformError::InvalidInput)?;
        let (headers, rows) = match format {
            TabularFormat::Delimited(delimiter) => parse_delimited(input, delimiter)?,
            TabularFormat::Markdown => parse_markdown(input)?,
        };
        validate_table(&headers, &rows)?;

        let records = rows
            .into_iter()
            .map(|row| {
                Value::Object(
                    headers
                        .iter()
                        .cloned()
                        .zip(row)
                        .map(|(header, cell)| (header, Value::String(cell)))
                        .collect::<Map<_, _>>(),
                )
            })
            .collect::<Vec<_>>();
        let json = serde_json::to_string(&records)
            .map_err(|error| TransformError::Internal(error.to_string()))?;
        let crusher = SmartCrusher::new(SmartCrusherConfig::default());
        let (compressed, was_modified) = crusher.crush(&json, ctx.query.as_deref())?;
        if !was_modified
            || input.len().saturating_sub(compressed.len()) < self.config.min_savings_bytes
        {
            return Err(TransformError::Skipped);
        }
        Ok(OffloadOutput::new(compressed, input.to_string()))
    }
}

impl OffloadTransform for TabularCompressor {
    fn name(&self) -> &'static str {
        "tabular_compressor"
    }

    fn applies_to(&self) -> ContentType {
        ContentType::Tabular
    }

    fn estimate_bloat(&self, input: &str) -> f64 {
        let rows = input.lines().filter(|line| !line.trim().is_empty()).count();
        rows.saturating_sub(1) as f64 / 15.0
    }

    fn cache_key(&self, input: &str) -> String {
        stash::compute_key(input)
    }

    fn apply(
        &self,
        input: &str,
        ctx: &CompressionContext,
    ) -> Result<OffloadOutput, TransformError> {
        self.compress(input, ctx)
    }
}

fn parse_delimited(
    input: &str,
    delimiter: char,
) -> Result<(Vec<String>, Vec<Vec<String>>), TransformError> {
    let mut parsed = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if quoted {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                }
            } else {
                field.push(ch);
            }
            continue;
        }
        match ch {
            '"' if field.trim().is_empty() => {
                field.clear();
                quoted = true;
            }
            current if current == delimiter => {
                row.push(field.trim().to_string());
                field.clear();
            }
            '\n' => {
                row.push(field.trim().to_string());
                field.clear();
                if row.iter().any(|cell| !cell.is_empty()) {
                    parsed.push(std::mem::take(&mut row));
                } else {
                    row.clear();
                }
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            _ => field.push(ch),
        }
    }
    if quoted {
        return Err(TransformError::InvalidInput);
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field.trim().to_string());
        if row.iter().any(|cell| !cell.is_empty()) {
            parsed.push(row);
        }
    }
    let mut rows = parsed.into_iter();
    let headers = rows.next().ok_or(TransformError::InvalidInput)?;
    Ok((headers, rows.collect()))
}

fn parse_markdown(input: &str) -> Result<(Vec<String>, Vec<Vec<String>>), TransformError> {
    let lines: Vec<&str> = input
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let separator = lines
        .windows(2)
        .position(|pair| pair[0].contains('|') && is_separator_row(pair[1]))
        .ok_or(TransformError::InvalidInput)?;
    if lines[separator..].iter().any(|line| line.contains("\\|")) {
        return Err(TransformError::InvalidInput);
    }
    let headers = split_markdown_row(lines[separator]);
    let rows = lines[separator + 2..]
        .iter()
        .map(|line| split_markdown_row(line))
        .collect();
    Ok((headers, rows))
}

fn split_markdown_row(line: &str) -> Vec<String> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(|cell| cell.trim().to_string())
        .collect()
}

fn is_separator_row(line: &str) -> bool {
    let cells = split_markdown_row(line);
    cells.len() >= 2
        && cells.iter().all(|cell| {
            let dashes = cell.trim_matches(':');
            dashes.len() >= 2 && dashes.chars().all(|ch| ch == '-')
        })
}

fn validate_table(headers: &[String], rows: &[Vec<String>]) -> Result<(), TransformError> {
    if headers.len() < 2 || rows.is_empty() || headers.iter().any(|header| header.is_empty()) {
        return Err(TransformError::InvalidInput);
    }
    let unique: BTreeSet<&str> = headers.iter().map(String::as_str).collect();
    if unique.len() != headers.len() || rows.iter().any(|row| row.len() != headers.len()) {
        return Err(TransformError::InvalidInput);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quoted_csv_cells_without_shifting_columns() {
        let input = include_str!("../../tests/fixtures/services.csv");
        let (headers, rows) = parse_delimited(input, ',').unwrap();
        assert_eq!(headers.len(), 6);
        assert_eq!(rows.len(), 30);
        assert_eq!(rows[29][1], "service, edge");
        assert_eq!(rows[29][2], "eu-west-1");
    }

    #[test]
    fn rejects_ragged_and_duplicate_header_tables() {
        let ragged = parse_delimited("id,name\n1,api\n2\n", ',').unwrap();
        assert_eq!(
            validate_table(&ragged.0, &ragged.1),
            Err(TransformError::InvalidInput)
        );
        let duplicate = parse_delimited("id,id\n1,api\n2,worker\n", ',').unwrap();
        assert_eq!(
            validate_table(&duplicate.0, &duplicate.1),
            Err(TransformError::InvalidInput)
        );
    }

    #[test]
    fn markdown_table_routes_through_smart_crusher() {
        let mut input = String::from("| id | service | status |\n| --- | --- | --- |\n");
        for index in 0..30 {
            input.push_str(&format!("| {index} | service-{index} | healthy |\n"));
        }
        let result = TabularCompressor::new(TabularCompressorConfig::default())
            .compress(&input, &CompressionContext::default())
            .unwrap();
        assert!(result.compressed.len() < input.len());
        assert!(result.compressed.contains("service"));
    }
}
