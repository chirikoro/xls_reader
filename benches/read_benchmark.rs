//! ベンチマーク: xls_reader vs calamine
//!
//! テスト用 .xls ファイルを自前で生成し、読み込み速度を比較する。
//! 実行: cargo bench

use std::hint::black_box;
use std::time::Instant;

use xls_reader::{CellValue, XlsColor, XlsWriter, XlsReader};

/// テスト用の .xls ファイルを生成する（N行 x M列、値+色付き）
fn generate_test_xls(rows: u16, cols: u16) -> Vec<u8> {
    let mut writer = XlsWriter::new();
    let colors = [
        XlsColor::RED,
        XlsColor::GREEN,
        XlsColor::BLUE,
        XlsColor::YELLOW,
        XlsColor::CYAN,
    ];
    {
        let mut sheet = writer.add_sheet("BenchSheet");
        for r in 0..rows {
            for c in 0..cols {
                let idx = (r as usize * cols as usize + c as usize) % 3;
                match idx {
                    0 => {
                        sheet.write_cell_with_color(
                            r, c,
                            CellValue::Number(r as f64 * 100.0 + c as f64),
                            colors[(r as usize + c as usize) % colors.len()].clone(),
                        );
                    }
                    1 => {
                        sheet.write_cell_with_color(
                            r, c,
                            CellValue::String(format!("R{}C{}", r, c)),
                            colors[(r as usize + c as usize) % colors.len()].clone(),
                        );
                    }
                    _ => {
                        sheet.write_number(r, c, r as f64 + c as f64 * 0.1);
                    }
                }
            }
        }
    }
    writer.to_bytes().expect("Failed to generate test xls")
}

fn bench_read_xls_reader(data: &[u8], iterations: u32) -> std::time::Duration {
    let start = Instant::now();
    for _ in 0..iterations {
        let reader = XlsReader::from_bytes(black_box(data)).unwrap();
        let sheet = reader.sheet_by_index(0).unwrap();
        // セルアクセスも計測
        let _ = black_box(sheet.value_ref(0, 0));
        let _ = black_box(sheet.background_color(0, 0));
        let _ = black_box(sheet.value_ref(49, 9));
        let _ = black_box(sheet.background_color(49, 9));
    }
    start.elapsed()
}

fn bench_read_calamine(data: &[u8], iterations: u32) -> std::time::Duration {
    use calamine::{Reader, Xls};
    use std::io::Cursor;

    let start = Instant::now();
    for _ in 0..iterations {
        let cursor = Cursor::new(black_box(data));
        let mut workbook: Xls<_> = Xls::new(cursor).unwrap();
        let sheet_name = workbook.sheet_names()[0].clone();
        let range = workbook.worksheet_range(&sheet_name).unwrap();
        // セルアクセスも計測
        let _ = black_box(range.get_value((0, 0)));
        let _ = black_box(range.get_value((49, 9)));
    }
    start.elapsed()
}

fn bench_cell_access_xls_reader(data: &[u8], iterations: u32) -> std::time::Duration {
    let reader = XlsReader::from_bytes(data).unwrap();
    let sheet = reader.sheet_by_index(0).unwrap();

    let start = Instant::now();
    for _ in 0..iterations {
        for r in 0..50u16 {
            for c in 0..10u16 {
                let _ = black_box(sheet.value_ref(r, c));
                let _ = black_box(sheet.background_color(r, c));
            }
        }
    }
    start.elapsed()
}

fn bench_cell_access_calamine(data: &[u8], iterations: u32) -> std::time::Duration {
    use calamine::{Reader, Xls};
    use std::io::Cursor;

    let cursor = Cursor::new(data);
    let mut workbook: Xls<_> = Xls::new(cursor).unwrap();
    let sheet_name = workbook.sheet_names()[0].clone();
    let range = workbook.worksheet_range(&sheet_name).unwrap();

    let start = Instant::now();
    for _ in 0..iterations {
        for r in 0..50u32 {
            for c in 0..10u32 {
                let _ = black_box(range.get_value((r, c)));
                // calamine はセル背景色の取得に対応していない
            }
        }
    }
    start.elapsed()
}

fn main() {
    println!("=== xls_reader ベンチマーク ===");
    println!();

    // テストデータ生成
    let rows = 100;
    let cols = 20;
    let total_cells = rows as u32 * cols as u32;

    print!("テストデータ生成中 ({}行 x {}列 = {}セル)... ", rows, cols, total_cells);
    let gen_start = Instant::now();
    let xls_data = generate_test_xls(rows, cols);
    println!("完了 ({:.1}ms, {}バイト)", gen_start.elapsed().as_secs_f64() * 1000.0, xls_data.len());
    println!();

    // --- ファイル読み込み (parse) ベンチマーク ---
    let iterations = 100;

    println!("--- ファイル読み込み (parse) x{} 回 ---", iterations);

    let dur_ours = bench_read_xls_reader(&xls_data, iterations);
    let avg_ours = dur_ours.as_secs_f64() * 1000.0 / iterations as f64;
    println!("  xls_reader:  {:.3}ms/回  (合計 {:.1}ms)", avg_ours, dur_ours.as_secs_f64() * 1000.0);

    let dur_calamine = bench_read_calamine(&xls_data, iterations);
    let avg_calamine = dur_calamine.as_secs_f64() * 1000.0 / iterations as f64;
    println!("  calamine:    {:.3}ms/回  (合計 {:.1}ms)", avg_calamine, dur_calamine.as_secs_f64() * 1000.0);

    let ratio = avg_calamine / avg_ours;
    if ratio > 1.0 {
        println!("  → xls_reader は calamine より {:.1}倍高速 (読み込み)", ratio);
    } else {
        println!("  → calamine は xls_reader より {:.1}倍高速 (読み込み)", 1.0 / ratio);
    }
    println!();

    // --- セルアクセスベンチマーク ---
    let cell_iters = 1000;
    println!("--- セルアクセス (全{}セル) x{} 回 ---", total_cells, cell_iters);
    println!("  ※ xls_reader は値+背景色、calamine は値のみ");

    let dur_ours = bench_cell_access_xls_reader(&xls_data, cell_iters);
    let avg_ours = dur_ours.as_secs_f64() * 1000.0 / cell_iters as f64;
    println!("  xls_reader:  {:.3}ms/回  (値+色)", avg_ours);

    let dur_calamine = bench_cell_access_calamine(&xls_data, cell_iters);
    let avg_calamine = dur_calamine.as_secs_f64() * 1000.0 / cell_iters as f64;
    println!("  calamine:    {:.3}ms/回  (値のみ)", avg_calamine);

    let ratio = avg_calamine / avg_ours;
    if ratio > 1.0 {
        println!("  → xls_reader は calamine より {:.1}倍高速 (セルアクセス)", ratio);
    } else {
        println!("  → calamine は xls_reader より {:.1}倍高速 (セルアクセス)", 1.0 / ratio);
    }

    println!();
    println!("注意: calamine はセル背景色の読み取りに非対応です。");
    println!("      xls_reader は色情報も含めて処理しています。");
}
