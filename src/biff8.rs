/// BIFF8 レコードタイプ定数
#[allow(dead_code)]
pub mod record_type {
    pub const BOF: u16 = 0x0809;
    pub const EOF: u16 = 0x000A;
    pub const BOUNDSHEET: u16 = 0x0085;
    pub const SST: u16 = 0x00FC;
    pub const CONTINUE: u16 = 0x003C;
    pub const XF: u16 = 0x00E0;
    pub const PALETTE: u16 = 0x0092;
    pub const FORMAT: u16 = 0x041E;
    pub const FONT: u16 = 0x0031;
    pub const LABEL_SST: u16 = 0x00FD;
    pub const NUMBER: u16 = 0x0203;
    pub const RK: u16 = 0x027E;
    pub const MULRK: u16 = 0x00BD;
    pub const MULBLANK: u16 = 0x00BE;
    pub const BLANK: u16 = 0x0201;
    pub const BOOLERR: u16 = 0x0205;
    pub const LABEL: u16 = 0x0204;
    pub const RSTRING: u16 = 0x00D6;
    pub const DIMENSION: u16 = 0x0200;
    pub const ROW: u16 = 0x0208;
    pub const INDEX: u16 = 0x020B;
    pub const WINDOW2: u16 = 0x023E;
    pub const CODEPAGE: u16 = 0x0042;
    pub const DATEMODE: u16 = 0x0022;
    pub const STYLE: u16 = 0x0293;
}

/// BOF レコードのタイプフィールド
#[allow(dead_code)]
pub mod bof_type {
    pub const WORKBOOK_GLOBALS: u16 = 0x0005;
    pub const WORKSHEET: u16 = 0x0010;
}

/// BIFF8 の BOF バージョン
pub const BIFF8_VERSION: u16 = 0x0600;

/// レコードヘッダのサイズ（4バイト: type u16 + length u16）
#[allow(dead_code)]
pub const RECORD_HEADER_SIZE: usize = 4;

/// XF レコードから背景色情報を抽出する
///
/// BIFF8 XF レコード構造 (20 bytes):
///   offset 0:  font index (u16)
///   offset 2:  format index (u16)
///   offset 4:  type/protection/parent (u16)
///   offset 6:  alignment (u16)
///   offset 8:  rotation (u16)
///   offset 10: text properties (u16)
///   offset 12: used attributes (u16)
///   offset 14: border colors/line styles (u32)
///   offset 18: pattern/color (u32) ← ここに背景色がある
///
/// offset 18 の u32:
///   bits 0-6:   pattern color index (foreground)
///   bits 7-13:  pattern background color index
///   bits 16-25: fill pattern type (0=none, 1=solid, etc.)
///
/// 実際の BIFF8 仕様:
///   offset 18 (2 bytes): border color (bottom + diag)
///   offset 20 (4 bytes): pattern info
///     bits 26-31 of this u32 = fill pattern
///     Then another set of bits for pattern fg/bg colors
///
/// 正確な構造:
///   XF record total = 20 bytes
///   offset 18: u16 (bottom border color + diag border color)
///   → ただし実際にはBIFF8 XFは20バイト
///   背景パターンとカラーは offset 16..20 に格納
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct XfRecord {
    pub font_index: u16,
    pub format_index: u16,
    pub fill_pattern: u8,
    pub fg_color_index: u16,
    pub bg_color_index: u16,
}

/// BIFF8 の XF レコードをパースする
pub fn parse_xf_record(data: &[u8]) -> Option<XfRecord> {
    if data.len() < 20 {
        return None;
    }

    let font_index = u16::from_le_bytes([data[0], data[1]]);
    let format_index = u16::from_le_bytes([data[2], data[3]]);

    // BIFF8 XF record layout (MS-XLS spec, 20 bytes):
    //   0-1:   ifnt (font index)
    //   2-3:   ifmt (format index)
    //   4-5:   type/protection/parent XF
    //   6:     alignment (horiz/vert/wrap)
    //   7:     rotation
    //   8:     indent/shrink/merge/reading order
    //   9:     attr flags (used attributes)
    //   10-11: border line styles (left/right/top/bottom, 4 bits each)
    //   12-13: left/right border color + diagonal flags
    //   14-17: (u32) top/bottom/diag border color + diag style + fill pattern
    //          bits 0-6:   icvTop (top border color)
    //          bits 7-13:  icvBottom (bottom border color)
    //          bits 14-20: icvDiag (diagonal color)
    //          bits 21-24: dgDiag (diagonal style)
    //          bit  25:    unused
    //          bits 26-31: fls (fill pattern: 0=none, 1=solid, ...)
    //   18-19: (u16) pattern colors
    //          bits 0-6:   icvFore (pattern foreground color)
    //          bits 7-13:  icvBack (pattern background color)

    let border_fill = u32::from_le_bytes([data[14], data[15], data[16], data[17]]);
    let fill_pattern = ((border_fill >> 26) & 0x3F) as u8;

    let color_word = u16::from_le_bytes([data[18], data[19]]);
    let fg_color_index = color_word & 0x7F;
    let bg_color_index = (color_word >> 7) & 0x7F;

    Some(XfRecord {
        font_index,
        format_index,
        fill_pattern,
        fg_color_index,
        bg_color_index,
    })
}

/// RK 値をデコードして f64 にする
pub fn decode_rk(rk: u32) -> f64 {
    let is_integer = (rk & 0x02) != 0;
    let is_div100 = (rk & 0x01) != 0;

    let value = if is_integer {
        (rk as i32 >> 2) as f64
    } else {
        let v = (rk & 0xFFFF_FFFC) as u64;
        let bytes = (v << 32).to_le_bytes();
        f64::from_le_bytes(bytes)
    };

    if is_div100 {
        value / 100.0
    } else {
        value
    }
}

/// BIFF8 の Unicode 文字列を読み取る
/// flags の bit0: 0=compressed(Latin1), 1=uncompressed(UTF-16LE)
/// flags の bit2: Asian phonetic (rich text extension)
/// flags の bit3: Extended string (Far East)
pub fn read_unicode_string(data: &[u8], offset: usize) -> Option<(String, usize)> {
    if offset + 3 > data.len() {
        return None;
    }

    let char_count = u16::from_le_bytes([data[offset], data[offset + 1]]) as usize;
    let flags = data[offset + 2];
    let is_wide = (flags & 0x01) != 0;
    let has_rich = (flags & 0x08) != 0;
    let has_ext = (flags & 0x04) != 0;

    let mut pos = offset + 3;

    let rich_count = if has_rich {
        if pos + 2 > data.len() {
            return None;
        }
        let c = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;
        pos += 2;
        c
    } else {
        0
    };

    let ext_size = if has_ext {
        if pos + 4 > data.len() {
            return None;
        }
        let s = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
            as usize;
        pos += 4;
        s
    } else {
        0
    };

    let text = if is_wide {
        let byte_len = char_count * 2;
        if pos + byte_len > data.len() {
            return None;
        }
        let mut s = String::with_capacity(char_count);
        for i in 0..char_count {
            let lo = data[pos + i * 2];
            let hi = data[pos + i * 2 + 1];
            let ch = u16::from_le_bytes([lo, hi]);
            if let Some(c) = char::from_u32(ch as u32) {
                s.push(c);
            }
        }
        pos += byte_len;
        s
    } else {
        let byte_len = char_count;
        if pos + byte_len > data.len() {
            return None;
        }
        let bytes = &data[pos..pos + byte_len];
        pos += byte_len;
        // Compressed strings are Latin-1 encoded
        bytes.iter().map(|&b| b as char).collect()
    };

    // Skip rich text formatting runs
    pos += rich_count * 4;
    // Skip extended string data
    pos += ext_size;

    Some((text, pos - offset))
}

/// SST (Shared String Table) 用の文字列読み取り
/// SST では string の先頭が char_count(u16) + flags(u8) の構造
pub fn read_sst_string(data: &[u8], offset: usize) -> Option<(String, usize)> {
    read_unicode_string(data, offset)
}

/// 短い Unicode 文字列を読む（char_count が u8 のパターン、BoundSheet 用）
pub fn read_short_unicode_string(data: &[u8], offset: usize) -> Option<(String, usize)> {
    if offset + 2 > data.len() {
        return None;
    }

    let char_count = data[offset] as usize;
    let flags = data[offset + 1];
    let is_wide = (flags & 0x01) != 0;

    let pos = offset + 2;

    let text = if is_wide {
        let byte_len = char_count * 2;
        if pos + byte_len > data.len() {
            return None;
        }
        let mut s = String::with_capacity(char_count);
        for i in 0..char_count {
            let lo = data[pos + i * 2];
            let hi = data[pos + i * 2 + 1];
            let ch = u16::from_le_bytes([lo, hi]);
            if let Some(c) = char::from_u32(ch as u32) {
                s.push(c);
            }
        }
        (s, 2 + byte_len)
    } else {
        let byte_len = char_count;
        if pos + byte_len > data.len() {
            return None;
        }
        let bytes = &data[pos..pos + byte_len];
        let text: String = bytes.iter().map(|&b| b as char).collect();
        (text, 2 + byte_len)
    };

    Some(text)
}
