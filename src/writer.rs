use std::path::Path;

use crate::biff8::record_type;
use crate::color::{XlsColor, DEFAULT_PALETTE};
use crate::error::{Result, XlsError};
use crate::CellValue;

/// .xls ファイルのライター
pub struct XlsWriter {
    sheets: Vec<WriterSheet>,
    palette: [(u8, u8, u8); 56],
}

struct WriterSheet {
    name: String,
    cells: Vec<WriterCell>,
}

struct WriterCell {
    row: u16,
    col: u16,
    value: CellValue,
    background_color: Option<XlsColor>,
}

impl XlsWriter {
    /// 新しいライターを作成する
    pub fn new() -> Self {
        Self {
            sheets: Vec::new(),
            palette: DEFAULT_PALETTE,
        }
    }

    /// シートを追加する
    pub fn add_sheet(&mut self, name: &str) -> SheetWriter<'_> {
        self.sheets.push(WriterSheet {
            name: name.to_string(),
            cells: Vec::new(),
        });
        let idx = self.sheets.len() - 1;
        SheetWriter {
            writer: self,
            sheet_idx: idx,
        }
    }

    /// カスタムパレットカラーを設定する（インデックス 0〜55）
    pub fn set_palette_color(&mut self, index: usize, r: u8, g: u8, b: u8) {
        if index < 56 {
            self.palette[index] = (r, g, b);
        }
    }

    /// ファイルに書き出す
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let data = self.build()?;
        std::fs::write(path, &data)?;
        Ok(())
    }

    /// バイト列として書き出す
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        self.build()
    }

    fn build(&self) -> Result<Vec<u8>> {
        if self.sheets.is_empty() {
            return Err(XlsError::InvalidFormat(
                "At least one sheet is required".into(),
            ));
        }

        // XF レコード群を構築（色ごとにユニークな XF を作る）
        let mut xf_list: Vec<XfDef> = Vec::new();
        // デフォルト XF (index 0〜15 は標準スタイル)
        for _ in 0..16 {
            xf_list.push(XfDef {
                fill_pattern: 0,
                fg_color_index: 0x40,
                bg_color_index: 0x41,
            });
        }
        // セル用のデフォルト XF (index 16: 書式なし)
        xf_list.push(XfDef {
            fill_pattern: 0,
            fg_color_index: 0x40,
            bg_color_index: 0x41,
        });

        // SST 構築と XF マッピング
        let mut sst_strings: Vec<String> = Vec::new();
        let mut sst_map: std::collections::HashMap<String, u32> = std::collections::HashMap::new();

        // セルの色→XFインデックスのマッピング
        let mut color_xf_map: std::collections::HashMap<Option<(u8, u8, u8)>, u16> =
            std::collections::HashMap::new();
        color_xf_map.insert(None, 16); // 背景色なしのデフォルト

        // 全セルを走査して必要な XF と SST を準備
        for sheet in &self.sheets {
            for cell in &sheet.cells {
                // 色の登録
                let color_key = cell.background_color.as_ref().map(|c| (c.red, c.green, c.blue));
                if !color_xf_map.contains_key(&color_key) {
                    if let Some(ref color) = cell.background_color {
                        let color_index = self.find_or_add_palette_color(color);
                        let xf_idx = xf_list.len() as u16;
                        xf_list.push(XfDef {
                            fill_pattern: 1, // Solid
                            fg_color_index: color_index,
                            bg_color_index: 0x41,
                        });
                        color_xf_map.insert(color_key, xf_idx);
                    }
                }

                // 文字列の登録
                if let CellValue::String(ref s) = cell.value {
                    if !sst_map.contains_key(s) {
                        let idx = sst_strings.len() as u32;
                        sst_map.insert(s.clone(), idx);
                        sst_strings.push(s.clone());
                    }
                }
            }
        }

        // Workbook ストリームを構築
        let mut workbook = Vec::new();

        // --- Workbook Globals ---

        // BOF
        write_bof(&mut workbook, 0x0005); // Workbook Globals

        // CODEPAGE (UTF-16)
        write_record(&mut workbook, record_type::CODEPAGE, &1200u16.to_le_bytes());

        // DATEMODE (1900)
        write_record(&mut workbook, record_type::DATEMODE, &0u16.to_le_bytes());

        // PALETTE (カスタムパレットがデフォルトと異なる場合)
        if self.palette != DEFAULT_PALETTE {
            let mut palette_data = Vec::new();
            palette_data.extend_from_slice(&56u16.to_le_bytes());
            for &(r, g, b) in self.palette.iter() {
                palette_data.extend_from_slice(&[r, g, b, 0x00]);
            }
            write_record(&mut workbook, record_type::PALETTE, &palette_data);
        }

        // FONT レコード（最低5つ必要）
        for _ in 0..5 {
            write_default_font(&mut workbook);
        }

        // FORMAT レコード（必要に応じて）
        // デフォルトの "General" フォーマット
        write_format_record(&mut workbook, 0, "General");

        // STYLE レコード
        write_record(
            &mut workbook,
            record_type::STYLE,
            &[0x00, 0x80, 0x00, 0xFF],
        );

        // XF レコード
        for (i, xf) in xf_list.iter().enumerate() {
            write_xf_record(&mut workbook, xf, i < 16);
        }

        // BOUNDSHEET レコード（プレースホルダ、後で offset を書き換える）
        let mut boundsheet_positions: Vec<usize> = Vec::new();
        for sheet in &self.sheets {
            boundsheet_positions.push(workbook.len());
            let mut bs_data = Vec::new();
            bs_data.extend_from_slice(&0u32.to_le_bytes()); // offset placeholder
            bs_data.push(0x00); // visibility: visible
            bs_data.push(0x00); // sheet type: worksheet
            write_short_unicode_string(&mut bs_data, &sheet.name);
            write_record(&mut workbook, record_type::BOUNDSHEET, &bs_data);
        }

        // SST
        {
            let mut sst_data = Vec::new();
            let total_refs: u32 = self
                .sheets
                .iter()
                .flat_map(|s| &s.cells)
                .filter(|c| matches!(c.value, CellValue::String(_)))
                .count() as u32;
            sst_data.extend_from_slice(&total_refs.to_le_bytes());
            sst_data.extend_from_slice(&(sst_strings.len() as u32).to_le_bytes());
            for s in &sst_strings {
                write_unicode_string(&mut sst_data, s);
            }
            write_record(&mut workbook, record_type::SST, &sst_data);
        }

        // EOF (Globals)
        write_record(&mut workbook, record_type::EOF, &[]);

        // --- 各シート ---
        for (sheet_idx, sheet) in self.sheets.iter().enumerate() {
            let sheet_offset = workbook.len() as u32;

            // BoundSheet の offset を書き換え
            let bs_pos = boundsheet_positions[sheet_idx];
            // record header は 4 bytes, then data starts with offset (4 bytes)
            let offset_pos = bs_pos + 4;
            workbook[offset_pos..offset_pos + 4]
                .copy_from_slice(&sheet_offset.to_le_bytes());

            // BOF (Sheet)
            write_bof(&mut workbook, 0x0010);

            // DIMENSION
            let (min_row, max_row, min_col, max_col) = sheet_dimensions(&sheet.cells);
            let mut dim_data = Vec::new();
            dim_data.extend_from_slice(&(min_row as u32).to_le_bytes());
            dim_data.extend_from_slice(&((max_row + 1) as u32).to_le_bytes());
            dim_data.extend_from_slice(&min_col.to_le_bytes());
            dim_data.extend_from_slice(&(max_col + 1).to_le_bytes());
            dim_data.extend_from_slice(&0u16.to_le_bytes()); // reserved
            write_record(&mut workbook, record_type::DIMENSION, &dim_data);

            // セルデータ
            for cell in &sheet.cells {
                let color_key = cell
                    .background_color
                    .as_ref()
                    .map(|c| (c.red, c.green, c.blue));
                let xf_index = *color_xf_map.get(&color_key).unwrap_or(&16);

                match &cell.value {
                    CellValue::Empty => {
                        if cell.background_color.is_some() {
                            let mut rec = Vec::new();
                            rec.extend_from_slice(&cell.row.to_le_bytes());
                            rec.extend_from_slice(&cell.col.to_le_bytes());
                            rec.extend_from_slice(&xf_index.to_le_bytes());
                            write_record(&mut workbook, record_type::BLANK, &rec);
                        }
                    }
                    CellValue::Number(v) => {
                        let mut rec = Vec::new();
                        rec.extend_from_slice(&cell.row.to_le_bytes());
                        rec.extend_from_slice(&cell.col.to_le_bytes());
                        rec.extend_from_slice(&xf_index.to_le_bytes());
                        rec.extend_from_slice(&v.to_le_bytes());
                        write_record(&mut workbook, record_type::NUMBER, &rec);
                    }
                    CellValue::String(s) => {
                        let sst_idx = sst_map.get(s).copied().unwrap_or(0);
                        let mut rec = Vec::new();
                        rec.extend_from_slice(&cell.row.to_le_bytes());
                        rec.extend_from_slice(&cell.col.to_le_bytes());
                        rec.extend_from_slice(&xf_index.to_le_bytes());
                        rec.extend_from_slice(&sst_idx.to_le_bytes());
                        write_record(&mut workbook, record_type::LABEL_SST, &rec);
                    }
                    CellValue::Bool(b) => {
                        let mut rec = Vec::new();
                        rec.extend_from_slice(&cell.row.to_le_bytes());
                        rec.extend_from_slice(&cell.col.to_le_bytes());
                        rec.extend_from_slice(&xf_index.to_le_bytes());
                        rec.push(if *b { 1 } else { 0 });
                        rec.push(0); // is_error = false
                        write_record(&mut workbook, record_type::BOOLERR, &rec);
                    }
                    CellValue::Error(e) => {
                        let mut rec = Vec::new();
                        rec.extend_from_slice(&cell.row.to_le_bytes());
                        rec.extend_from_slice(&cell.col.to_le_bytes());
                        rec.extend_from_slice(&xf_index.to_le_bytes());
                        rec.push(*e);
                        rec.push(1); // is_error = true
                        write_record(&mut workbook, record_type::BOOLERR, &rec);
                    }
                }
            }

            // WINDOW2
            let window2_data: [u8; 18] = [
                0x06, 0x08, // options: show gridlines, show headers
                0x00, 0x00, // first visible row
                0x00, 0x00, // first visible col
                0x00, 0x00, 0x00, 0x00, // gridline color
                0x00, 0x00, // page break zoom
                0x00, 0x00, // normal zoom
                0x00, 0x00, 0x00, 0x00, // reserved
            ];
            write_record(&mut workbook, record_type::WINDOW2, &window2_data);

            // EOF (Sheet)
            write_record(&mut workbook, record_type::EOF, &[]);
        }

        // OLE2 コンテナに包む
        let ole2_data = wrap_in_ole2(&workbook)?;
        Ok(ole2_data)
    }

    fn find_or_add_palette_color(&self, color: &XlsColor) -> u16 {
        // パレット内を検索
        for (i, &(r, g, b)) in self.palette.iter().enumerate() {
            if r == color.red && g == color.green && b == color.blue {
                return (i + 0x08) as u16;
            }
        }
        // 見つからない場合は最も近い色を使う
        let mut best_idx = 0;
        let mut best_dist = u32::MAX;
        for (i, &(r, g, b)) in self.palette.iter().enumerate() {
            let dr = (r as i32 - color.red as i32).unsigned_abs();
            let dg = (g as i32 - color.green as i32).unsigned_abs();
            let db = (b as i32 - color.blue as i32).unsigned_abs();
            let dist = dr * dr + dg * dg + db * db;
            if dist < best_dist {
                best_dist = dist;
                best_idx = i;
            }
        }
        (best_idx + 0x08) as u16
    }
}

impl Default for XlsWriter {
    fn default() -> Self {
        Self::new()
    }
}

/// シート書き込み用のヘルパー
pub struct SheetWriter<'a> {
    writer: &'a mut XlsWriter,
    sheet_idx: usize,
}

impl<'a> SheetWriter<'a> {
    /// セルに値を書き込む
    pub fn write_cell(&mut self, row: u16, col: u16, value: CellValue) -> &mut Self {
        self.writer.sheets[self.sheet_idx].cells.push(WriterCell {
            row,
            col,
            value,
            background_color: None,
        });
        self
    }

    /// セルに値と背景色を書き込む
    pub fn write_cell_with_color(
        &mut self,
        row: u16,
        col: u16,
        value: CellValue,
        bg_color: XlsColor,
    ) -> &mut Self {
        self.writer.sheets[self.sheet_idx].cells.push(WriterCell {
            row,
            col,
            value,
            background_color: Some(bg_color),
        });
        self
    }

    /// 数値を書き込む
    pub fn write_number(&mut self, row: u16, col: u16, value: f64) -> &mut Self {
        self.write_cell(row, col, CellValue::Number(value))
    }

    /// 文字列を書き込む
    pub fn write_string(&mut self, row: u16, col: u16, value: &str) -> &mut Self {
        self.write_cell(row, col, CellValue::String(value.to_string()))
    }

    /// 背景色のみ設定する（空セル）
    pub fn write_color(&mut self, row: u16, col: u16, bg_color: XlsColor) -> &mut Self {
        self.write_cell_with_color(row, col, CellValue::Empty, bg_color)
    }
}

// --- 内部ヘルパー関数 ---

struct XfDef {
    fill_pattern: u8,
    fg_color_index: u16,
    bg_color_index: u16,
}

fn write_record(buf: &mut Vec<u8>, rec_type: u16, data: &[u8]) {
    buf.extend_from_slice(&rec_type.to_le_bytes());
    buf.extend_from_slice(&(data.len() as u16).to_le_bytes());
    buf.extend_from_slice(data);
}

fn write_bof(buf: &mut Vec<u8>, bof_type: u16) {
    let mut data = Vec::new();
    data.extend_from_slice(&0x0600u16.to_le_bytes()); // BIFF version
    data.extend_from_slice(&bof_type.to_le_bytes()); // type
    data.extend_from_slice(&0x0DBB_u16.to_le_bytes()); // build ID
    data.extend_from_slice(&0x07CC_u16.to_le_bytes()); // build year
    data.extend_from_slice(&0u32.to_le_bytes()); // file history flags
    data.extend_from_slice(&0x06u32.to_le_bytes()); // lowest BIFF version
    write_record(buf, record_type::BOF, &data);
}

fn write_default_font(buf: &mut Vec<u8>) {
    let mut data = Vec::new();
    data.extend_from_slice(&200u16.to_le_bytes()); // height (10pt * 20)
    data.extend_from_slice(&0u16.to_le_bytes()); // options
    data.extend_from_slice(&0x7FFF_u16.to_le_bytes()); // color index (automatic)
    data.extend_from_slice(&400u16.to_le_bytes()); // weight (normal)
    data.extend_from_slice(&0u16.to_le_bytes()); // escapement
    data.push(0); // underline
    data.push(0); // family
    data.push(0); // charset
    data.push(0); // reserved
    // Font name: "Arial"
    let font_name = "Arial";
    data.push(font_name.len() as u8);
    data.push(0x00); // compressed (Latin1)
    data.extend_from_slice(font_name.as_bytes());
    write_record(buf, record_type::FONT, &data);
}

fn write_format_record(buf: &mut Vec<u8>, index: u16, format_str: &str) {
    let mut data = Vec::new();
    data.extend_from_slice(&index.to_le_bytes());
    write_unicode_string(&mut data, format_str);
    write_record(buf, record_type::FORMAT, &data);
}

fn write_xf_record(buf: &mut Vec<u8>, xf: &XfDef, is_style: bool) {
    let mut data = vec![0u8; 20];

    // font index = 0
    data[0] = 0;
    data[1] = 0;

    // format index = 0
    data[2] = 0;
    data[3] = 0;

    // type/protection: bit 2 = style flag
    if is_style {
        data[4] = 0x04; // style XF
    } else {
        data[4] = 0x00; // cell XF
    }
    data[5] = 0x00;

    // alignment: general, no wrap
    data[6] = 0x20; // general horizontal alignment
    data[7] = 0x00;

    // rotation
    data[8] = 0x00;

    // text properties
    data[9] = 0x00;

    // used attributes
    data[10] = 0x00;
    data[11] = 0x00;

    // borders (4 bytes)
    data[12] = 0x00;
    data[13] = 0x00;
    data[14] = 0x00;
    data[15] = 0x00;

    // colors: bits 0-6 = fg, bits 7-13 = bg
    let color_word: u16 = (xf.fg_color_index & 0x7F) | ((xf.bg_color_index & 0x7F) << 7);
    data[16] = (color_word & 0xFF) as u8;
    data[17] = (color_word >> 8) as u8;

    // pattern: bits 10-15 = fill pattern
    let pattern_word: u16 = (xf.fill_pattern as u16) << 10;
    data[18] = (pattern_word & 0xFF) as u8;
    data[19] = (pattern_word >> 8) as u8;

    write_record(buf, record_type::XF, &data);
}

fn write_unicode_string(buf: &mut Vec<u8>, s: &str) {
    let chars: Vec<u16> = s.encode_utf16().collect();
    let is_ascii = s.is_ascii();

    buf.extend_from_slice(&(chars.len() as u16).to_le_bytes());

    if is_ascii {
        buf.push(0x00); // compressed
        for &ch in &chars {
            buf.push(ch as u8);
        }
    } else {
        buf.push(0x01); // uncompressed (UTF-16LE)
        for &ch in &chars {
            buf.extend_from_slice(&ch.to_le_bytes());
        }
    }
}

fn write_short_unicode_string(buf: &mut Vec<u8>, s: &str) {
    let chars: Vec<u16> = s.encode_utf16().collect();
    let is_ascii = s.is_ascii();

    buf.push(chars.len() as u8);

    if is_ascii {
        buf.push(0x00); // compressed
        for &ch in &chars {
            buf.push(ch as u8);
        }
    } else {
        buf.push(0x01); // uncompressed
        for &ch in &chars {
            buf.extend_from_slice(&ch.to_le_bytes());
        }
    }
}

fn sheet_dimensions(cells: &[WriterCell]) -> (u16, u16, u16, u16) {
    if cells.is_empty() {
        return (0, 0, 0, 0);
    }
    let min_row = cells.iter().map(|c| c.row).min().unwrap_or(0);
    let max_row = cells.iter().map(|c| c.row).max().unwrap_or(0);
    let min_col = cells.iter().map(|c| c.col).min().unwrap_or(0);
    let max_col = cells.iter().map(|c| c.col).max().unwrap_or(0);
    (min_row, max_row, min_col, max_col)
}

/// Workbook ストリームを OLE2 コンテナに包む
fn wrap_in_ole2(workbook_data: &[u8]) -> Result<Vec<u8>> {
    let sector_size: usize = 512;
    let _mini_sector_size: usize = 64;
    let mini_stream_cutoff: usize = 0x1000;

    // Workbook が mini stream cutoff 以下ならミニストリームに格納する必要があるが
    // 簡略化のため常に通常セクターに格納する
    let workbook_sectors = (workbook_data.len() + sector_size - 1) / sector_size;

    // ディレクトリエントリ: Root Entry + Workbook
    // ディレクトリは1セクターに収まる (128 bytes * 4 entries max per sector)
    let dir_sectors = 1;

    // FAT に必要なエントリ数
    // Sectors: workbook_sectors + dir_sectors + FAT sectors
    // FAT は自分自身のセクターも含む必要がある
    let entries_per_fat_sector = sector_size / 4;
    let total_data_sectors = workbook_sectors + dir_sectors;
    let fat_sectors = (total_data_sectors + entries_per_fat_sector) / entries_per_fat_sector; // +1 for FAT sector itself
    let total_sectors = total_data_sectors + fat_sectors;

    // ファイル全体のサイズ
    let file_size = 512 + total_sectors * sector_size; // header + sectors
    let mut output = vec![0u8; file_size];

    // --- OLE2 ヘッダ (512 bytes) ---
    let header = &mut output[0..512];

    // Magic number
    header[0..8].copy_from_slice(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]);

    // Minor version
    header[24..26].copy_from_slice(&0x003E_u16.to_le_bytes());
    // Major version (3 = sector size 512)
    header[26..28].copy_from_slice(&0x0003_u16.to_le_bytes());
    // Byte order (little-endian)
    header[28..30].copy_from_slice(&0xFFFE_u16.to_le_bytes());
    // Sector size power (9 = 512)
    header[30..32].copy_from_slice(&9u16.to_le_bytes());
    // Mini sector size power (6 = 64)
    header[32..34].copy_from_slice(&6u16.to_le_bytes());

    // Total directory sectors (0 for v3)
    header[40..44].copy_from_slice(&0u32.to_le_bytes());
    // Total FAT sectors
    header[44..48].copy_from_slice(&(fat_sectors as u32).to_le_bytes());
    // First directory sector
    let dir_sector_id = workbook_sectors;
    header[48..52].copy_from_slice(&(dir_sector_id as u32).to_le_bytes());

    // Mini stream cutoff size
    header[56..60].copy_from_slice(&(mini_stream_cutoff as u32).to_le_bytes());
    // First mini FAT sector (none)
    header[60..64].copy_from_slice(&0xFFFFFFFE_u32.to_le_bytes());
    // Number of mini FAT sectors
    header[64..68].copy_from_slice(&0u32.to_le_bytes());
    // First DIFAT sector (none)
    header[68..72].copy_from_slice(&0xFFFFFFFE_u32.to_le_bytes());
    // Number of DIFAT sectors
    header[72..76].copy_from_slice(&0u32.to_le_bytes());

    // DIFAT array (109 entries, first one points to FAT sector)
    let fat_sector_id = workbook_sectors + dir_sectors;
    header[76..80].copy_from_slice(&(fat_sector_id as u32).to_le_bytes());
    // 残りの DIFAT エントリを 0xFFFFFFFF (free) で埋める
    for i in 1..109 {
        let off = 76 + i * 4;
        if off + 4 <= 512 {
            header[off..off + 4].copy_from_slice(&0xFFFFFFFF_u32.to_le_bytes());
        }
    }

    // --- Workbook データセクター ---
    let wb_offset = 512;
    let copy_len = workbook_data.len().min(workbook_sectors * sector_size);
    output[wb_offset..wb_offset + copy_len].copy_from_slice(&workbook_data[..copy_len]);

    // --- ディレクトリセクター ---
    let dir_offset = 512 + dir_sector_id * sector_size;

    // Root Entry (128 bytes)
    {
        let entry = &mut output[dir_offset..dir_offset + 128];
        // Name: "Root Entry" in UTF-16LE
        let name = "Root Entry";
        for (i, ch) in name.encode_utf16().enumerate() {
            let off = i * 2;
            entry[off..off + 2].copy_from_slice(&ch.to_le_bytes());
        }
        // Name size in bytes (including null terminator)
        let name_size = (name.encode_utf16().count() + 1) * 2;
        entry[64..66].copy_from_slice(&(name_size as u16).to_le_bytes());
        // Object type: Root Storage
        entry[66] = 0x05;
        // Color: Black (0 = red, 1 = black)
        entry[67] = 0x01;
        // Left/right/child sibling
        entry[68..72].copy_from_slice(&0xFFFFFFFF_u32.to_le_bytes()); // left
        entry[72..76].copy_from_slice(&0xFFFFFFFF_u32.to_le_bytes()); // right
        entry[76..80].copy_from_slice(&1u32.to_le_bytes()); // child (Workbook entry)
        // Start sector (mini stream - not used)
        entry[116..120].copy_from_slice(&0xFFFFFFFE_u32.to_le_bytes());
        // Size
        entry[120..124].copy_from_slice(&0u32.to_le_bytes());
    }

    // Workbook Entry (128 bytes)
    {
        let entry = &mut output[dir_offset + 128..dir_offset + 256];
        // Name: "Workbook" in UTF-16LE
        let name = "Workbook";
        for (i, ch) in name.encode_utf16().enumerate() {
            let off = i * 2;
            entry[off..off + 2].copy_from_slice(&ch.to_le_bytes());
        }
        let name_size = (name.encode_utf16().count() + 1) * 2;
        entry[64..66].copy_from_slice(&(name_size as u16).to_le_bytes());
        // Object type: Stream
        entry[66] = 0x02;
        // Color: Black
        entry[67] = 0x01;
        // Siblings
        entry[68..72].copy_from_slice(&0xFFFFFFFF_u32.to_le_bytes()); // left
        entry[72..76].copy_from_slice(&0xFFFFFFFF_u32.to_le_bytes()); // right
        entry[76..80].copy_from_slice(&0xFFFFFFFF_u32.to_le_bytes()); // child
        // Start sector
        entry[116..120].copy_from_slice(&0u32.to_le_bytes()); // sector 0
        // Size
        entry[120..124].copy_from_slice(&(workbook_data.len() as u32).to_le_bytes());
    }

    // 残りのディレクトリエントリを空にする
    for i in 2..4 {
        let entry = &mut output[dir_offset + i * 128..dir_offset + (i + 1) * 128];
        // 空エントリ
        entry[66] = 0x00; // Unknown type
        entry[68..72].copy_from_slice(&0xFFFFFFFF_u32.to_le_bytes());
        entry[72..76].copy_from_slice(&0xFFFFFFFF_u32.to_le_bytes());
        entry[76..80].copy_from_slice(&0xFFFFFFFF_u32.to_le_bytes());
    }

    // --- FAT セクター ---
    let fat_offset = 512 + fat_sector_id * sector_size;
    // FAT初期化: 全エントリを0xFFFFFFFF (free)
    for i in 0..entries_per_fat_sector {
        let off = fat_offset + i * 4;
        if off + 4 <= output.len() {
            output[off..off + 4].copy_from_slice(&0xFFFFFFFF_u32.to_le_bytes());
        }
    }

    // Workbook セクターチェーン
    for i in 0..workbook_sectors {
        let off = fat_offset + i * 4;
        if i + 1 < workbook_sectors {
            output[off..off + 4].copy_from_slice(&((i + 1) as u32).to_le_bytes());
        } else {
            output[off..off + 4].copy_from_slice(&0xFFFFFFFE_u32.to_le_bytes()); // end of chain
        }
    }

    // ディレクトリセクター
    {
        let off = fat_offset + dir_sector_id * 4;
        if off + 4 <= output.len() {
            output[off..off + 4].copy_from_slice(&0xFFFFFFFE_u32.to_le_bytes()); // end of chain
        }
    }

    // FAT セクター自身のマーク (0xFFFFFFFD = FAT sector)
    {
        let off = fat_offset + fat_sector_id * 4;
        if off + 4 <= output.len() {
            output[off..off + 4].copy_from_slice(&0xFFFFFFFD_u32.to_le_bytes());
        }
    }

    Ok(output)
}
