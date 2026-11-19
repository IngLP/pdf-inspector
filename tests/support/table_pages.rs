use std::fmt::Write;

pub struct TablePage<'a> {
    pub rows: &'a [&'a [&'a str]],
    pub before: &'a str,
    pub after: &'a str,
}

pub const HEADER: &[&str] = &["Region", "Amount", "Status"];

pub const START: &[&[&str]] = &[
    HEADER,
    &["Northern region", "17", "Ready for review"],
    &["Central region", "34", "Awaiting approval"],
    &["Coastal region", "51", "Ready for review"],
];
pub const CONTINUATION: &[&[&str]] = &[
    HEADER,
    &["Southern region", "68", "Awaiting approval"],
    &["Western region", "85", "Ready for review"],
    &["Eastern region", "102", "Reviewed and approved"],
];

pub fn repeated_header_pdf() -> Vec<u8> {
    make_table_pages_pdf(&[
        TablePage {
            rows: START,
            before: "The regional report begins with the results shown in this table.",
            after: "",
        },
        TablePage {
            rows: CONTINUATION,
            before: "",
            after: "This paragraph follows the final row of the regional table.",
        },
    ])
}

// A small uncompressed PDF builder using only the standard library, so the
// same fixture can be built by rustc for the Windows launcher smoke test.
pub fn make_table_pages_pdf(pages: &[TablePage<'_>]) -> Vec<u8> {
    let kids = (0..pages.len())
        .map(|index| format!("{} 0 R", 4 + index * 2))
        .collect::<Vec<_>>()
        .join(" ");
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        format!("<< /Type /Pages /Count {} /Kids [{kids}] >>", pages.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    for page in pages {
        let content_id = objects.len() + 2;
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
             /Resources << /Font << /F1 3 0 R >> >> /Contents {content_id} 0 R >>"
        ));
        // The fixture strings are ASCII and contain no PDF literal-string
        // punctuation. Draw both axes so the line-based detector owns the grid.
        let mut content = String::new();
        if !page.before.is_empty() {
            writeln!(content, "BT /F1 12 Tf 72 750 Td ({}) Tj ET", page.before).unwrap();
        }
        let cols = page.rows[0].len();
        let bottom = 700 - page.rows.len() * 22;
        let right = 72 + cols * 140;
        for col in 0..=cols {
            let x = 72 + col * 140;
            writeln!(content, "{x} {bottom} m {x} 700 l S").unwrap();
        }
        for row in 0..=page.rows.len() {
            let y = 700 - row * 22;
            writeln!(content, "72 {y} m {right} {y} l S").unwrap();
        }
        for (row, cells) in page.rows.iter().enumerate() {
            assert_eq!(cells.len(), cols);
            for (col, cell) in cells.iter().enumerate() {
                let (x, y) = (78 + col * 140, 685 - row * 22);
                writeln!(content, "BT /F1 10 Tf {x} {y} Td ({cell}) Tj ET").unwrap();
            }
        }
        if !page.after.is_empty() {
            writeln!(content, "BT /F1 12 Tf 72 410 Td ({}) Tj ET", page.after).unwrap();
        }
        objects.push(format!(
            "<< /Length {} >>\nstream\n{content}endstream",
            content.len()
        ));
    }

    let mut pdf = "%PDF-1.4\n".to_string();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        writeln!(pdf, "{} 0 obj\n{object}\nendobj", index + 1).unwrap();
    }
    let xref = pdf.len();
    let size = offsets.len() + 1;
    writeln!(pdf, "xref\n0 {size}\n0000000000 65535 f ").unwrap();
    for offset in offsets {
        writeln!(pdf, "{offset:010} 00000 n ").unwrap();
    }
    write!(
        pdf,
        "trailer\n<< /Root 1 0 R /Size {size} >>\nstartxref\n{xref}\n%%EOF\n"
    )
    .unwrap();
    pdf.into_bytes()
}
