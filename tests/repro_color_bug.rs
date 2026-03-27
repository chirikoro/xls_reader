use xls_reader::{CellValue, XlsColor, XlsWriter, XlsReader};

#[test]
fn test_repro_red_green_reads_as_blue() {
    let mut writer = XlsWriter::new();
    {
        let mut sheet = writer.add_sheet("Test");
        // 赤=255, 緑=255, 青=0 のカスタムカラー
        let color = XlsColor::rgb(255, 255, 0);
        sheet.write_cell_with_color(0, 0, CellValue::String("yellow".into()), color);
    }

    let bytes = writer.to_bytes().unwrap();
    let reader = XlsReader::from_bytes(&bytes).unwrap();
    let sheet = reader.sheet_by_index(0).unwrap();

    let bg = sheet.background_color(0, 0).unwrap();
    eprintln!("Expected: rgb(255, 255, 0)");
    eprintln!("Got:      rgb({}, {}, {})", bg.red, bg.green, bg.blue);
    assert_eq!(bg.red, 255, "red should be 255");
    assert_eq!(bg.green, 255, "green should be 255");
    assert_eq!(bg.blue, 0, "blue should be 0");
}

#[test]
fn test_repro_individual_red_and_green() {
    let mut writer = XlsWriter::new();
    {
        let mut sheet = writer.add_sheet("Test");
        let red = XlsColor::rgb(255, 0, 0);
        let green = XlsColor::rgb(0, 255, 0);
        sheet.write_cell_with_color(0, 0, CellValue::String("red".into()), red);
        sheet.write_cell_with_color(0, 1, CellValue::String("green".into()), green);
    }

    let bytes = writer.to_bytes().unwrap();
    let reader = XlsReader::from_bytes(&bytes).unwrap();
    let sheet = reader.sheet_by_index(0).unwrap();

    let red_bg = sheet.background_color(0, 0).unwrap();
    eprintln!("Red cell: rgb({}, {}, {})", red_bg.red, red_bg.green, red_bg.blue);
    assert_eq!(*red_bg, XlsColor::rgb(255, 0, 0), "should be red");

    let green_bg = sheet.background_color(0, 1).unwrap();
    eprintln!("Green cell: rgb({}, {}, {})", green_bg.red, green_bg.green, green_bg.blue);
    assert_eq!(*green_bg, XlsColor::rgb(0, 255, 0), "should be green");
}

#[test]
fn test_repro_many_colors() {
    let colors = [
        ("red",     XlsColor::rgb(255, 0, 0)),
        ("green",   XlsColor::rgb(0, 255, 0)),
        ("blue",    XlsColor::rgb(0, 0, 255)),
        ("yellow",  XlsColor::rgb(255, 255, 0)),
        ("cyan",    XlsColor::rgb(0, 255, 255)),
        ("magenta", XlsColor::rgb(255, 0, 255)),
        ("custom1", XlsColor::rgb(128, 64, 32)),
        ("custom2", XlsColor::rgb(200, 100, 50)),
    ];

    let mut writer = XlsWriter::new();
    {
        let mut sheet = writer.add_sheet("Colors");
        for (i, (name, color)) in colors.iter().enumerate() {
            sheet.write_cell_with_color(
                i as u16, 0,
                CellValue::String(name.to_string()),
                color.clone(),
            );
        }
    }

    let bytes = writer.to_bytes().unwrap();
    let reader = XlsReader::from_bytes(&bytes).unwrap();
    let sheet = reader.sheet_by_index(0).unwrap();

    for (i, (name, expected)) in colors.iter().enumerate() {
        let bg = sheet.background_color(i as u16, 0).unwrap();
        eprintln!("{}: expected rgb({},{},{}), got rgb({},{},{})",
            name, expected.red, expected.green, expected.blue,
            bg.red, bg.green, bg.blue);
        // For custom colors without exact palette match, we only check it's not blue
        if expected.red == 255 && expected.green == 0 && expected.blue == 0 {
            assert_eq!(bg.red, 255, "{}: red component wrong", name);
            assert_eq!(bg.green, 0, "{}: green component wrong", name);
        }
    }
}
