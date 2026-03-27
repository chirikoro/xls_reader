use xls_reader::{CellValue, XlsColor, XlsWriter, XlsReader};

#[test]
fn test_write_and_read_roundtrip() {
    let mut writer = XlsWriter::new();
    {
        let mut sheet = writer.add_sheet("TestSheet");
        sheet.write_string(0, 0, "Hello");
        sheet.write_number(0, 1, 42.0);
        sheet.write_number(1, 0, 3.14);
        sheet.write_string(1, 1, "World");
    }

    let bytes = writer.to_bytes().expect("Failed to write xls");
    let reader = XlsReader::from_bytes(&bytes).expect("Failed to read xls");

    assert_eq!(reader.sheet_count(), 1);
    assert_eq!(reader.sheet_names(), vec!["TestSheet"]);

    let sheet = reader.sheet_by_index(0).unwrap();
    assert_eq!(sheet.value(0, 0), CellValue::String("Hello".to_string()));
    assert_eq!(sheet.value(0, 1), CellValue::Number(42.0));
    assert_eq!(sheet.value(1, 0), CellValue::Number(3.14));
    assert_eq!(sheet.value(1, 1), CellValue::String("World".to_string()));
}

#[test]
fn test_write_and_read_with_colors() {
    let mut writer = XlsWriter::new();
    {
        let mut sheet = writer.add_sheet("Colors");
        sheet.write_cell_with_color(0, 0, CellValue::String("Red BG".into()), XlsColor::RED);
        sheet.write_cell_with_color(0, 1, CellValue::Number(100.0), XlsColor::YELLOW);
        sheet.write_cell_with_color(1, 0, CellValue::Empty, XlsColor::GREEN);
        sheet.write_string(1, 1, "No Color");
    }

    let bytes = writer.to_bytes().expect("Failed to write xls");
    let reader = XlsReader::from_bytes(&bytes).expect("Failed to read xls");

    let sheet = reader.sheet_by_name("Colors").unwrap();

    assert_eq!(sheet.value(0, 0), CellValue::String("Red BG".to_string()));
    assert_eq!(sheet.value(0, 1), CellValue::Number(100.0));
    assert_eq!(sheet.value(1, 1), CellValue::String("No Color".to_string()));

    assert_eq!(sheet.background_color(0, 0), Some(&XlsColor::RED));
    assert_eq!(sheet.background_color(0, 1), Some(&XlsColor::YELLOW));
    assert!(sheet.background_color(1, 0).is_some());
    assert_eq!(sheet.background_color(1, 1), None);
}

#[test]
fn test_multiple_sheets() {
    let mut writer = XlsWriter::new();
    {
        let mut sheet1 = writer.add_sheet("Sheet1");
        sheet1.write_string(0, 0, "First");
    }
    {
        let mut sheet2 = writer.add_sheet("Sheet2");
        sheet2.write_string(0, 0, "Second");
    }

    let bytes = writer.to_bytes().expect("Failed to write xls");
    let reader = XlsReader::from_bytes(&bytes).expect("Failed to read xls");

    assert_eq!(reader.sheet_count(), 2);
    assert_eq!(reader.sheet_names(), vec!["Sheet1", "Sheet2"]);

    assert_eq!(
        reader.sheet_by_name("Sheet1").unwrap().value(0, 0),
        CellValue::String("First".to_string())
    );
    assert_eq!(
        reader.sheet_by_name("Sheet2").unwrap().value(0, 0),
        CellValue::String("Second".to_string())
    );
}

#[test]
fn test_bool_and_error_cells() {
    let mut writer = XlsWriter::new();
    {
        let mut sheet = writer.add_sheet("BoolErr");
        sheet.write_cell(0, 0, CellValue::Bool(true));
        sheet.write_cell(0, 1, CellValue::Bool(false));
        sheet.write_cell(1, 0, CellValue::Error(0x07));
    }

    let bytes = writer.to_bytes().expect("Failed to write xls");
    let reader = XlsReader::from_bytes(&bytes).expect("Failed to read xls");

    let sheet = reader.sheet_by_index(0).unwrap();
    assert_eq!(sheet.value(0, 0), CellValue::Bool(true));
    assert_eq!(sheet.value(0, 1), CellValue::Bool(false));
    assert_eq!(sheet.value(1, 0), CellValue::Error(0x07));
}

#[test]
fn test_cell_value_display() {
    assert_eq!(format!("{}", CellValue::Empty), "");
    assert_eq!(format!("{}", CellValue::Number(3.14)), "3.14");
    assert_eq!(format!("{}", CellValue::String("test".into())), "test");
    assert_eq!(format!("{}", CellValue::Bool(true)), "true");
    assert_eq!(format!("{}", CellValue::Error(0x07)), "#ERR(7)");
}

#[test]
fn test_color_hex() {
    assert_eq!(XlsColor::RED.to_hex(), "#FF0000");
    assert_eq!(XlsColor::rgb(0, 128, 255).to_hex(), "#0080FF");
    assert_eq!(XlsColor::BLACK.to_hex(), "#000000");
}

#[test]
fn test_empty_value_methods() {
    let empty = CellValue::Empty;
    assert!(empty.is_empty());
    assert!(empty.as_f64().is_none());
    assert!(empty.as_str().is_none());

    let num = CellValue::Number(42.0);
    assert!(!num.is_empty());
    assert_eq!(num.as_f64(), Some(42.0));

    let s = CellValue::String("hello".into());
    assert_eq!(s.as_str(), Some("hello"));
}

#[test]
fn test_sheet_dimensions() {
    let mut writer = XlsWriter::new();
    {
        let mut sheet = writer.add_sheet("Dim");
        sheet.write_number(2, 3, 1.0);
        sheet.write_number(5, 7, 2.0);
    }

    let bytes = writer.to_bytes().expect("Failed to write xls");
    let reader = XlsReader::from_bytes(&bytes).expect("Failed to read xls");

    let sheet = reader.sheet_by_index(0).unwrap();
    assert_eq!(sheet.max_row(), Some(5));
    assert_eq!(sheet.max_col(), Some(7));
}

#[test]
fn test_japanese_strings() {
    let mut writer = XlsWriter::new();
    {
        let mut sheet = writer.add_sheet("日本語テスト");
        sheet.write_string(0, 0, "こんにちは");
        sheet.write_string(0, 1, "世界");
    }

    let bytes = writer.to_bytes().expect("Failed to write xls");
    let reader = XlsReader::from_bytes(&bytes).expect("Failed to read xls");

    assert_eq!(reader.sheet_names(), vec!["日本語テスト"]);
    let sheet = reader.sheet_by_index(0).unwrap();
    assert_eq!(sheet.value(0, 0), CellValue::String("こんにちは".to_string()));
    assert_eq!(sheet.value(0, 1), CellValue::String("世界".to_string()));
}

#[test]
fn test_file_save_and_open() {
    let dir = std::env::temp_dir();
    let path = dir.join("test_xls_reader.xls");

    let mut writer = XlsWriter::new();
    {
        let mut sheet = writer.add_sheet("FileTest");
        sheet.write_string(0, 0, "saved to file");
        sheet.write_cell_with_color(0, 1, CellValue::Number(99.0), XlsColor::BLUE);
    }

    writer.save(&path).expect("Failed to save xls file");

    let reader = XlsReader::open(&path).expect("Failed to open xls file");
    let sheet = reader.sheet_by_index(0).unwrap();
    assert_eq!(sheet.value(0, 0), CellValue::String("saved to file".to_string()));
    assert_eq!(sheet.value(0, 1), CellValue::Number(99.0));
    assert_eq!(sheet.background_color(0, 1), Some(&XlsColor::BLUE));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_value_ref_no_clone() {
    let mut writer = XlsWriter::new();
    {
        let mut sheet = writer.add_sheet("Ref");
        sheet.write_string(0, 0, "hello");
        sheet.write_number(0, 1, 42.0);
    }

    let bytes = writer.to_bytes().unwrap();
    let reader = XlsReader::from_bytes(&bytes).unwrap();
    let sheet = reader.sheet_by_index(0).unwrap();

    // value_ref returns a reference without cloning
    assert_eq!(sheet.value_ref(0, 0).as_str(), Some("hello"));
    assert_eq!(sheet.value_ref(0, 1).as_f64(), Some(42.0));
    assert!(sheet.value_ref(99, 99).is_empty());
}
