//! # xls_reader
//!
//! .xls (BIFF8) ファイルの読み書きライブラリ。
//! セルの値とセル背景色の読み書きをサポートする。
//! 条件付き書式には対応しない。

mod biff8;
mod color;
mod error;
mod reader;
mod writer;

pub use color::XlsColor;
pub use error::XlsError;
pub use reader::XlsReader;
pub use writer::XlsWriter;

/// セルの値を表す列挙型
#[derive(Debug, Clone, PartialEq)]
pub enum CellValue {
    Empty,
    Number(f64),
    String(String),
    Bool(bool),
    Error(u8),
}

impl CellValue {
    /// 数値として取得する
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            CellValue::Number(v) => Some(*v),
            _ => None,
        }
    }

    /// 文字列として取得する
    pub fn as_str(&self) -> Option<&str> {
        match self {
            CellValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// 空かどうか判定する
    pub fn is_empty(&self) -> bool {
        matches!(self, CellValue::Empty)
    }
}

impl std::fmt::Display for CellValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CellValue::Empty => write!(f, ""),
            CellValue::Number(v) => write!(f, "{v}"),
            CellValue::String(s) => write!(f, "{s}"),
            CellValue::Bool(b) => write!(f, "{b}"),
            CellValue::Error(e) => write!(f, "#ERR({e})"),
        }
    }
}

/// セル情報（値 + 書式）
#[derive(Debug, Clone)]
pub struct Cell {
    pub row: u16,
    pub col: u16,
    pub value: CellValue,
    pub background_color: Option<XlsColor>,
}

/// シート情報
#[derive(Debug, Clone)]
pub struct Sheet {
    pub name: String,
    pub cells: Vec<Cell>,
}

impl Sheet {
    /// 指定座標のセルを取得する
    pub fn cell(&self, row: u16, col: u16) -> Option<&Cell> {
        self.cells.iter().find(|c| c.row == row && c.col == col)
    }

    /// 指定座標のセル値を取得する
    pub fn value(&self, row: u16, col: u16) -> CellValue {
        self.cell(row, col)
            .map(|c| c.value.clone())
            .unwrap_or(CellValue::Empty)
    }

    /// 指定座標の背景色を取得する
    pub fn background_color(&self, row: u16, col: u16) -> Option<XlsColor> {
        self.cell(row, col).and_then(|c| c.background_color.clone())
    }

    /// 使用されている行の最大値を取得する
    pub fn max_row(&self) -> Option<u16> {
        self.cells.iter().map(|c| c.row).max()
    }

    /// 使用されている列の最大値を取得する
    pub fn max_col(&self) -> Option<u16> {
        self.cells.iter().map(|c| c.col).max()
    }
}
