//! Race number -> driver / team / class keywords, from an entry list CSV.
//!
//! Port of `conrod/mapping.py`. The CSV needs a number column; every other
//! column becomes a keyword, so a two-column grid and a full entry list both
//! load with no configuration.

use crate::py;
use std::collections::HashMap;

/// Column names accepted as the number column, in order of preference.
const NUMBER_FIELDS: [&str; 7] = [
    "number",
    "no",
    "no.",
    "num",
    "car",
    "race number",
    "racenumber",
];

#[derive(Debug, Clone, Default, PartialEq)]
pub struct NumberMap {
    /// Canonical number -> (column, value) in column order.
    pub rows: HashMap<String, Vec<(String, String)>>,
}

impl NumberMap {
    /// Parse entry-list CSV text. Errors only when there is a header but no
    /// number column; an empty file is an empty map.
    pub fn parse(text: &str) -> Result<NumberMap, String> {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .flexible(true)
            .from_reader(text.as_bytes());
        let mut records = reader.records();
        let header: Vec<String> = match records.next() {
            Some(Ok(record)) => record.iter().map(str::to_string).collect(),
            Some(Err(e)) => return Err(e.to_string()),
            None => return Ok(NumberMap::default()),
        };
        let Some(number_field) = find_number_field(&header) else {
            return Err(format!("no 'number' column (found: {header:?})"));
        };

        let mut rows = HashMap::new();
        for record in records {
            let record = record.map_err(|e| e.to_string())?;
            if record.len() == 1 && record.get(0) == Some("") {
                continue; // a blank line, which Python's DictReader skips
            }
            // dict(zip(header, record)): a repeated column name keeps its
            // first position and its last value; surplus cells are dropped.
            let mut cells: Vec<(&str, &str)> = Vec::new();
            for (name, value) in header.iter().zip(record.iter()) {
                match cells.iter_mut().find(|(n, _)| *n == name.as_str()) {
                    Some(cell) => cell.1 = value,
                    None => cells.push((name, value)),
                }
            }
            let number = cells
                .iter()
                .find(|(n, _)| *n == number_field)
                .map_or("", |c| c.1);
            let key = canonical(number);
            if key.is_empty() {
                continue;
            }
            let mut row: Vec<(String, String)> = Vec::new();
            for (name, value) in cells {
                let value = value.trim();
                if name.is_empty() || name == number_field || value.is_empty() {
                    continue;
                }
                let name = name.trim().to_string();
                match row.iter_mut().find(|(n, _)| *n == name) {
                    Some(cell) => cell.1 = value.to_string(),
                    None => row.push((name, value.to_string())),
                }
            }
            rows.insert(key, row);
        }
        Ok(NumberMap { rows })
    }

    /// Keywords to write for one detected number.
    pub fn keywords_for(&self, number: &str, prefix: &str) -> Vec<String> {
        let mut out = vec![
            format!("{prefix}{number}"),
            format!("{prefix}#{number}"),
            format!("{prefix}Car {number}"),
        ];
        if let Some(row) = self.rows.get(&canonical(number)) {
            for (_, value) in row {
                // A cell may hold several values, e.g. "Repco;Castrol".
                for part in value.replace(';', ",").split(',') {
                    let part = part.trim();
                    if !part.is_empty() {
                        out.push(format!("{prefix}{part}"));
                    }
                }
            }
        }
        let mut seen = std::collections::HashSet::new();
        out.retain(|k| seen.insert(k.clone()));
        out
    }

    pub fn describe(&self, number: &str) -> String {
        self.rows
            .get(&canonical(number))
            .map(|row| {
                row.iter()
                    .map(|(_, v)| v.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default()
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

fn find_number_field(fields: &[String]) -> Option<&str> {
    NUMBER_FIELDS.iter().find_map(|candidate| {
        fields
            .iter()
            .find(|f| !f.is_empty() && f.trim().to_lowercase() == *candidate)
            .map(String::as_str)
    })
}

/// "#07 " and "7" are the same competitor as far as lookup is concerned.
pub fn canonical(value: &str) -> String {
    let digits: String = value.chars().filter(|&c| py::is_digit(c)).collect();
    let trimmed = digits.trim_start_matches('0');
    if trimmed.is_empty() {
        digits
    } else {
        trimmed.to_string()
    }
}
