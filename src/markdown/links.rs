//! Anchoring `/Link /URI` annotations to the text they decorate.
//!
//! Extraction hands the Markdown pipeline the page's link annotations as
//! [`ItemType::Link`] items carrying the annotation's rectangle
//! (`extractor::links`). This module turns them into the two things the
//! pipeline emits: a per-page [`PageLinkAnchors`] index that line rendering
//! consults to wrap the covered text in `[anchor](url)`, and a record per
//! annotation so a consumer never has to re-parse the Markdown to learn where
//! a link pointed.
//!
//! A link whose rectangle covers no rendered text — the clickable logo, the
//! social icon, the bitmap chart used as a button — has no anchor to attach
//! to. Its destination is not dropped: it is listed at the foot of its page
//! under [`LINK_LIST_HEADING`], together with the image sitting under the
//! rectangle when there is one, so it survives into the Markdown. The list
//! carries destinations rather than annotations, so a URL the page already
//! anchors, and a second rectangle on the same URL, add no entry to it; a
//! destination the Markdown cannot carry at all ([`crate::types`]'s scheme
//! rule) gets none either, and is reported only through [`MarkdownLink`].

use std::collections::{HashMap, HashSet};

use crate::tables::Table;
use crate::types::{ItemType, LinkAnchor, PageLinkAnchors, TextItem, TextLine};

/// Heading the per-page list of unanchored links is written under. The
/// literal is part of the emitted Markdown, so it is repeated verbatim in the
/// public documentation of `PageMarkdown::links` and in the tests that pin the
/// output; change it in all of them or in none.
pub(crate) const LINK_LIST_HEADING: &str = "**Links on this page**";

/// What became of one `/Link /URI` annotation.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkdownLink {
    /// Destination URI.
    pub url: String,
    /// The annotation's `/Rect` as `(x, y, width, height)`, with `y` the
    /// bottom edge and growing upward, in the same frame as [`TextItem`].
    pub rect: (f32, f32, f32, f32),
    /// 1-indexed page carrying the annotation.
    pub page: u32,
    /// The text under the rectangle, when there is any.
    ///
    /// Present whenever extraction found words there, which is not the same as
    /// those words reaching the output: see [`Self::anchored_inline`].
    pub anchor: Option<String>,
    /// `true` when a line or a table cell handed to the Markdown renderer
    /// carried this annotation's anchor, which normally means the output shows
    /// it as `[anchor](url)`.
    ///
    /// `false` says only that none did — not that no text sat under the
    /// rectangle. [`Self::anchor`] can be `Some` alongside it: a folio, a
    /// stripped running header, a cell whose joined text the anchor could not
    /// be located in, all leave words under a rectangle that no rendered line
    /// carries. Those destinations are the ones the page's
    /// `**Links on this page**` list exists for, though the list is not a
    /// promise either — it leaves out a destination another annotation already
    /// anchored on the page, and one whose scheme the Markdown does not carry
    /// (`javascript:`, a bare `#`). [`Self::url`] is the only complete record.
    pub anchored_inline: bool,
}

/// The link annotations of a conversion, indexed by 1-indexed page.
pub(crate) fn anchors_by_page(links: &[TextItem]) -> HashMap<u32, PageLinkAnchors> {
    let mut by_page: HashMap<u32, Vec<LinkAnchor>> = HashMap::new();
    for link in links {
        let ItemType::Link(url) = &link.item_type else {
            continue;
        };
        by_page.entry(link.page).or_default().push(LinkAnchor {
            url: url.clone(),
            x: link.x,
            y: link.y,
            width: link.width,
            height: link.height,
            writes_out_url: false,
        });
    }
    by_page
        .into_iter()
        .map(|(page, anchors)| (page, PageLinkAnchors::new(anchors)))
        .collect()
}

/// The text under each annotation's rectangle, keyed by the annotation's page
/// and its position in that page's anchor list. Position, not URL, because one
/// page routinely carries the same URL twice.
pub(crate) fn anchor_texts<'a>(
    items: impl Iterator<Item = &'a TextItem>,
    anchors: &HashMap<u32, PageLinkAnchors>,
) -> HashMap<(u32, usize), String> {
    let mut texts: HashMap<(u32, usize), String> = HashMap::new();
    for item in items {
        let Some(page_anchors) = anchors.get(&item.page) else {
            continue;
        };
        // A super/subscript run is rendered unlinked (its `<sup>` tags would
        // have to nest inside the anchor), so it must not be counted as one
        // here either — an annotation credited with an anchor nothing emits
        // would be missing from the page's link list as well.
        if item.text.trim().is_empty() || item.is_script() {
            continue;
        }
        for range in page_anchors.ranges_for(item) {
            let claimed: String = item
                .text
                .chars()
                .skip(range.start)
                .take(range.end - range.start)
                .collect();
            let claimed = claimed.trim();
            if claimed.is_empty() {
                continue;
            }
            let text = texts.entry((item.page, range.index)).or_default();
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(claimed);
        }
    }
    texts
}

/// Report a page's annotations with no anchor at all, for a page whose text
/// never reached conversion: the destinations are exact file data and survive,
/// but nothing extraction produced there can be trusted as anchor text.
pub(crate) fn unanchored(items: &[TextItem], include_links: bool) -> Vec<MarkdownLink> {
    if !include_links {
        return Vec::new();
    }
    items
        .iter()
        .filter_map(|item| {
            let ItemType::Link(url) = &item.item_type else {
                return None;
            };
            Some(MarkdownLink {
                url: url.clone(),
                rect: (item.x, item.y, item.width, item.height),
                page: item.page,
                anchor: None,
                anchored_inline: false,
            })
        })
        .collect()
}

/// Decide, for every annotation, what text sits under it and whether that text
/// reached the Markdown.
///
/// The annotations come from `anchors`, which is what every other step reads
/// too, so the position each record is keyed by cannot drift from the one
/// [`anchor_texts`] and [`anchor_table_cells`] used.
///
/// `table_anchors` holds the annotations a table cell already claimed. The
/// rest are judged against the very lines the renderer is about to walk, so a
/// link reported unanchored is exactly one whose text is in neither the prose
/// nor a table — even when `anchor` shows what the rectangle covers, because
/// a folio, a stripped running header or a cell the anchor could not be
/// located in all leave text under a rectangle that no output carries.
pub(crate) fn resolve(
    anchors: &HashMap<u32, PageLinkAnchors>,
    lines: &[TextLine],
    anchor_texts_by_link: &HashMap<(u32, usize), String>,
    table_anchors: &HashMap<(u32, usize), String>,
) -> Vec<MarkdownLink> {
    let line_items = lines.iter().flat_map(|line| line.items.iter());
    let line_anchors = anchor_texts(line_items, anchors);

    let mut pages: Vec<u32> = anchors.keys().copied().collect();
    pages.sort_unstable();
    let mut resolved = Vec::new();
    for page in pages {
        for (index, anchor) in anchors[&page].iter().enumerate() {
            resolved.push(MarkdownLink {
                url: anchor.url.clone(),
                rect: (anchor.x, anchor.y, anchor.width, anchor.height),
                page,
                anchor: anchor_texts_by_link.get(&(page, index)).cloned(),
                // Carrying the anchor is necessary but not sufficient: a
                // destination the Markdown does not write (`javascript:`, a
                // bare `#`) leaves its words in the text unwrapped, so no
                // link was emitted for it however much text it covered.
                anchored_inline: (line_anchors.contains_key(&(page, index))
                    || table_anchors.contains_key(&(page, index)))
                    && crate::types::destination_allowed_in_markdown(&anchor.url),
            });
        }
    }
    resolved
}

/// Wrap in `[anchor](url)` every cell of `table` whose text an annotation
/// covers, recording what each claiming annotation anchored.
///
/// Table cells are joined from their items long before the Markdown is
/// assembled, so the character-level splice line rendering uses is not
/// available here. Instead the rectangle's `x` picks the column — column
/// boundaries are the one part of a detected table's geometry that maps
/// cleanly onto a rectangle — and the anchor text picks the row among the
/// cells of that column, nearest first. Geometry proposes, the text verifies:
/// an annotation whose text is in no cell of its column anchors nothing, and
/// its URL falls through to the page's link list rather than decorating the
/// wrong words.
pub(crate) fn anchor_table_cells(
    page: u32,
    table: &Table,
    anchors: &PageLinkAnchors,
    texts: &HashMap<(u32, usize), String>,
    claimed: &mut HashMap<(u32, usize), String>,
) -> Option<Table> {
    if table.columns.is_empty() || table.cells.is_empty() {
        return None;
    }
    let mut cells = table.cells.clone();
    let mut changed = false;
    for (index, anchor) in anchors.iter().enumerate() {
        let key = (page, index);
        if claimed.contains_key(&key) || !crate::types::destination_allowed_in_markdown(&anchor.url)
        {
            continue;
        }
        let Some(text) = texts.get(&key) else {
            continue;
        };
        let (centre_x, centre_y) = anchor.centre();
        if !spans_ordinate(&table.rows, centre_y) {
            continue;
        }
        let Some(column) = nearest_ordinate(&table.columns, centre_x) else {
            continue;
        };
        let Some((row, start)) = cells
            .iter()
            .enumerate()
            .filter_map(|(index, row)| {
                let cell = row.get(column)?;
                find_outside_links(cell, text).map(|start| (index, start))
            })
            .min_by(|(a, _), (b, _)| {
                ordinate_distance(&table.rows, *a, centre_y).total_cmp(&ordinate_distance(
                    &table.rows,
                    *b,
                    centre_y,
                ))
            })
        else {
            continue;
        };
        let replacement = if anchor.writes_out_url {
            text.clone()
        } else {
            crate::types::inline_link(text, &anchor.url)
        };
        cells[row][column].replace_range(start..start + text.len(), &replacement);
        claimed.insert(key, text.clone());
        changed = true;
    }
    changed.then(|| Table {
        columns: table.columns.clone(),
        rows: table.rows.clone(),
        cells,
        item_indices: table.item_indices.clone(),
        kind: table.kind,
    })
}

/// First occurrence of `needle` in `cell` that is not already inside a
/// Markdown link, so a second annotation on the same cell splices beside the
/// first instead of inside its brackets. One cell often joins several linked
/// names ("Northwind", "Delta Works") that arrived as separate items.
///
/// `needle` is an anchor text, which [`anchor_texts`] never leaves empty; an
/// empty one would not advance the scan.
fn find_outside_links(cell: &str, needle: &str) -> Option<usize> {
    debug_assert!(!needle.is_empty(), "anchor text is never empty");
    if needle.is_empty() {
        return None;
    }
    let mut from = 0usize;
    while let Some(offset) = cell[from..].find(needle) {
        let start = from + offset;
        if !inside_link(cell, start) {
            return Some(start);
        }
        from = start + needle.len();
        if from >= cell.len() {
            break;
        }
    }
    None
}

/// True when byte `position` falls inside a `[text](destination)` span.
///
/// The scan is driven by `](`, not by `[`, because a cell routinely carries a
/// bracket that opens nothing — a bibliography reference like `[3]`, a
/// footnote marker. Taking the first `[` as a link's start would stretch the
/// span from that bracket to the end of the next real link and swallow every
/// word between them. The link's text starts at the *last* bracket before its
/// `](`, so `[3] si [x](u)` leaves "si" outside.
fn inside_link(cell: &str, position: usize) -> bool {
    let mut cursor = 0usize;
    while let Some(offset) = cell[cursor..].find("](") {
        let close = cursor + offset;
        let Some(end) = cell[close..].find(')').map(|offset| close + offset) else {
            return false;
        };
        match last_unescaped_bracket(&cell[cursor..close]).map(|offset| cursor + offset) {
            Some(open) if position >= open => {
                if position < end {
                    return true;
                }
            }
            Some(_) => return false,
            None => {}
        }
        cursor = end + 1;
        if cursor >= cell.len() {
            break;
        }
    }
    false
}

/// Byte offset of the last `[` in `text` that opens something, skipping the
/// `\[` [`crate::types::inline_link`] writes for a bracket inside an anchor.
/// Without that, a cell already carrying `[Models \[3\]](url)` would look as
/// if its link started at the escaped bracket, and every word before it would
/// read as outside the link it is plainly inside.
fn last_unescaped_bracket(text: &str) -> Option<usize> {
    let mut found = None;
    let mut escaped = false;
    for (offset, character) in text.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '[' => found = Some(offset),
            _ => {}
        }
    }
    found
}

/// True when `position` falls within the ordinates' own span, widened by one
/// step at each end so a rectangle drawn a little past the first or last row
/// still counts.
///
/// Without it a cell could be anchored by an annotation living anywhere else
/// on the page, since the text match alone is happy to find "si" in a table
/// far from the prose the rectangle decorates.
fn spans_ordinate(ordinates: &[f32], position: f32) -> bool {
    let (Some(&first), Some(&last)) = (ordinates.first(), ordinates.last()) else {
        return false;
    };
    let (low, high) = if first <= last {
        (first, last)
    } else {
        (last, first)
    };
    let margin = if ordinates.len() > 1 {
        (high - low) / (ordinates.len() - 1) as f32
    } else {
        0.0
    };
    position >= low - margin && position <= high + margin
}

/// Index of the ordinate closest to `position`.
///
/// A detected table's `columns` and `rows` are the ordinates the detector
/// clustered its items around, not cell edges, so a rectangle is assigned to
/// the nearest one rather than to the last one before it.
fn nearest_ordinate(ordinates: &[f32], position: f32) -> Option<usize> {
    ordinates
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (*a - position).abs().total_cmp(&(*b - position).abs()))
        .map(|(index, _)| index)
}

/// Distance from an ordinate to `position`. An index with no recorded
/// ordinate — `rows` and `cells` need not be the same length — is infinitely
/// far, so it only wins when nothing else matches.
fn ordinate_distance(ordinates: &[f32], index: usize, position: f32) -> f32 {
    ordinates
        .get(index)
        .map(|ordinate| (ordinate - position).abs())
        .unwrap_or(f32::INFINITY)
}

/// The foot-of-page list for the destinations of `page` that reached no text,
/// or `None` when the page has none.
///
/// The list exists so no destination is lost, so it carries destinations, not
/// annotations: one entry per URL the page's Markdown does not already point
/// at. A page routinely hangs the same URL on several rectangles — the logo,
/// the wordmark under it, the "read more" at the end of the paragraph — and
/// listing that URL once per rectangle would repeat it without telling the
/// reader anything the first entry did not.
///
/// A destination anchored inline anywhere on the page is left out for the same
/// reason: it already reached the Markdown, where it sits on the words it
/// decorates instead of at the foot of the page.
///
/// `images` are the page's image placeholders; a link sitting on one is listed
/// with it, because the clickable logo is the common shape of an unanchored
/// link and naming the image is what tells a consumer the URL is not a lost
/// anchor.
pub(crate) fn page_link_list(
    page: u32,
    links: &[MarkdownLink],
    images: &HashMap<u32, Vec<PageImage>>,
) -> Option<String> {
    let anchored: HashSet<&str> = links
        .iter()
        .filter(|link| link.page == page && link.anchored_inline)
        .map(|link| link.url.as_str())
        .collect();
    let mut listed: HashSet<&str> = HashSet::new();
    let unanchored: Vec<&MarkdownLink> = links
        .iter()
        .filter(|link| {
            link.page == page
                && !link.anchored_inline
                && crate::types::destination_allowed_in_markdown(&link.url)
                && !anchored.contains(link.url.as_str())
                && listed.insert(link.url.as_str())
        })
        .collect();
    if unanchored.is_empty() {
        return None;
    }
    let empty = Vec::new();
    let page_images = images.get(&page).unwrap_or(&empty);
    let mut list = String::from(LINK_LIST_HEADING);
    list.push('\n');
    for link in unanchored {
        list.push_str("\n- <");
        list.push_str(&link.url);
        list.push('>');
        if let Some(image) = covering_image(link.rect, page_images) {
            list.push_str(" on ![Image: ");
            list.push_str(&image.name);
            list.push_str("](image)");
        }
    }
    list.push('\n');
    Some(list)
}

/// An image placeholder of a page: where it sits and the name the Markdown
/// image emitter would give it.
#[derive(Debug, Clone)]
pub(crate) struct PageImage {
    pub(crate) x0: f32,
    pub(crate) y0: f32,
    pub(crate) x1: f32,
    pub(crate) y1: f32,
    pub(crate) name: String,
}

/// The Markdown without the per-page link lists.
///
/// Whether a page needs OCR is judged on the text extraction produced — how
/// empty it is, how much of it is garbage. A list of destinations is neither:
/// it is clean ASCII the annotations supplied, and letting it into that
/// judgement would keep a page out of OCR because its links looked healthy.
pub(crate) fn without_link_lists(markdown: &str) -> String {
    let mut kept: Vec<&str> = Vec::new();
    let mut in_list = false;
    for line in markdown.lines() {
        let trimmed = line.trim();
        if trimmed == LINK_LIST_HEADING {
            in_list = true;
            continue;
        }
        if in_list {
            if trimmed.is_empty() || trimmed.starts_with("- <") {
                continue;
            }
            in_list = false;
        }
        kept.push(line);
    }
    kept.join("\n")
}

/// The image under a link rectangle, if the rectangle's centre falls inside
/// one. The centre, rather than any overlap, because a link laid over a chart
/// often extends past the artwork's own box.
fn covering_image(rect: (f32, f32, f32, f32), images: &[PageImage]) -> Option<&PageImage> {
    let (x, y, width, height) = rect;
    let centre_x = x + width / 2.0;
    let centre_y = y + height / 2.0;
    images.iter().find(|image| {
        centre_x >= image.x0 && centre_x <= image.x1 && centre_y >= image.y0 && centre_y <= image.y1
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tables::TableKind;

    /// A 12pt span whose glyphs occupy `width` points from `x` on `page`.
    fn span(page: u32, text: &str, x: f32, y: f32, width: f32) -> TextItem {
        TextItem {
            text: text.to_string(),
            x,
            y,
            width,
            height: 12.0,
            font: "F1".to_string(),
            font_tag: String::new(),
            font_size: 12.0,
            page,
            is_bold: false,
            is_italic: false,
            is_underline: false,
            is_strikeout: false,
            item_type: ItemType::Text,
            mcid: None,
            baseline_shift: 0.0,
        }
    }

    /// A `/Link /URI` annotation as `extractor::links` reports it.
    fn link(page: u32, url: &str, x: f32, y: f32, width: f32) -> TextItem {
        let mut item = span(page, url, x, y - 3.0, width);
        item.height = 18.0;
        item.font_size = 0.0;
        item.item_type = ItemType::Link(url.to_string());
        item
    }

    fn line(page: u32, items: Vec<TextItem>) -> TextLine {
        TextLine {
            y: items.first().map(|item| item.y).unwrap_or(0.0),
            items,
            page,
            adaptive_threshold: 0.1,
        }
    }

    #[test]
    fn a_link_over_prose_reports_the_words_it_covers() {
        let text = span(1, "Fonte: Statista, 2024", 10.0, 100.0, 105.0);
        let links = vec![link(1, "https://statista.com", 45.0, 100.0, 70.0)];
        let anchors = anchors_by_page(&links);
        let texts = anchor_texts(std::iter::once(&text), &anchors);
        let lines = vec![line(1, vec![text])];

        let resolved = resolve(&anchors, &lines, &texts, &HashMap::new());

        assert_eq!(
            resolved,
            vec![MarkdownLink {
                url: "https://statista.com".to_string(),
                rect: (45.0, 97.0, 70.0, 18.0),
                page: 1,
                anchor: Some("Statista, 2024".to_string()),
                anchored_inline: true,
            }]
        );
    }

    #[test]
    fn a_link_over_a_superscript_run_is_reported_unanchored() {
        // Line rendering emits a super/subscript run unlinked, so counting it
        // as anchored would credit the annotation with an anchor nothing
        // carries — and keep its URL out of the page's list as well.
        let mut marker = span(1, "1", 10.0, 100.0, 6.0);
        marker.baseline_shift = 4.0;
        let links = vec![link(1, "https://doi.org/10.1000/x", 8.0, 100.0, 10.0)];
        let anchors = anchors_by_page(&links);
        let texts = anchor_texts(std::iter::once(&marker), &anchors);
        assert!(texts.is_empty(), "a script run yields no anchor text");

        let lines = vec![line(1, vec![marker])];
        let resolved = resolve(&anchors, &lines, &texts, &HashMap::new());

        assert_eq!(resolved[0].anchor, None);
        assert!(!resolved[0].anchored_inline);
        assert_eq!(
            page_link_list(1, &resolved, &HashMap::new()),
            Some("**Links on this page**\n\n- <https://doi.org/10.1000/x>\n".to_string())
        );
    }

    #[test]
    fn a_link_over_no_text_is_reported_unanchored_and_listed() {
        let links = vec![link(1, "https://example.com", 400.0, 400.0, 80.0)];
        let anchors = anchors_by_page(&links);
        let resolved = resolve(&anchors, &[], &HashMap::new(), &HashMap::new());

        assert_eq!(resolved[0].anchor, None);
        assert!(!resolved[0].anchored_inline);

        let images = HashMap::from([(
            1,
            vec![PageImage {
                x0: 380.0,
                y0: 380.0,
                x1: 520.0,
                y1: 460.0,
                name: "Image21".to_string(),
            }],
        )]);
        assert_eq!(
            page_link_list(1, &resolved, &images),
            Some(
                "**Links on this page**\n\n- <https://example.com> on ![Image: Image21](image)\n"
                    .to_string()
            )
        );
    }

    #[test]
    fn a_link_off_every_image_is_listed_without_one() {
        let links = vec![link(1, "https://example.com", 400.0, 400.0, 80.0)];
        let anchors = anchors_by_page(&links);
        let resolved = resolve(&anchors, &[], &HashMap::new(), &HashMap::new());

        assert_eq!(
            page_link_list(1, &resolved, &HashMap::new()),
            Some("**Links on this page**\n\n- <https://example.com>\n".to_string())
        );
    }

    #[test]
    fn the_list_carries_each_destination_once_and_skips_the_anchored_ones() {
        // Four rectangles, two destinations. The shop URL is anchored on the
        // wordmark, so the logo rectangle that repeats it adds nothing; the
        // press URL is on two images and belongs in the list exactly once.
        let wordmark = span(1, "Example", 10.0, 100.0, 24.0);
        let links = vec![
            link(1, "https://example.com/shop", 8.0, 100.0, 28.0),
            link(1, "https://example.com/shop", 400.0, 400.0, 80.0),
            link(1, "https://example.com/press", 400.0, 300.0, 80.0),
            link(1, "https://example.com/press", 400.0, 200.0, 80.0),
        ];
        let anchors = anchors_by_page(&links);
        let texts = anchor_texts(std::iter::once(&wordmark), &anchors);
        let lines = vec![line(1, vec![wordmark])];

        let resolved = resolve(&anchors, &lines, &texts, &HashMap::new());

        assert_eq!(
            resolved
                .iter()
                .map(|link| link.anchored_inline)
                .collect::<Vec<_>>(),
            vec![true, false, false, false]
        );
        assert_eq!(
            page_link_list(1, &resolved, &HashMap::new()),
            Some("**Links on this page**\n\n- <https://example.com/press>\n".to_string())
        );
    }

    #[test]
    fn a_destination_the_reader_cannot_follow_is_not_listed_either() {
        let links = vec![
            link(1, "javascript:void(0)", 400.0, 400.0, 80.0),
            link(1, "#", 400.0, 300.0, 80.0),
        ];
        let anchors = anchors_by_page(&links);
        let resolved = resolve(&anchors, &[], &HashMap::new(), &HashMap::new());

        assert_eq!(resolved.len(), 2, "both annotations are still reported");
        assert_eq!(page_link_list(1, &resolved, &HashMap::new()), None);
    }

    #[test]
    fn a_page_whose_links_are_all_anchored_gets_no_list() {
        let text = span(1, "Statista", 10.0, 100.0, 48.0);
        let links = vec![link(1, "https://statista.com", 8.0, 100.0, 52.0)];
        let anchors = anchors_by_page(&links);
        let texts = anchor_texts(std::iter::once(&text), &anchors);
        let lines = vec![line(1, vec![text])];
        let resolved = resolve(&anchors, &lines, &texts, &HashMap::new());

        assert_eq!(page_link_list(1, &resolved, &HashMap::new()), None);
    }

    #[test]
    fn the_same_url_twice_on_a_page_stays_two_records() {
        let first = span(1, "shop", 10.0, 200.0, 24.0);
        let second = span(1, "here", 10.0, 100.0, 24.0);
        let links = vec![
            link(1, "https://example.com", 8.0, 200.0, 28.0),
            link(1, "https://example.com", 8.0, 100.0, 28.0),
        ];
        let anchors = anchors_by_page(&links);
        let texts = anchor_texts([&first, &second].into_iter(), &anchors);
        let lines = vec![line(1, vec![first]), line(1, vec![second])];

        let resolved = resolve(&anchors, &lines, &texts, &HashMap::new());

        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].anchor, Some("shop".to_string()));
        assert_eq!(resolved[1].anchor, Some("here".to_string()));
    }

    /// A three-column table whose ordinates are the detector's clusters.
    fn table(cells: Vec<Vec<String>>) -> Table {
        Table {
            columns: vec![100.0, 300.0, 500.0],
            rows: vec![400.0, 300.0, 200.0],
            cells,
            item_indices: Vec::new(),
            kind: TableKind::Data,
        }
    }

    #[test]
    fn a_link_over_a_table_cell_wraps_that_cell() {
        // The rectangles sit just short of their column's ordinate, as they
        // do in a real document: a detected table's `columns` are the
        // clusters its items were grouped around, not cell edges, so the
        // nearest one is the cell — the last one before it is the cell to
        // its left, which here is empty or holds another value.
        let table = table(vec![
            vec!["Nome".into(), "Sito".into(), "Social".into()],
            vec!["Bravo".into(), "si".into(), "si".into()],
        ]);
        let links = vec![
            link(1, "https://example.org/", 288.0, 300.0, 14.0),
            link(1, "https://instagram.com/bravo", 488.0, 300.0, 14.0),
        ];
        let anchors = anchors_by_page(&links);
        let texts = HashMap::from([((1, 0), "si".to_string()), ((1, 1), "si".to_string())]);
        let mut claimed = HashMap::new();

        let anchored =
            anchor_table_cells(1, &table, &anchors[&1], &texts, &mut claimed).expect("anchored");

        assert_eq!(
            anchored.cells[1],
            vec![
                "Bravo".to_string(),
                "[si](https://example.org/)".to_string(),
                "[si](https://instagram.com/bravo)".to_string(),
            ]
        );
        assert_eq!(claimed.len(), 2);
    }

    #[test]
    fn two_links_in_one_cell_splice_beside_each_other() {
        let table = table(vec![vec!["Northwind Delta Works".into()]]);
        let links = vec![
            link(1, "https://northwind.example", 95.0, 400.0, 60.0),
            link(1, "https://deltaworks.example", 95.0, 400.0, 60.0),
        ];
        let anchors = anchors_by_page(&links);
        let texts = HashMap::from([
            ((1, 0), "Northwind".to_string()),
            ((1, 1), "Delta Works".to_string()),
        ]);
        let mut claimed = HashMap::new();

        let anchored =
            anchor_table_cells(1, &table, &anchors[&1], &texts, &mut claimed).expect("anchored");

        assert_eq!(
            anchored.cells[0][0],
            "[Northwind](https://northwind.example) [Delta Works](https://deltaworks.example)"
        );
    }

    #[test]
    fn two_links_on_the_same_words_of_one_cell_do_not_nest() {
        // Both annotations read "si" out of the same cell. The second must
        // splice onto the second occurrence, not into the first link's
        // brackets.
        let table = table(vec![vec!["Nome".into(), "si si".into()]]);
        let links = vec![
            link(1, "https://sito-a.it", 292.0, 400.0, 12.0),
            link(1, "https://sito-b.it", 306.0, 400.0, 12.0),
        ];
        let anchors = anchors_by_page(&links);
        let texts = HashMap::from([((1, 0), "si".to_string()), ((1, 1), "si".to_string())]);
        let mut claimed = HashMap::new();

        let anchored =
            anchor_table_cells(1, &table, &anchors[&1], &texts, &mut claimed).expect("anchored");

        assert_eq!(
            anchored.cells[0][1],
            "[si](https://sito-a.it) [si](https://sito-b.it)"
        );
    }

    #[test]
    fn a_multibyte_cell_anchor_is_spliced_on_the_right_boundary() {
        // The cell carries two-byte characters before and inside the anchor.
        let table = table(vec![vec!["Città".into(), "Società è qui".into()]]);
        let links = vec![link(1, "https://societa.example", 292.0, 400.0, 40.0)];
        let anchors = anchors_by_page(&links);
        let texts = HashMap::from([((1, 0), "Società".to_string())]);
        let mut claimed = HashMap::new();

        let anchored =
            anchor_table_cells(1, &table, &anchors[&1], &texts, &mut claimed).expect("anchored");

        assert_eq!(
            anchored.cells[0][1],
            "[Società](https://societa.example) è qui"
        );
    }

    #[test]
    fn a_link_far_from_the_table_does_not_anchor_a_cell_that_happens_to_match() {
        // The rectangle sits in the prose at the top of the page; the word
        // under it also appears in a table cell far below. Only the text
        // matched, and text alone must not be enough.
        let table = table(vec![vec!["Nome".into(), "Roma".into()]]);
        let links = vec![link(1, "https://roma.it", 292.0, 900.0, 40.0)];
        let anchors = anchors_by_page(&links);
        let texts = HashMap::from([((1, 0), "Roma".to_string())]);
        let mut claimed = HashMap::new();

        assert!(anchor_table_cells(1, &table, &anchors[&1], &texts, &mut claimed).is_none());
        assert!(claimed.is_empty());
    }

    #[test]
    fn a_link_whose_text_is_in_no_cell_of_its_column_anchors_nothing() {
        let table = table(vec![vec!["Nome".into(), "si".into(), "si".into()]]);
        let links = vec![link(1, "https://example.com", 295.0, 400.0, 14.0)];
        let anchors = anchors_by_page(&links);
        let texts = HashMap::from([((1, 0), "assente".to_string())]);
        let mut claimed = HashMap::new();

        assert!(anchor_table_cells(1, &table, &anchors[&1], &texts, &mut claimed).is_none());
        assert!(claimed.is_empty());
    }

    #[test]
    fn an_occurrence_inside_an_existing_link_is_skipped() {
        assert_eq!(find_outside_links("[si](https://a.it) si", "si"), Some(19));
        assert_eq!(find_outside_links("si [si](https://a.it)", "si"), Some(0));
        assert_eq!(find_outside_links("[si](https://a.it)", "si"), None);
        assert_eq!(find_outside_links("plain si", "si"), Some(6));
    }

    #[test]
    fn a_bracket_that_opens_no_link_does_not_hide_the_anchor() {
        // A cell carrying a bibliography reference must not lose its anchor:
        // `[3]` opens nothing, so everything after it is still outside a link.
        assert_eq!(find_outside_links("see [3] si [x](u)", "si"), Some(8));
        assert_eq!(find_outside_links("see 3 si [x](u)", "si"), Some(6));
        // The bracket after the last link opens nothing either.
        assert_eq!(find_outside_links("[x](u) si [3]", "si"), Some(7));
        // And a real link still hides what is inside it.
        assert_eq!(find_outside_links("see [3] [si](u)", "si"), None);
    }

    #[test]
    fn a_bracket_escaped_inside_an_anchor_does_not_open_a_link() {
        // `inline_link` writes a bracket of the anchor as `\[`, so the link's
        // text starts at the outer bracket and covers the escaped one. A
        // second annotation must not read the words before it as free.
        let cell = "[Models \\[3\\] here](https://a.it/one)";
        assert_eq!(find_outside_links(cell, "Models"), None);
        assert_eq!(find_outside_links(cell, "here"), None);
        assert_eq!(
            find_outside_links(&format!("{cell} Models"), "Models"),
            Some(cell.len() + 1)
        );
    }
}
