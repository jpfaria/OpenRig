//! Responsibility: turns an edited capture grid into manifest parameters.
//!
//! A grid is accepted only when every capture of the package has one row,
//! and the cells pick exactly one capture per setting. Capture files, their
//! levels and their noise gates are kept as they are.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use plugin_loader::manifest::{GridCapture, GridParameter, ParameterValue};

use super::grid_read::cell_text;
use super::grid_types::{ColumnKind, EditorGrid};

#[derive(Debug, Clone, PartialEq)]
pub enum GridError {
    EmptyColumnName,
    /// A column name holds a dot or is a name the engine already uses.
    InvalidColumnName(String),
    DuplicateColumn(String),
    /// A row has more or fewer cells than there are columns.
    CellCount(PathBuf),
    EmptyCell(PathBuf, String),
    NotANumber(PathBuf, String),
    NotASwitch(PathBuf, String),
    /// Two captures share every cell, so no setting picks one of them.
    DuplicateSetting(PathBuf, PathBuf),
    /// A row names a file the package does not hold.
    UnknownFile(PathBuf),
    /// A capture of the package has no row.
    MissingFile(PathBuf),
    DuplicateFile(PathBuf),
}

impl std::fmt::Display for GridError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyColumnName => write!(f, "every parameter needs a name"),
            Self::InvalidColumnName(name) => write!(f, "`{name}` cannot name a parameter"),
            Self::DuplicateColumn(name) => write!(f, "two parameters are named `{name}`"),
            Self::CellCount(file) => {
                write!(f, "{} needs one value per parameter", file.display())
            }
            Self::EmptyCell(file, column) => {
                write!(f, "{} has no value for `{column}`", file.display())
            }
            Self::NotANumber(file, cell) => {
                write!(f, "{}: `{cell}` is not a number", file.display())
            }
            Self::NotASwitch(file, cell) => {
                write!(f, "{}: `{cell}` is not true or false", file.display())
            }
            Self::DuplicateSetting(a, b) => write!(
                f,
                "{} and {} have the same setting",
                a.display(),
                b.display()
            ),
            Self::UnknownFile(file) => write!(f, "{} is not in this plugin", file.display()),
            Self::MissingFile(file) => write!(f, "{} has no row", file.display()),
            Self::DuplicateFile(file) => write!(f, "{} has two rows", file.display()),
        }
    }
}

impl std::error::Error for GridError {}

/// The parameters and captures `grid` describes for a package whose
/// captures are `captures`. `previous` keeps the order of choices that
/// already existed; `reserved` are names the engine itself uses.
pub fn apply_grid(
    grid: &EditorGrid,
    previous: &[GridParameter],
    captures: &[GridCapture],
    reserved: &[String],
) -> Result<(Vec<GridParameter>, Vec<GridCapture>), GridError> {
    check_columns(grid, reserved)?;
    let rows = row_values(grid)?;
    let mut by_file: BTreeMap<&PathBuf, &Vec<ParameterValue>> = BTreeMap::new();
    for (row, values) in grid.rows.iter().zip(&rows) {
        if !captures.iter().any(|c| c.file == row.file) {
            return Err(GridError::UnknownFile(row.file.clone()));
        }
        if by_file.insert(&row.file, values).is_some() {
            return Err(GridError::DuplicateFile(row.file.clone()));
        }
    }
    if let Some(missing) = captures.iter().find(|c| !by_file.contains_key(&c.file)) {
        return Err(GridError::MissingFile(missing.file.clone()));
    }
    check_distinct(grid, &rows)?;
    let parameters = parameters(grid, &rows, previous);
    let captures = captures
        .iter()
        .map(|capture| GridCapture {
            values: grid
                .columns
                .iter()
                .zip(by_file[&capture.file].iter())
                .map(|(column, value)| (column.name.clone(), value.clone()))
                .collect(),
            ..capture.clone()
        })
        .collect();
    Ok((parameters, captures))
}

/// The parameters of a new plugin whose captures are the grid's rows.
pub fn grid_parameters(
    grid: &EditorGrid,
    reserved: &[String],
) -> Result<(Vec<GridParameter>, Vec<BTreeMap<String, ParameterValue>>), GridError> {
    check_columns(grid, reserved)?;
    let rows = row_values(grid)?;
    let mut files = BTreeSet::new();
    for row in &grid.rows {
        if !files.insert(&row.file) {
            return Err(GridError::DuplicateFile(row.file.clone()));
        }
    }
    check_distinct(grid, &rows)?;
    let parameters = parameters(grid, &rows, &[]);
    let values = rows
        .into_iter()
        .map(|values| {
            grid.columns
                .iter()
                .map(|c| c.name.clone())
                .zip(values)
                .collect()
        })
        .collect();
    Ok((parameters, values))
}

fn check_columns(grid: &EditorGrid, reserved: &[String]) -> Result<(), GridError> {
    let mut seen = BTreeSet::new();
    for column in &grid.columns {
        let name = column.name.trim();
        if name.is_empty() {
            return Err(GridError::EmptyColumnName);
        }
        if name != column.name || name.contains('.') || reserved.iter().any(|r| r == name) {
            return Err(GridError::InvalidColumnName(column.name.clone()));
        }
        if !seen.insert(name) {
            return Err(GridError::DuplicateColumn(column.name.clone()));
        }
    }
    Ok(())
}

fn row_values(grid: &EditorGrid) -> Result<Vec<Vec<ParameterValue>>, GridError> {
    grid.rows
        .iter()
        .map(|row| {
            if row.cells.len() != grid.columns.len() {
                return Err(GridError::CellCount(row.file.clone()));
            }
            grid.columns
                .iter()
                .zip(&row.cells)
                .map(|(column, cell)| {
                    let cell = cell.trim();
                    if cell.is_empty() {
                        return Err(GridError::EmptyCell(row.file.clone(), column.name.clone()));
                    }
                    match column.kind {
                        ColumnKind::Knob => cell
                            .parse::<f64>()
                            .ok()
                            .filter(|n| n.is_finite())
                            .map(ParameterValue::Number)
                            .ok_or_else(|| GridError::NotANumber(row.file.clone(), cell.into())),
                        ColumnKind::Switch => match cell {
                            "true" => Ok(ParameterValue::Bool(true)),
                            "false" => Ok(ParameterValue::Bool(false)),
                            _ => Err(GridError::NotASwitch(row.file.clone(), cell.into())),
                        },
                        ColumnKind::Choice => Ok(ParameterValue::Text(cell.into())),
                    }
                })
                .collect()
        })
        .collect()
}

fn check_distinct(grid: &EditorGrid, rows: &[Vec<ParameterValue>]) -> Result<(), GridError> {
    for (i, a) in rows.iter().enumerate() {
        if let Some(j) = rows[..i].iter().position(|b| b == a) {
            return Err(GridError::DuplicateSetting(
                grid.rows[j].file.clone(),
                grid.rows[i].file.clone(),
            ));
        }
    }
    Ok(())
}

/// One parameter per column, holding only the values some capture uses:
/// knobs low to high, switches off then on, choices in their previous
/// order and new ones as they first appear.
fn parameters(
    grid: &EditorGrid,
    rows: &[Vec<ParameterValue>],
    previous: &[GridParameter],
) -> Vec<GridParameter> {
    grid.columns
        .iter()
        .enumerate()
        .map(|(i, column)| {
            let mut used: Vec<ParameterValue> = Vec::new();
            for value in rows.iter().map(|r| &r[i]) {
                if !used.contains(value) {
                    used.push(value.clone());
                }
            }
            match column.kind {
                ColumnKind::Knob | ColumnKind::Switch => used.sort_by(compare),
                ColumnKind::Choice => {
                    let before: Vec<String> = previous
                        .iter()
                        .find(|p| p.name == column.name)
                        .map(|p| p.values.iter().map(cell_text).collect())
                        .unwrap_or_default();
                    let rank = |v: &ParameterValue| {
                        let text = cell_text(v);
                        before.iter().position(|b| *b == text).unwrap_or(usize::MAX)
                    };
                    used.sort_by_key(rank);
                }
            }
            GridParameter {
                name: column.name.clone(),
                display_name: Some(
                    column
                        .display_name
                        .clone()
                        .filter(|d| !d.trim().is_empty())
                        .unwrap_or_else(|| capitalize_first(&column.name)),
                ),
                values: used,
            }
        })
        .collect()
}

fn compare(a: &ParameterValue, b: &ParameterValue) -> std::cmp::Ordering {
    match (a, b) {
        (ParameterValue::Number(a), ParameterValue::Number(b)) => a.total_cmp(b),
        (ParameterValue::Bool(a), ParameterValue::Bool(b)) => a.cmp(b),
        _ => std::cmp::Ordering::Equal,
    }
}

fn capitalize_first(name: &str) -> String {
    let words = name.replace('_', " ");
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
