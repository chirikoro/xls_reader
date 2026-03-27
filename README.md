# xls_reader

A pure Rust library for reading and writing legacy Excel `.xls` (BIFF8) files with cell background color support.

**No external Excel dependencies** — OLE2 compound file parsing and BIFF8 record handling are fully implemented from scratch.

## Features

- Read cell values (string, number, boolean, error) from `.xls` files
- Write cell values to `.xls` files
- Read and write cell background colors (fill colors)
- Multi-sheet support
- Full Unicode (UTF-16) support including CJK characters
- Custom palette color support
- Conditional formatting is intentionally **not** supported

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
xls_reader = { path = "." }
```

## Usage

### Reading an `.xls` file

```rust
use xls_reader::{XlsReader, CellValue};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let reader = XlsReader::open("example.xls")?;

    // List all sheet names
    println!("Sheets: {:?}", reader.sheet_names());

    // Access a sheet by index
    let sheet = reader.sheet_by_index(0).unwrap();

    // Read cell values
    let value = sheet.value(0, 0); // row 0, col 0
    println!("A1 = {}", value);

    // Read cell background color
    if let Some(color) = sheet.background_color(0, 0) {
        println!("A1 background: {}", color.to_hex()); // e.g. "#FF0000"
    }

    // Iterate over all cells in a sheet
    for cell in &sheet.cells {
        println!(
            "({}, {}): value={}, bg={:?}",
            cell.row, cell.col, cell.value, cell.background_color
        );
    }

    Ok(())
}
```

### Writing an `.xls` file

```rust
use xls_reader::{XlsWriter, XlsColor, CellValue};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut writer = XlsWriter::new();

    // Add a sheet and write cells
    {
        let mut sheet = writer.add_sheet("Sheet1");

        // Simple values
        sheet.write_string(0, 0, "Hello");
        sheet.write_number(0, 1, 42.0);

        // Value with background color
        sheet.write_cell_with_color(
            1, 0,
            CellValue::String("Important".into()),
            XlsColor::RED,
        );

        // Background color only (empty cell)
        sheet.write_color(1, 1, XlsColor::YELLOW);
    }

    // Save to file
    writer.save("output.xls")?;

    Ok(())
}
```

### Custom palette colors

```rust
use xls_reader::{XlsWriter, XlsColor};

let mut writer = XlsWriter::new();

// Override palette index 0 with a custom color
writer.set_palette_color(0, 0xAB, 0xCD, 0xEF);

let mut sheet = writer.add_sheet("Custom");
sheet.write_cell_with_color(
    0, 0,
    xls_reader::CellValue::String("Custom color".into()),
    XlsColor::rgb(0xAB, 0xCD, 0xEF),
);
```

## API Reference

### Types

| Type | Description |
|------|-------------|
| `XlsReader` | Reads `.xls` files — values and background colors |
| `XlsWriter` | Writes `.xls` files — values and background colors |
| `Sheet` | A worksheet containing cells |
| `Cell` | A cell with value, position, and optional background color |
| `CellValue` | Cell value enum: `Empty`, `Number(f64)`, `String(String)`, `Bool(bool)`, `Error(u8)` |
| `XlsColor` | RGB color with `red`, `green`, `blue` fields |
| `XlsError` | Error type for all operations |

### `XlsReader`

| Method | Description |
|--------|-------------|
| `open(path)` | Open a `.xls` file from disk |
| `from_bytes(data)` | Parse from byte slice |
| `sheet_names()` | Get all sheet names |
| `sheet_count()` | Get number of sheets |
| `sheet_by_index(i)` | Get sheet by index |
| `sheet_by_name(name)` | Get sheet by name |
| `sheets()` | Get all sheets |

### `Sheet`

| Method | Description |
|--------|-------------|
| `cell(row, col)` | Get cell at position |
| `value(row, col)` | Get cell value (returns `Empty` if not found) |
| `background_color(row, col)` | Get background color (returns `None` if not set) |
| `max_row()` | Maximum row index used |
| `max_col()` | Maximum column index used |

### `XlsWriter`

| Method | Description |
|--------|-------------|
| `new()` | Create a new writer |
| `add_sheet(name)` | Add a sheet (returns `SheetWriter`) |
| `set_palette_color(index, r, g, b)` | Override a palette color |
| `save(path)` | Write to file |
| `to_bytes()` | Write to byte vector |

### `SheetWriter`

| Method | Description |
|--------|-------------|
| `write_cell(row, col, value)` | Write a cell value |
| `write_cell_with_color(row, col, value, color)` | Write a cell with background color |
| `write_number(row, col, value)` | Write a number |
| `write_string(row, col, value)` | Write a string |
| `write_color(row, col, color)` | Set background color on an empty cell |

### `XlsColor`

| Item | Description |
|------|-------------|
| `XlsColor::rgb(r, g, b)` | Create a color from RGB values |
| `to_hex()` | Convert to `#RRGGBB` string |
| `XlsColor::BLACK` | Predefined constant |
| `XlsColor::WHITE` | Predefined constant |
| `XlsColor::RED` | Predefined constant |
| `XlsColor::GREEN` | Predefined constant |
| `XlsColor::BLUE` | Predefined constant |
| `XlsColor::YELLOW` | Predefined constant |
| `XlsColor::MAGENTA` | Predefined constant |
| `XlsColor::CYAN` | Predefined constant |

## Limitations

- Only `.xls` (BIFF8) format is supported — `.xlsx` is not supported
- Conditional formatting is not read or written
- Formulas are not evaluated — formula cells are read as their cached values
- Charts, images, and other embedded objects are not supported
- Cell borders and font styling are not supported (background color only)
- Maximum of 56 custom palette colors (BIFF8 palette limitation)

## License

MIT
