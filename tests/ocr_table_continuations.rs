#![cfg(all(feature = "ocr", not(target_arch = "wasm32")))]

#[path = "support/table_pages.rs"]
mod table_pages;

use pdf_inspector::{
    vision::{process_pdf_with_ocr_mem, OcrPdfOptions, OcrPdfResult},
    MarkdownOptions,
};
use table_pages::{
    make_table_pages_pdf, repeated_header_pdf, TablePage, CONTINUATION, HEADER, START,
};

const MORE: &[&[&str]] = &[
    HEADER,
    &["Upstate region", "119", "Awaiting approval"],
    &["Downstate region", "136", "Ready for review"],
    &["Overseas region", "153", "Reviewed and approved"],
];
const START_TWO: &[&[&str]] = &[
    &["Region", "Amount"],
    &["Northern region", "17"],
    &["Central region", "34"],
    &["Coastal region", "51"],
];
const HEADER_LINE: &str = "|Region|Amount|Status|";
const SEPARATOR: &str = "|---|---|---|";
const START_LAST: &str = "|Coastal region|51|Ready for review|";
const CONTINUATION_FIRST: &str = "|Southern region|68|Awaiting approval|";
const CONTINUATION_LAST: &str = "|Eastern region|102|Reviewed and approved|";
const MORE_FIRST: &str = "|Upstate region|119|Awaiting approval|";
const MORE_LAST: &str = "|Overseas region|153|Reviewed and approved|";

fn page<'a>(rows: &'a [&'a [&'a str]]) -> TablePage<'a> {
    TablePage {
        rows,
        before: "",
        after: "",
    }
}

fn convert(pdf: &[u8], markers: bool) -> OcrPdfResult {
    process_pdf_with_ocr_mem(
        pdf,
        OcrPdfOptions::new().markdown(MarkdownOptions {
            include_page_numbers: markers,
            ..MarkdownOptions::default()
        }),
    )
    .unwrap()
}

#[test]
fn repeated_header_is_removed_between_trailing_and_leading_tables() {
    let pdf = repeated_header_pdf();
    for markers in [false, true] {
        let result = convert(&pdf, markers);
        let markdown = &result.markdown;
        assert!(
            markdown.contains("The regional report begins"),
            "{markdown}"
        );
        assert!(
            markdown.contains(&format!("{START_LAST}\n{CONTINUATION_FIRST}")),
            "{markdown}"
        );
        assert_eq!(markdown.matches(HEADER_LINE).count(), 1, "{markdown}");
        let after = if markers {
            "\n\n<!-- Page 2 -->\n\n"
        } else {
            "\n\n"
        };
        assert!(
            markdown.contains(&format!(
                "{CONTINUATION_LAST}{after}# This paragraph follows"
            )),
            "{markdown}"
        );
        assert_eq!(
            markdown.matches("<!-- Page ").count(),
            if markers { 2 } else { 0 }
        );
        assert!(result
            .pages
            .iter()
            .all(|page| page.markdown.contains(HEADER_LINE)));
    }
}

#[test]
fn continuation_header_with_different_cells_becomes_a_data_row() {
    let pdf = make_table_pages_pdf(&[page(START), page(&CONTINUATION[1..])]);
    for markers in [false, true] {
        let result = convert(&pdf, markers);
        let markdown = &result.markdown;
        assert!(
            markdown.contains(&format!("{START_LAST}\n{CONTINUATION_FIRST}")),
            "{markdown}"
        );
        assert_eq!(markdown.matches(SEPARATOR).count(), 1, "{markdown}");
        assert!(markdown.contains(CONTINUATION_LAST), "{markdown}");
        let suffix = if markers { "\n\n<!-- Page 2 -->" } else { "" };
        assert!(
            markdown
                .trim_end()
                .ends_with(&format!("{CONTINUATION_LAST}{suffix}")),
            "{markdown}"
        );
    }
}

#[test]
fn different_column_counts_separate_tables() {
    let pdf = make_table_pages_pdf(&[page(START_TWO), page(CONTINUATION), page(MORE)]);
    for markers in [false, true] {
        let result = convert(&pdf, markers);
        let markdown = &result.markdown;
        assert!(
            markdown.contains("|Region|Amount|\n|---|---|"),
            "{markdown}"
        );
        assert!(
            markdown.contains(&format!("{HEADER_LINE}\n{SEPARATOR}")),
            "{markdown}"
        );
        assert_eq!(markdown.matches(HEADER_LINE).count(), 1, "{markdown}");
        assert!(
            markdown.contains(&format!("{CONTINUATION_LAST}\n{MORE_FIRST}")),
            "{markdown}"
        );
    }
}

#[test]
fn prose_after_or_before_a_table_blocks_the_join() {
    let prose = "The note between these tables applies only to the preceding results.";
    for prose_on_first_page in [true, false] {
        let mut first = page(START);
        let mut second = page(CONTINUATION);
        if prose_on_first_page {
            first.after = prose;
        } else {
            second.before = prose;
        }
        let pdf = make_table_pages_pdf(&[first, second, page(&MORE[1..])]);
        for markers in [false, true] {
            let result = convert(&pdf, markers);
            let markdown = &result.markdown;
            assert_eq!(markdown.matches(HEADER_LINE).count(), 2, "{markdown}");
            assert!(
                markdown.find(START_LAST).unwrap() < markdown.find(prose).unwrap()
                    && markdown.find(prose).unwrap() < markdown.find(CONTINUATION_FIRST).unwrap(),
                "{markdown}"
            );
            assert!(
                markdown.contains(&format!("{CONTINUATION_LAST}\n{MORE_FIRST}")),
                "{markdown}"
            );
        }
    }
}

#[test]
fn table_chain_uses_the_original_header_and_keeps_all_page_markers() {
    let pdf = make_table_pages_pdf(&[page(START), page(&CONTINUATION[1..]), page(MORE)]);
    for markers in [false, true] {
        let result = convert(&pdf, markers);
        let markdown = &result.markdown;
        assert_eq!(markdown.matches(HEADER_LINE).count(), 1, "{markdown}");
        assert_eq!(markdown.matches(SEPARATOR).count(), 1, "{markdown}");
        assert!(
            markdown.contains(&format!("{START_LAST}\n{CONTINUATION_FIRST}")),
            "{markdown}"
        );
        assert!(
            markdown.contains(&format!("{CONTINUATION_LAST}\n{MORE_FIRST}")),
            "{markdown}"
        );
        let pages_after_rows = markdown.split_once(MORE_LAST).unwrap().1;
        let expected = if markers {
            "\n\n<!-- Page 2 -->\n\n<!-- Page 3 -->\n"
        } else {
            "\n"
        };
        assert_eq!(pages_after_rows, expected, "{markdown}");
    }
}
