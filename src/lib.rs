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

/// セルデータの内部ストレージ（値と色を分離して格納）
#[derive(Debug, Clone)]
struct CellData {
    value: CellValue,
    bg_color: Option<XlsColor>,
}

/// シート情報
///
/// セルは 2D フラットベクターに格納し、O(1) でアクセスする。
/// calamine と同等のアクセス速度を実現する。
#[derive(Debug, Clone)]
pub struct Sheet {
    pub name: String,
    /// 従来の互換用セルリスト
    pub cells: Vec<Cell>,
    /// 2D フラット配列（高速アクセス用）
    grid: Vec<Option<CellData>>,
    min_row: u16,
    min_col: u16,
    num_rows: usize,
    num_cols: usize,
}

impl Sheet {
    /// セルリストからシートを構築する（2Dグリッドも同時に構築）
    pub(crate) fn new(name: String, cells: Vec<Cell>) -> Self {
        if cells.is_empty() {
            return Self {
                name,
                cells,
                grid: Vec::new(),
                min_row: 0,
                min_col: 0,
                num_rows: 0,
                num_cols: 0,
            };
        }

        let min_row = cells.iter().map(|c| c.row).min().unwrap();
        let max_row = cells.iter().map(|c| c.row).max().unwrap();
        let min_col = cells.iter().map(|c| c.col).min().unwrap();
        let max_col = cells.iter().map(|c| c.col).max().unwrap();

        let num_rows = (max_row - min_row + 1) as usize;
        let num_cols = (max_col - min_col + 1) as usize;

        let mut grid = Vec::with_capacity(num_rows * num_cols);
        grid.resize_with(num_rows * num_cols, || None);

        for cell in &cells {
            let r = (cell.row - min_row) as usize;
            let c = (cell.col - min_col) as usize;
            let idx = r * num_cols + c;
            grid[idx] = Some(CellData {
                value: cell.value.clone(),
                bg_color: cell.background_color.clone(),
            });
        }

        Self {
            name,
            cells,
            grid,
            min_row,
            min_col,
            num_rows,
            num_cols,
        }
    }

    /// グリッドのインデックスを計算する
    #[inline(always)]
    fn grid_index(&self, row: u16, col: u16) -> Option<usize> {
        if self.num_cols == 0 {
            return None;
        }
        let r = row.wrapping_sub(self.min_row) as usize;
        let c = col.wrapping_sub(self.min_col) as usize;
        if r < self.num_rows && c < self.num_cols {
            Some(r * self.num_cols + c)
        } else {
            None
        }
    }

    /// 指定座標のセルを取得する — O(1)
    pub fn cell(&self, row: u16, col: u16) -> Option<&Cell> {
        // グリッドで存在チェックしてから cells リストを探す
        let idx = self.grid_index(row, col)?;
        if self.grid[idx].is_some() {
            self.cells.iter().find(|c| c.row == row && c.col == col)
        } else {
            None
        }
    }

    /// 指定座標のセル値への参照を取得する — O(1)、クローン不要
    #[inline]
    pub fn value_ref(&self, row: u16, col: u16) -> &CellValue {
        if let Some(idx) = self.grid_index(row, col) {
            if let Some(ref data) = self.grid[idx] {
                return &data.value;
            }
        }
        &CellValue::Empty
    }

    /// 指定座標のセル値を取得する（後方互換）
    pub fn value(&self, row: u16, col: u16) -> CellValue {
        self.value_ref(row, col).clone()
    }

    /// 指定座標の背景色を取得する — O(1)
    #[inline]
    pub fn background_color(&self, row: u16, col: u16) -> Option<&XlsColor> {
        if let Some(idx) = self.grid_index(row, col) {
            if let Some(ref data) = self.grid[idx] {
                return data.bg_color.as_ref();
            }
        }
        None
    }

    /// 使用されている行の最大値を取得する
    pub fn max_row(&self) -> Option<u16> {
        if self.num_rows == 0 {
            None
        } else {
            Some(self.min_row + self.num_rows as u16 - 1)
        }
    }

    /// 使用されている列の最大値を取得する
    pub fn max_col(&self) -> Option<u16> {
        if self.num_cols == 0 {
            None
        } else {
            Some(self.min_col + self.num_cols as u16 - 1)
        }
    }
}
