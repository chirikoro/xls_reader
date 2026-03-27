use std::path::Path;

use crate::biff8::{self, record_type, XfRecord};
use crate::color::{self, XlsColor, DEFAULT_PALETTE};
use crate::error::{Result, XlsError};
use crate::{Cell, CellValue, Sheet};

/// .xls ファイルのリーダー
pub struct XlsReader {
    sheets: Vec<Sheet>,
    palette: [(u8, u8, u8); 56],
}

/// シートの位置情報（BoundSheet レコードから）
struct BoundSheetInfo {
    offset: u32,
    name: String,
}

impl XlsReader {
    /// ファイルパスから .xls ファイルを読み込む
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let data = std::fs::read(path)?;
        Self::from_bytes(&data)
    }

    /// バイト列から .xls データを読み込む
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        // .xls は OLE2 (Compound File Binary) コンテナに格納されている
        // まず OLE2 コンテナから Workbook ストリームを取り出す
        let workbook_data = extract_workbook_stream(data)?;

        Self::parse_workbook(&workbook_data)
    }

    /// ワークブックのバイナリストリームをパースする
    fn parse_workbook(data: &[u8]) -> Result<Self> {
        let mut palette = DEFAULT_PALETTE;
        let mut xf_records: Vec<XfRecord> = Vec::new();
        let mut sst_strings: Vec<String> = Vec::new();
        let mut bound_sheets: Vec<BoundSheetInfo> = Vec::new();
        let mut pos = 0;

        // Phase 1: Workbook Globals ストリームをパース
        // BOF を確認
        if pos + 4 > data.len() {
            return Err(XlsError::InvalidFormat("File too short".into()));
        }

        let rec_type = u16::from_le_bytes([data[pos], data[pos + 1]]);
        let rec_len = u16::from_le_bytes([data[pos + 2], data[pos + 3]]) as usize;

        if rec_type != record_type::BOF {
            return Err(XlsError::InvalidFormat("Missing BOF record".into()));
        }
        if pos + 4 + rec_len > data.len() || rec_len < 4 {
            return Err(XlsError::InvalidFormat("Invalid BOF record".into()));
        }

        let version = u16::from_le_bytes([data[pos + 4], data[pos + 5]]);
        let bof_type = u16::from_le_bytes([data[pos + 6], data[pos + 7]]);

        if version != biff8::BIFF8_VERSION {
            return Err(XlsError::UnsupportedVersion);
        }
        if bof_type != biff8::bof_type::WORKBOOK_GLOBALS {
            return Err(XlsError::InvalidFormat(
                "Expected Workbook Globals BOF".into(),
            ));
        }

        pos += 4 + rec_len;

        // Globals 部分のレコードを読む

        while pos + 4 <= data.len() {
            let rec_type = u16::from_le_bytes([data[pos], data[pos + 1]]);
            let rec_len = u16::from_le_bytes([data[pos + 2], data[pos + 3]]) as usize;


            if pos + 4 + rec_len > data.len() {

                break;
            }

            let rec_data = &data[pos + 4..pos + 4 + rec_len];

            match rec_type {
                record_type::PALETTE => {
                    parse_palette(rec_data, &mut palette);
                }
                record_type::XF => {
                    if let Some(xf) = biff8::parse_xf_record(rec_data) {
                        xf_records.push(xf);
                    }
                }
                record_type::SST => {
                    // SST may be followed by CONTINUE records
                    let mut sst_data = rec_data.to_vec();
                    let mut next_pos = pos + 4 + rec_len;

                    // Collect CONTINUE records
                    while next_pos + 4 <= data.len() {
                        let next_type =
                            u16::from_le_bytes([data[next_pos], data[next_pos + 1]]);
                        let next_len =
                            u16::from_le_bytes([data[next_pos + 2], data[next_pos + 3]])
                                as usize;
                        if next_type == record_type::CONTINUE
                            && next_pos + 4 + next_len <= data.len()
                        {
                            sst_data
                                .extend_from_slice(&data[next_pos + 4..next_pos + 4 + next_len]);
                            next_pos += 4 + next_len;
                        } else {
                            break;
                        }
                    }

                    sst_strings = parse_sst(&sst_data);
                    pos = next_pos;
                    continue;
                }
                record_type::BOUNDSHEET => {
                    if rec_data.len() >= 8 {
                        let sheet_offset = u32::from_le_bytes([
                            rec_data[0],
                            rec_data[1],
                            rec_data[2],
                            rec_data[3],
                        ]);
                        // rec_data[4] = visibility, rec_data[5] = sheet type
                        if let Some((name, _)) =
                            biff8::read_short_unicode_string(rec_data, 6)
                        {
                            bound_sheets.push(BoundSheetInfo {
                                offset: sheet_offset,
                                name,
                            });
                        }
                    }
                }
                record_type::EOF => {
                    break;
                }
                _ => {}
            }

            pos += 4 + rec_len;
        }

        // Phase 2: 各シートをパース
        let mut sheets = Vec::new();
        for sheet_info in &bound_sheets {
            let sheet = parse_sheet(
                data,
                sheet_info.offset as usize,
                &sheet_info.name,
                &sst_strings,
                &xf_records,
                &palette,
            )?;
            sheets.push(sheet);
        }

        Ok(Self { sheets, palette })
    }

    /// シートの一覧を取得する
    pub fn sheet_names(&self) -> Vec<&str> {
        self.sheets.iter().map(|s| s.name.as_str()).collect()
    }

    /// シート数を取得する
    pub fn sheet_count(&self) -> usize {
        self.sheets.len()
    }

    /// インデックスでシートを取得する
    pub fn sheet_by_index(&self, index: usize) -> Option<&Sheet> {
        self.sheets.get(index)
    }

    /// 名前でシートを取得する
    pub fn sheet_by_name(&self, name: &str) -> Option<&Sheet> {
        self.sheets.iter().find(|s| s.name == name)
    }

    /// 全シートを取得する
    pub fn sheets(&self) -> &[Sheet] {
        &self.sheets
    }

    /// カスタムパレットを取得する
    pub fn palette(&self) -> &[(u8, u8, u8); 56] {
        &self.palette
    }
}

/// OLE2 コンテナから Workbook ストリームを抽出する
fn extract_workbook_stream(data: &[u8]) -> Result<Vec<u8>> {
    // OLE2 (Compound File Binary Format) のヘッダチェック
    if data.len() < 512 {
        return Err(XlsError::InvalidFormat("File too short for OLE2".into()));
    }

    let magic = &data[0..8];
    let ole2_magic: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

    if magic != ole2_magic {
        return Err(XlsError::InvalidFormat(
            "Not a valid OLE2/Compound Binary File".into(),
        ));
    }

    // OLE2 ヘッダの解析
    let sector_size_power = u16::from_le_bytes([data[30], data[31]]);
    let sector_size = 1usize << sector_size_power;
    let mini_sector_size_power = u16::from_le_bytes([data[32], data[33]]);
    let _mini_sector_size = 1usize << mini_sector_size_power;

    let _total_fat_sectors = u32::from_le_bytes([data[44], data[45], data[46], data[47]]);
    let first_dir_sector = u32::from_le_bytes([data[48], data[49], data[50], data[51]]);
    // offset 52: transaction signature (4 bytes) - skip
    // offset 56: mini stream cutoff size (4 bytes) - skip
    let first_mini_fat_sector = u32::from_le_bytes([data[60], data[61], data[62], data[63]]);
    let _num_mini_fat_sectors = u32::from_le_bytes([data[64], data[65], data[66], data[67]]);
    let first_difat_sector = u32::from_le_bytes([data[68], data[69], data[70], data[71]]);
    let num_difat_sectors = u32::from_le_bytes([data[72], data[73], data[74], data[75]]);


    // FAT セクター配列を構築（DIFAT から）
    let mut fat_sectors: Vec<u32> = Vec::new();

    // ヘッダ内のDIFAT配列（最大109エントリ）
    for i in 0..109 {
        let offset = 76 + i * 4;
        if offset + 4 > 512 {
            break;
        }
        let sec = u32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]]);
        if sec == 0xFFFFFFFE || sec == 0xFFFFFFFF {
            break;
        }
        fat_sectors.push(sec);
    }

    // 追加 DIFAT セクターがある場合
    if num_difat_sectors > 0 && first_difat_sector != 0xFFFFFFFE {
        let mut difat_sec = first_difat_sector;
        for _ in 0..num_difat_sectors {
            if difat_sec == 0xFFFFFFFE || difat_sec == 0xFFFFFFFF {
                break;
            }
            let sec_offset = 512 + difat_sec as usize * sector_size;
            let entries_per_sector = sector_size / 4 - 1; // last entry is next DIFAT sector
            for j in 0..entries_per_sector {
                let off = sec_offset + j * 4;
                if off + 4 > data.len() {
                    break;
                }
                let sec = u32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]]);
                if sec == 0xFFFFFFFE || sec == 0xFFFFFFFF {
                    break;
                }
                fat_sectors.push(sec);
            }
            // Next DIFAT sector
            let next_off = sec_offset + entries_per_sector * 4;
            if next_off + 4 <= data.len() {
                difat_sec = u32::from_le_bytes([data[next_off], data[next_off + 1], data[next_off + 2], data[next_off + 3]]);
            } else {
                break;
            }
        }
    }

    // FAT テーブルを構築

    let total_fat_entries = fat_sectors.len() * (sector_size / 4);
    let mut fat = vec![0xFFFFFFFFu32; total_fat_entries];
    for (i, &fat_sec) in fat_sectors.iter().enumerate() {
        let sec_offset = 512 + fat_sec as usize * sector_size;
        let entries = sector_size / 4;
        for j in 0..entries {
            let off = sec_offset + j * 4;
            if off + 4 <= data.len() {
                let idx = i * entries + j;
                if idx < fat.len() {
                    fat[idx] = u32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]]);
                }
            }
        }
    }

    // セクターチェーンを辿ってデータを読み出すヘルパー
    let read_chain = |start_sector: u32| -> Vec<u8> {
        let mut result = Vec::new();
        let mut sec = start_sector;
        let mut safety = 0;
        while sec != 0xFFFFFFFE && sec != 0xFFFFFFFF && (sec as usize) < fat.len() {
            let offset = 512 + sec as usize * sector_size;
            if offset + sector_size <= data.len() {
                result.extend_from_slice(&data[offset..offset + sector_size]);
            }
            sec = fat[sec as usize];
            safety += 1;
            if safety > 1_000_000 {
                break;
            }
        }
        result
    };

    // ディレクトリエントリを読む
    let dir_data = read_chain(first_dir_sector);

    // Mini FAT を読む
    let mini_fat_data = if first_mini_fat_sector != 0xFFFFFFFE {
        read_chain(first_mini_fat_sector)
    } else {
        Vec::new()
    };
    let mini_fat: Vec<u32> = mini_fat_data
        .chunks(4)
        .map(|c| {
            if c.len() == 4 {
                u32::from_le_bytes([c[0], c[1], c[2], c[3]])
            } else {
                0xFFFFFFFE
            }
        })
        .collect();

    // ルートエントリのデータ（Mini Stream コンテナ）
    let root_start_sector = if dir_data.len() >= 128 {
        u32::from_le_bytes([dir_data[116], dir_data[117], dir_data[118], dir_data[119]])
    } else {
        0xFFFFFFFE
    };
    let mini_stream_data = if root_start_sector != 0xFFFFFFFE {
        read_chain(root_start_sector)
    } else {
        Vec::new()
    };
    let mini_stream_cutoff = 0x1000usize; // 4096 bytes
    let mini_sector_size = 1usize << mini_sector_size_power;

    // ディレクトリエントリをスキャンして "Workbook" または "Book" を探す
    let dir_entry_size = 128;
    let num_entries = dir_data.len() / dir_entry_size;

    for i in 0..num_entries {
        let entry_offset = i * dir_entry_size;
        let entry = &dir_data[entry_offset..entry_offset + dir_entry_size];

        // エントリ名を読む (UTF-16LE, max 64 bytes = 32 chars)
        let name_len = u16::from_le_bytes([entry[64], entry[65]]) as usize;
        if name_len < 2 || name_len > 64 {
            continue;
        }

        let name_bytes = &entry[0..name_len];
        let name: String = name_bytes
            .chunks(2)
            .filter_map(|c| {
                if c.len() == 2 {
                    let ch = u16::from_le_bytes([c[0], c[1]]);
                    if ch == 0 {
                        None
                    } else {
                        char::from_u32(ch as u32)
                    }
                } else {
                    None
                }
            })
            .collect();

        if name != "Workbook" && name != "Book" {
            continue;
        }

        // エントリタイプ
        let entry_type = entry[66];
        if entry_type != 2 {
            // 2 = Stream
            continue;
        }

        let start_sector = u32::from_le_bytes([entry[116], entry[117], entry[118], entry[119]]);
        let stream_size = u32::from_le_bytes([entry[120], entry[121], entry[122], entry[123]]) as usize;

        let stream_data = if stream_size < mini_stream_cutoff && !mini_stream_data.is_empty() {
            // Mini Stream から読む
            let mut result = Vec::new();
            let mut sec = start_sector;
            let mut remaining = stream_size;
            let mut safety = 0;
            while sec != 0xFFFFFFFE && sec != 0xFFFFFFFF && remaining > 0 {
                let offset = sec as usize * mini_sector_size;
                let read_size = remaining.min(mini_sector_size);
                if offset + read_size <= mini_stream_data.len() {
                    result.extend_from_slice(&mini_stream_data[offset..offset + read_size]);
                }
                remaining = remaining.saturating_sub(mini_sector_size);
                if (sec as usize) < mini_fat.len() {
                    sec = mini_fat[sec as usize];
                } else {
                    break;
                }
                safety += 1;
                if safety > 1_000_000 {
                    break;
                }
            }
            result.truncate(stream_size);
            result
        } else {
            // 通常のセクターチェーンから読む
            let mut result = read_chain(start_sector);
            result.truncate(stream_size);
            result
        };

        return Ok(stream_data);
    }

    Err(XlsError::InvalidFormat(
        "Workbook stream not found in OLE2 container".into(),
    ))
}

/// PALETTE レコードをパースする
fn parse_palette(data: &[u8], palette: &mut [(u8, u8, u8); 56]) {
    if data.len() < 2 {
        return;
    }
    let count = u16::from_le_bytes([data[0], data[1]]) as usize;
    for i in 0..count.min(56) {
        let offset = 2 + i * 4;
        if offset + 4 <= data.len() {
            palette[i] = (data[offset], data[offset + 1], data[offset + 2]);
            // data[offset+3] は unused (0x00)
        }
    }
}

/// SST (Shared String Table) レコードをパースする
fn parse_sst(data: &[u8]) -> Vec<String> {
    if data.len() < 8 {
        return Vec::new();
    }

    let _total_refs = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let unique_count = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;

    let mut strings = Vec::with_capacity(unique_count);
    let mut offset = 8;

    for _ in 0..unique_count {
        if offset >= data.len() {
            break;
        }
        match biff8::read_sst_string(data, offset) {
            Some((s, consumed)) => {
                strings.push(s);
                offset += consumed;
            }
            None => {
                // パースできない場合は空文字列を追加して次へ
                strings.push(String::new());
                break;
            }
        }
    }

    strings
}

/// シートのレコードをパースする
fn parse_sheet(
    data: &[u8],
    offset: usize,
    name: &str,
    sst_strings: &[String],
    xf_records: &[XfRecord],
    palette: &[(u8, u8, u8); 56],
) -> Result<Sheet> {
    let mut cells = Vec::new();
    let mut pos = offset;

    // BOF レコードを確認
    if pos + 4 > data.len() {
        return Err(XlsError::InvalidFormat("Sheet BOF missing".into()));
    }

    let rec_type = u16::from_le_bytes([data[pos], data[pos + 1]]);
    let rec_len = u16::from_le_bytes([data[pos + 2], data[pos + 3]]) as usize;

    if rec_type != record_type::BOF {
        return Err(XlsError::InvalidFormat("Expected Sheet BOF".into()));
    }

    pos += 4 + rec_len;

    // シートのレコードを読む
    while pos + 4 <= data.len() {
        let rec_type = u16::from_le_bytes([data[pos], data[pos + 1]]);
        let rec_len = u16::from_le_bytes([data[pos + 2], data[pos + 3]]) as usize;

        if pos + 4 + rec_len > data.len() {
            break;
        }

        let rec_data = &data[pos + 4..pos + 4 + rec_len];

        match rec_type {
            record_type::LABEL_SST => {
                if rec_data.len() >= 10 {
                    let row = u16::from_le_bytes([rec_data[0], rec_data[1]]);
                    let col = u16::from_le_bytes([rec_data[2], rec_data[3]]);
                    let xf_index = u16::from_le_bytes([rec_data[4], rec_data[5]]);
                    let sst_index = u32::from_le_bytes([
                        rec_data[6],
                        rec_data[7],
                        rec_data[8],
                        rec_data[9],
                    ]) as usize;

                    let value = if sst_index < sst_strings.len() {
                        CellValue::String(sst_strings[sst_index].clone())
                    } else {
                        CellValue::String(String::new())
                    };

                    let bg = resolve_bg_color(xf_index, xf_records, palette);
                    cells.push(Cell {
                        row,
                        col,
                        value,
                        background_color: bg,
                    });
                }
            }
            record_type::NUMBER => {
                if rec_data.len() >= 14 {
                    let row = u16::from_le_bytes([rec_data[0], rec_data[1]]);
                    let col = u16::from_le_bytes([rec_data[2], rec_data[3]]);
                    let xf_index = u16::from_le_bytes([rec_data[4], rec_data[5]]);
                    let value = f64::from_le_bytes([
                        rec_data[6],
                        rec_data[7],
                        rec_data[8],
                        rec_data[9],
                        rec_data[10],
                        rec_data[11],
                        rec_data[12],
                        rec_data[13],
                    ]);

                    let bg = resolve_bg_color(xf_index, xf_records, palette);
                    cells.push(Cell {
                        row,
                        col,
                        value: CellValue::Number(value),
                        background_color: bg,
                    });
                }
            }
            record_type::RK => {
                if rec_data.len() >= 10 {
                    let row = u16::from_le_bytes([rec_data[0], rec_data[1]]);
                    let col = u16::from_le_bytes([rec_data[2], rec_data[3]]);
                    let xf_index = u16::from_le_bytes([rec_data[4], rec_data[5]]);
                    let rk_val = u32::from_le_bytes([
                        rec_data[6],
                        rec_data[7],
                        rec_data[8],
                        rec_data[9],
                    ]);
                    let value = biff8::decode_rk(rk_val);

                    let bg = resolve_bg_color(xf_index, xf_records, palette);
                    cells.push(Cell {
                        row,
                        col,
                        value: CellValue::Number(value),
                        background_color: bg,
                    });
                }
            }
            record_type::MULRK => {
                if rec_data.len() >= 6 {
                    let row = u16::from_le_bytes([rec_data[0], rec_data[1]]);
                    let first_col = u16::from_le_bytes([rec_data[2], rec_data[3]]);
                    let last_col =
                        u16::from_le_bytes([rec_data[rec_data.len() - 2], rec_data[rec_data.len() - 1]]);

                    let mut offset = 4;
                    for col in first_col..=last_col {
                        if offset + 6 > rec_data.len() - 2 {
                            break;
                        }
                        let xf_index =
                            u16::from_le_bytes([rec_data[offset], rec_data[offset + 1]]);
                        let rk_val = u32::from_le_bytes([
                            rec_data[offset + 2],
                            rec_data[offset + 3],
                            rec_data[offset + 4],
                            rec_data[offset + 5],
                        ]);
                        let value = biff8::decode_rk(rk_val);

                        let bg = resolve_bg_color(xf_index, xf_records, palette);
                        cells.push(Cell {
                            row,
                            col,
                            value: CellValue::Number(value),
                            background_color: bg,
                        });
                        offset += 6;
                    }
                }
            }
            record_type::MULBLANK => {
                if rec_data.len() >= 6 {
                    let row = u16::from_le_bytes([rec_data[0], rec_data[1]]);
                    let first_col = u16::from_le_bytes([rec_data[2], rec_data[3]]);
                    let last_col =
                        u16::from_le_bytes([rec_data[rec_data.len() - 2], rec_data[rec_data.len() - 1]]);

                    let mut offset = 4;
                    for col in first_col..=last_col {
                        if offset + 2 > rec_data.len() - 2 {
                            break;
                        }
                        let xf_index =
                            u16::from_le_bytes([rec_data[offset], rec_data[offset + 1]]);

                        let bg = resolve_bg_color(xf_index, xf_records, palette);
                        if bg.is_some() {
                            cells.push(Cell {
                                row,
                                col,
                                value: CellValue::Empty,
                                background_color: bg,
                            });
                        }
                        offset += 2;
                    }
                }
            }
            record_type::BLANK => {
                if rec_data.len() >= 6 {
                    let row = u16::from_le_bytes([rec_data[0], rec_data[1]]);
                    let col = u16::from_le_bytes([rec_data[2], rec_data[3]]);
                    let xf_index = u16::from_le_bytes([rec_data[4], rec_data[5]]);

                    let bg = resolve_bg_color(xf_index, xf_records, palette);
                    if bg.is_some() {
                        cells.push(Cell {
                            row,
                            col,
                            value: CellValue::Empty,
                            background_color: bg,
                        });
                    }
                }
            }
            record_type::BOOLERR => {
                if rec_data.len() >= 8 {
                    let row = u16::from_le_bytes([rec_data[0], rec_data[1]]);
                    let col = u16::from_le_bytes([rec_data[2], rec_data[3]]);
                    let xf_index = u16::from_le_bytes([rec_data[4], rec_data[5]]);
                    let bval = rec_data[6];
                    let is_error = rec_data[7];

                    let value = if is_error != 0 {
                        CellValue::Error(bval)
                    } else {
                        CellValue::Bool(bval != 0)
                    };

                    let bg = resolve_bg_color(xf_index, xf_records, palette);
                    cells.push(Cell {
                        row,
                        col,
                        value,
                        background_color: bg,
                    });
                }
            }
            record_type::LABEL => {
                // Legacy LABEL record (BIFF2-BIFF7 compat)
                if rec_data.len() >= 8 {
                    let row = u16::from_le_bytes([rec_data[0], rec_data[1]]);
                    let col = u16::from_le_bytes([rec_data[2], rec_data[3]]);
                    let xf_index = u16::from_le_bytes([rec_data[4], rec_data[5]]);

                    let text = if let Some((s, _)) = biff8::read_unicode_string(rec_data, 6) {
                        s
                    } else {
                        String::new()
                    };

                    let bg = resolve_bg_color(xf_index, xf_records, palette);
                    cells.push(Cell {
                        row,
                        col,
                        value: CellValue::String(text),
                        background_color: bg,
                    });
                }
            }
            record_type::EOF => {
                break;
            }
            _ => {}
        }

        pos += 4 + rec_len;
    }

    Ok(Sheet {
        name: name.to_string(),
        cells,
    })
}

/// XF インデックスから背景色を解決する
fn resolve_bg_color(
    xf_index: u16,
    xf_records: &[XfRecord],
    palette: &[(u8, u8, u8); 56],
) -> Option<XlsColor> {
    let xf = xf_records.get(xf_index as usize)?;

    if xf.fill_pattern == 0 {
        // パターンなし = 背景色なし
        return None;
    }

    if xf.fill_pattern == 1 {
        // Solid fill: foreground color が実際の背景色になる
        color::resolve_color_index(xf.fg_color_index, palette)
    } else {
        // その他のパターン: 背景色を使う
        color::resolve_color_index(xf.bg_color_index, palette)
    }
}
