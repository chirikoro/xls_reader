# xls_reader

Rust 製の `.xls` (BIFF8) ファイル読み書きライブラリ。セルの値に加え、セル背景色の読み書きに対応しています。

**外部 Excel ライブラリ不要** — OLE2 コンテナ解析と BIFF8 レコード処理をすべて Rust で自前実装しています。

## 機能

- `.xls` ファイルからセル値（文字列・数値・真偽値・エラー）を読み取り
- `.xls` ファイルへのセル値の書き込み
- セル背景色（塗りつぶし色）の読み取り・書き込み
- 複数シート対応
- Unicode (UTF-16) 完全対応（日本語等の CJK 文字を含む）
- カスタムパレットカラー対応
- 条件付き書式は意図的に **非対応**

## インストール

`Cargo.toml` に追加してください：

```toml
[dependencies]
xls_reader = { path = "." }
```

## 使い方

### `.xls` ファイルの読み込み

```rust
use xls_reader::{XlsReader, CellValue};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let reader = XlsReader::open("example.xls")?;

    // シート名の一覧を取得
    println!("シート: {:?}", reader.sheet_names());

    // インデックスでシートを取得
    let sheet = reader.sheet_by_index(0).unwrap();

    // セル値を読み取り
    let value = sheet.value(0, 0); // 行0, 列0
    println!("A1 = {}", value);

    // セル背景色を読み取り
    if let Some(color) = sheet.background_color(0, 0) {
        println!("A1 の背景色: {}", color.to_hex()); // 例: "#FF0000"
    }

    // シート内の全セルをイテレート
    for cell in &sheet.cells {
        println!(
            "({}, {}): 値={}, 背景色={:?}",
            cell.row, cell.col, cell.value, cell.background_color
        );
    }

    Ok(())
}
```

### `.xls` ファイルの書き込み

```rust
use xls_reader::{XlsWriter, XlsColor, CellValue};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut writer = XlsWriter::new();

    // シートを追加してセルに書き込む
    {
        let mut sheet = writer.add_sheet("シート1");

        // 値のみ
        sheet.write_string(0, 0, "こんにちは");
        sheet.write_number(0, 1, 42.0);

        // 値 + 背景色
        sheet.write_cell_with_color(
            1, 0,
            CellValue::String("重要".into()),
            XlsColor::RED,
        );

        // 背景色のみ（空セル）
        sheet.write_color(1, 1, XlsColor::YELLOW);
    }

    // ファイルに保存
    writer.save("output.xls")?;

    Ok(())
}
```

### カスタムパレットカラー

```rust
use xls_reader::{XlsWriter, XlsColor};

let mut writer = XlsWriter::new();

// パレットインデックス 0 にカスタムカラーを設定
writer.set_palette_color(0, 0xAB, 0xCD, 0xEF);

let mut sheet = writer.add_sheet("カスタム色");
sheet.write_cell_with_color(
    0, 0,
    xls_reader::CellValue::String("カスタムカラー".into()),
    XlsColor::rgb(0xAB, 0xCD, 0xEF),
);
```

## API リファレンス

### 型一覧

| 型 | 説明 |
|----|------|
| `XlsReader` | `.xls` ファイルを読み込む（値と背景色） |
| `XlsWriter` | `.xls` ファイルを書き出す（値と背景色） |
| `Sheet` | セルを含むワークシート |
| `Cell` | 値・座標・背景色を持つセル |
| `CellValue` | セル値の列挙型: `Empty`, `Number(f64)`, `String(String)`, `Bool(bool)`, `Error(u8)` |
| `XlsColor` | `red`, `green`, `blue` フィールドを持つ RGB カラー |
| `XlsError` | すべての操作のエラー型 |

### `XlsReader`

| メソッド | 説明 |
|----------|------|
| `open(path)` | ファイルパスから `.xls` ファイルを開く |
| `from_bytes(data)` | バイトスライスからパースする |
| `sheet_names()` | 全シート名を取得する |
| `sheet_count()` | シート数を取得する |
| `sheet_by_index(i)` | インデックスでシートを取得する |
| `sheet_by_name(name)` | 名前でシートを取得する |
| `sheets()` | 全シートを取得する |

### `Sheet`

| メソッド | 説明 |
|----------|------|
| `cell(row, col)` | 指定座標のセルを取得する |
| `value(row, col)` | セル値を取得する（未設定の場合 `Empty`） |
| `background_color(row, col)` | 背景色を取得する（未設定の場合 `None`） |
| `max_row()` | 使用されている最大行インデックス |
| `max_col()` | 使用されている最大列インデックス |

### `XlsWriter`

| メソッド | 説明 |
|----------|------|
| `new()` | 新しいライターを作成する |
| `add_sheet(name)` | シートを追加する（`SheetWriter` を返す） |
| `set_palette_color(index, r, g, b)` | パレットカラーを上書きする |
| `save(path)` | ファイルに書き出す |
| `to_bytes()` | バイト列として書き出す |

### `SheetWriter`

| メソッド | 説明 |
|----------|------|
| `write_cell(row, col, value)` | セルに値を書き込む |
| `write_cell_with_color(row, col, value, color)` | セルに値と背景色を書き込む |
| `write_number(row, col, value)` | 数値を書き込む |
| `write_string(row, col, value)` | 文字列を書き込む |
| `write_color(row, col, color)` | 空セルに背景色を設定する |

### `XlsColor`

| 項目 | 説明 |
|------|------|
| `XlsColor::rgb(r, g, b)` | RGB 値からカラーを作成する |
| `to_hex()` | `#RRGGBB` 形式の文字列に変換する |
| `XlsColor::BLACK` | 定義済み定数 |
| `XlsColor::WHITE` | 定義済み定数 |
| `XlsColor::RED` | 定義済み定数 |
| `XlsColor::GREEN` | 定義済み定数 |
| `XlsColor::BLUE` | 定義済み定数 |
| `XlsColor::YELLOW` | 定義済み定数 |
| `XlsColor::MAGENTA` | 定義済み定数 |
| `XlsColor::CYAN` | 定義済み定数 |

## 制限事項

- `.xls` (BIFF8) 形式のみ対応 — `.xlsx` には非対応
- 条件付き書式は読み書きしない
- 数式は評価しない（キャッシュされた計算結果値として読み取る）
- グラフ・画像・その他の埋め込みオブジェクトは非対応
- セル罫線やフォントスタイルは非対応（背景色のみ）
- カスタムパレットカラーは最大 56 色（BIFF8 パレットの制限）

## ライセンス

MIT
