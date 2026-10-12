//! Office files read as grounding: a Word document, an Excel workbook, and a PowerPoint deck,
//! each made here as the smallest package its application writes, read back as the text the
//! model is given.

use scaena_sources::{Kind, read};
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;

/// A package of `parts`, each a path and its XML.
fn package(parts: &[(&str, &str)]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (path, xml) in parts {
        zip.start_file(*path, SimpleFileOptions::default()).unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

const CORE: &str = r#"<?xml version="1.0"?><cp:coreProperties xmlns:cp="c" xmlns:dc="d"><dc:title>Tidewater Q3</dc:title></cp:coreProperties>"#;

#[test]
fn a_word_document_reads_with_its_headings_lists_and_tables() {
    let document = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="w"><w:body>
  <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Results</w:t></w:r></w:p>
  <w:p><w:r><w:t xml:space="preserve">Revenue grew </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>41%</w:t></w:r><w:del><w:r><w:delText>39%</w:delText></w:r></w:del><w:r><w:t>.</w:t></w:r></w:p>
  <w:p><w:pPr><w:numPr><w:ilvl w:val="0"/></w:numPr></w:pPr><w:r><w:t>Four ports</w:t></w:r></w:p>
  <w:tbl><w:tr><w:tc><w:p><w:r><w:t>Port</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Share</w:t></w:r></w:p></w:tc></w:tr>
         <w:tr><w:tc><w:p><w:r><w:t>Houston</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>41%</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
  <w:sdt><w:sdtContent><w:p><w:r><w:t>In a control</w:t></w:r></w:p></w:sdtContent></w:sdt>
</w:body></w:document>"#;
    let docx = package(&[("word/document.xml", document), ("docProps/core.xml", CORE)]);
    let s = read("q3.docx", &docx).unwrap();
    assert_eq!(s.kind, Kind::Word);
    assert_eq!(
        s.text,
        "# Tidewater Q3\n# Results\nRevenue grew 41%.\n- Four ports\nPort\tShare\nHouston\t41%\n\nIn a control"
    );
}

#[test]
fn a_workbook_reads_each_sheet_under_its_name() {
    let workbook = r#"<workbook xmlns="m" xmlns:r="r"><sheets>
  <sheet name="Revenue" sheetId="1" r:id="rId1"/><sheet name="Empty" sheetId="2" r:id="rId2"/></sheets></workbook>"#;
    let rels = r#"<Relationships xmlns="p">
  <Relationship Id="rId1" Type="http://schemas/worksheet" Target="worksheets/sheet1.xml"/>
  <Relationship Id="rId2" Type="http://schemas/worksheet" Target="/xl/worksheets/sheet2.xml"/></Relationships>"#;
    let strings =
        r#"<sst xmlns="m"><si><t>Quarter</t></si><si><t>Revenue</t></si><si><r><t>Q</t></r><r><t>1</t></r></si></sst>"#;
    let sheet = r#"<worksheet xmlns="m"><sheetData>
  <row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c></row>
  <row r="2"><c r="A2" t="s"><v>2</v></c><c r="C2"><v>12.5</v></c></row>
  <row r="3"><c r="A3" t="inlineStr"><is><t>Q2</t></is></c><c r="B3" t="b"><v>1</v></c></row>
</sheetData></worksheet>"#;
    let empty = r#"<worksheet xmlns="m"><sheetData/></worksheet>"#;
    let xlsx = package(&[
        ("xl/workbook.xml", workbook),
        ("xl/_rels/workbook.xml.rels", rels),
        ("xl/sharedStrings.xml", strings),
        ("xl/worksheets/sheet1.xml", sheet),
        ("xl/worksheets/sheet2.xml", empty),
    ]);
    let s = read("revenue.xlsx", &xlsx).unwrap();
    assert_eq!(s.kind, Kind::Sheets);
    assert_eq!(s.sheets.len(), 2);
    assert_eq!(s.sheets[0].rows, vec![vec!["Quarter", "Revenue"], vec!["Q1", "", "12.5"], vec!["Q2", "TRUE"]]);
    assert_eq!(s.text, "## Sheet: Revenue\nQuarter\tRevenue\nQ1\t\t12.5\nQ2\tTRUE");
}

#[test]
fn a_deck_reads_its_slides_notes_charts_and_sections() {
    let presentation = r#"<p:presentation xmlns:p="p" xmlns:r="r" xmlns:p14="p14">
  <p:sldIdLst><p:sldId id="256" r:id="rId2"/><p:sldId id="257" r:id="rId3"/></p:sldIdLst>
  <p:extLst><p:ext uri="x"><p14:sectionLst><p14:section name="Proof"><p14:sldIdLst><p14:sldId id="257"/></p14:sldIdLst></p14:section></p14:sectionLst></p:ext></p:extLst>
</p:presentation>"#;
    let rels = r#"<Relationships xmlns="p">
  <Relationship Id="rId2" Type="http://schemas/slide" Target="slides/slide1.xml"/>
  <Relationship Id="rId3" Type="http://schemas/slide" Target="slides/slide2.xml"/></Relationships>"#;
    let slide = |title: &str, body: &str, extra: &str| {
        format!(
            r#"<p:sld xmlns:p="p" xmlns:a="a" xmlns:r="r" xmlns:c="c"><p:cSld><p:spTree>
  <p:sp><p:nvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:txBody><a:p><a:r><a:t>{title}</a:t></a:r></a:p></p:txBody></p:sp>
  <p:grpSp><p:sp><p:nvSpPr><p:nvPr><p:ph idx="1"/></p:nvPr></p:nvSpPr><p:txBody><a:p><a:r><a:t>{body}</a:t></a:r></a:p><a:p><a:r><a:t> </a:t></a:r></a:p></p:txBody></p:sp></p:grpSp>
  {extra}
</p:spTree></p:cSld></p:sld>"#
        )
    };
    let one = slide("Shipping's carbon bill", "3% of global CO2", "");
    let frame = r#"<p:graphicFrame><a:graphic><a:graphicData><c:chart r:id="rId9"/></a:graphicData></a:graphic></p:graphicFrame>"#;
    let two = slide("Revenue tripled", "Four ports live", frame);
    let two_rels = r#"<Relationships xmlns="p">
  <Relationship Id="rId9" Type="http://schemas/chart" Target="../charts/chart1.xml"/>
  <Relationship Id="rId7" Type="http://schemas/notesSlide" Target="../notesSlides/notesSlide2.xml"/></Relationships>"#;
    let chart = r#"<c:chartSpace xmlns:c="c" xmlns:a="a"><c:chart>
  <c:title><c:tx><c:rich><a:p><a:r><a:t>Revenue ($M)</a:t></a:r></a:p></c:rich></c:tx></c:title>
  <c:plotArea><c:barChart><c:ser>
    <c:tx><c:strRef><c:strCache><c:pt idx="0"><c:v>Revenue</c:v></c:pt></c:strCache></c:strRef></c:tx>
    <c:cat><c:strRef><c:strCache><c:pt idx="0"><c:v>2024</c:v></c:pt><c:pt idx="1"><c:v>2025</c:v></c:pt><c:pt idx="2"><c:v>2026</c:v></c:pt></c:strCache></c:strRef></c:cat>
    <c:val><c:numRef><c:numCache><c:formatCode>$#,##0.0</c:formatCode><c:pt idx="0"><c:v>0.4</c:v></c:pt><c:pt idx="2"><c:v>4.1</c:v></c:pt></c:numCache></c:numRef></c:val>
  </c:ser></c:barChart></c:plotArea></c:chart></c:chartSpace>"#;
    let notes = r#"<p:notes xmlns:p="p" xmlns:a="a"><p:cSld><p:spTree>
  <p:sp><p:nvSpPr><p:nvPr><p:ph type="sldImg"/></p:nvPr></p:nvSpPr></p:sp>
  <p:sp><p:nvSpPr><p:nvPr><p:ph type="body" idx="1"/></p:nvPr></p:nvSpPr><p:txBody><a:p><a:r><a:t>Pause on the jump.</a:t></a:r></a:p></p:txBody></p:sp>
  <p:sp><p:nvSpPr><p:nvPr><p:ph type="sldNum"/></p:nvPr></p:nvSpPr><p:txBody><a:p><a:r><a:t>2</a:t></a:r></a:p></p:txBody></p:sp>
</p:spTree></p:cSld></p:notes>"#;
    let pptx = package(&[
        ("ppt/presentation.xml", presentation),
        ("ppt/_rels/presentation.xml.rels", rels),
        ("ppt/slides/slide1.xml", &one),
        ("ppt/slides/slide2.xml", &two),
        ("ppt/slides/_rels/slide2.xml.rels", two_rels),
        ("ppt/charts/chart1.xml", chart),
        ("ppt/notesSlides/notesSlide2.xml", notes),
        ("docProps/core.xml", CORE),
    ]);
    let s = read("pitch.pptx", &pptx).unwrap();
    assert_eq!(s.kind, Kind::Slides);
    assert_eq!(
        s.text,
        "# Tidewater Q3\n\n## Slide 1: Shipping's carbon bill\n3% of global CO2\n\n# Section: Proof\n\n## Slide 2: Revenue tripled\nFour ports live\nChart \"Revenue ($M)\" (bar):\n  Revenue [$#,##0.0]: 2024=0.4, 2025=\u{2014}, 2026=4.1\nSpeaker notes: Pause on the jump."
    );
    // No slide of it is a slide of the deck: only these words reach the model.
    assert!(read("pitch.pptx", b"not a zip").is_err());
}
