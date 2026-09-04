//! Shared types used across the extraction and markdown pipelines.
//!
//! Centralises `TextItem`, `TextLine`, `PdfRect`, font-width / encoding
//! type aliases, and the `ItemType` enum so that every module can import
//! them from one place.

use std::collections::HashMap;

use crate::text_utils::should_join_items;

/// Result tuple returned by page-level text extraction: text items, rectangles, line segments,
/// and whether fonts with unresolvable gid-encoded glyphs were encountered.
pub(crate) type PageExtraction = (Vec<TextItem>, Vec<PdfRect>, Vec<PdfLine>);

// ── Font types (crate-internal) ──────────────────────────────────────

/// Font encoding map: maps byte codes to Unicode characters
pub(crate) type FontEncodingMap = HashMap<u8, char>;

/// All font encodings for a page
pub(crate) type PageFontEncodings = HashMap<String, FontEncodingMap>;

/// Font width information extracted from PDF font dictionaries
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct FontWidthInfo {
    /// Glyph widths: maps character code to width in font units
    pub(crate) widths: HashMap<u16, u16>,
    /// Default width for glyphs not in the widths table
    pub(crate) default_width: u16,
    /// Width of the space character (code 32) if known
    pub(crate) space_width: u16,
    /// Whether this is a CID font (2-byte character codes)
    pub(crate) is_cid: bool,
    /// Scale factor to convert font units to text space units.
    /// For Type1/TrueType: 0.001 (widths in 1000ths of em)
    /// For Type3: FontMatrix[0] (e.g., 0.00048828125 for 2048-unit grid)
    pub(crate) units_scale: f32,
    /// Writing mode: 0 = horizontal (default), 1 = vertical
    pub(crate) wmode: u8,
}

/// All font width info for a page, keyed by font resource name
pub(crate) type PageFontWidths = HashMap<String, FontWidthInfo>;

// ── Public types ─────────────────────────────────────────────────────

/// Type of extracted item
#[derive(Debug, Clone, Default)]
pub enum ItemType {
    /// Regular text content
    #[default]
    Text,
    /// Image placeholder
    Image,
    /// Hyperlink (with URL)
    Link(String),
    /// Form field (name: value)
    FormField,
}

/// Layout complexity analysis result.
///
/// Callers can use this to decide whether the extracted markdown is reliable
/// or whether the PDF should be routed to an OCR pipeline instead.
#[derive(Debug, Clone, Default)]
pub struct LayoutComplexity {
    /// True if any page has tables or multi-column text.
    pub is_complex: bool,
    /// 1-indexed pages where table borders were detected (rect count > 6).
    pub pages_with_tables: Vec<u32>,
    /// 1-indexed pages where 2+ text columns were detected.
    pub pages_with_columns: Vec<u32>,
}

/// A line segment from PDF path operators (`m`/`l`/`S`).
#[derive(Debug, Clone)]
pub struct PdfLine {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub page: u32,
}

/// A rectangle from a PDF `re` operator (cell boundary, border, etc.)
#[derive(Debug, Clone)]
pub struct PdfRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub page: u32,
}

/// A text item with position information.
///
/// `x`, `y`, `width`, `height` describe the item's axis-aligned box in PDF
/// points, y-up. For ordinary horizontal text the box runs from the baseline
/// one em upward and spans the run's advance, which is what every consumer
/// historically assumed. A run shown with a rotated text matrix gets the
/// bounding box of its rotated glyph run instead — tall and thin for a
/// vertical margin stamp — and reports the angle in `rotation`.
///
/// # Coordinate frame
///
/// Items returned by the public position APIs (`extract_text_with_positions*`)
/// are relative to the page's **visible page box** —
/// `CropBox ∩ MediaBox` when the page has a CropBox, else the MediaBox; a
/// CropBox that does not overlap the MediaBox is ignored, and a page without
/// a MediaBox is measured against US Letter (see `extractor::page_box`) —
/// with the box's lower-left corner as the origin and `y` growing upward.
/// A renderer's page image and the region APIs use the same box from its
/// top-left corner with `y` growing downward, so flipping `y` by the box
/// height lets items and rendered regions be intersected directly. Raw
/// content-stream coordinates differ whenever the CropBox or MediaBox origin
/// is not `(0, 0)`. A page whose text is predominantly rotated has its frame
/// turned so the text reads left-to-right (`PageRotation`, reported by
/// `extract_text_with_positions_and_rotations_mem`) and the shift is turned
/// the same way; `/Rotate` is not applied. Inside the markdown pipeline
/// items stay in raw user space.
#[derive(Debug, Clone)]
pub struct TextItem {
    /// The text content
    pub text: String,
    /// Left edge of the item's box, in PDF points from the visible page
    /// box's left edge (see the coordinate frame note on [`TextItem`]).
    pub x: f32,
    /// Bottom edge of the item's box, in PDF points from the visible page
    /// box's bottom edge with `y` growing upward (see the coordinate frame
    /// note on [`TextItem`]). For horizontal text this is the baseline;
    /// descenders are not included. Image, link and form-field items carry
    /// their rect's bottom edge.
    pub y: f32,
    /// Horizontal extent of the box: the advance for horizontal text, the
    /// em size for a vertical run. Zero only for a horizontal run whose font
    /// carries no width information (advance unknown).
    pub width: f32,
    /// Vertical extent of the box: the rendered em size for horizontal
    /// text, the advance for a vertical run.
    pub height: f32,
    /// Rotation of the run's baseline in degrees, counter-clockwise from the
    /// page's +x axis, normalised to `[0, 360)`: `0` for ordinary
    /// left-to-right text, `90` for text reading bottom-to-top (a margin
    /// stamp rotated counter-clockwise), `270` for text reading
    /// top-to-bottom, `180` for upside-down text. Rotation-only matrices
    /// report exact multiples of 90; skewed matrices (deskewed OCR layers,
    /// diagonal watermarks) report fractional angles. A reflected text
    /// matrix has no rotation — its reading direction and its glyphs'
    /// orientation differ by a half turn — and reports how its glyphs
    /// stand: `0` for the mirrored-x matrix some producers paint
    /// right-to-left text with (upright glyphs reading left), `180` for a
    /// y-flipped one. A negative `Tf` size turns a run around and reads as
    /// `180`. `0` for items that don't come from a text matrix (images,
    /// links, form fields, OCR).
    /// On a page whose text is predominantly rotated the extractor turns
    /// the coordinate frame so the dominant runs read as `0`; upright
    /// strays then report `270` on a counter-clockwise page and `90` on a
    /// clockwise one.
    pub rotation: f32,
    /// Whether the run's advance came from font metrics. `false` when the
    /// font carries no width information (or, for an ActualText span, when
    /// the advance could not be recovered from the text matrix): the box's
    /// extent along the baseline is then an estimate of half an em per
    /// painted glyph (an ActualText span counts the glyphs it covers, not its
    /// replacement text), laid in the direction the run reads, rather than
    /// a measurement. A
    /// font that reports a genuine zero advance keeps `true`. Items that
    /// don't come from a text matrix (images, links, form fields, OCR)
    /// always report `true`.
    pub advance_known: bool,
    /// Font name: the `/BaseFont` family name ("ABCDEF+CMMI10"), which
    /// identifies the actual face (see `extractor::fonts::item_font_name`
    /// for the CID carve-out).
    pub font: String,
    /// The raw font resource tag ("F2", "T22") the item's show operator
    /// selected. This is exactly what `font` carried before 1.16.0, with
    /// the same caveats: the tag's namespace is the enclosing page or Form
    /// XObject's `/Resources` (the same tag on another page may name a
    /// different face), and an item merged from multiple runs keeps the
    /// first run's tag. Within one page it distinguishes font *programs*
    /// that share a family name (two subsets of the same face keep
    /// distinct tags), which family-keyed `font` cannot. Empty for items
    /// that don't originate from a content-stream show operator (images,
    /// links, form fields, OCR).
    pub font_tag: String,
    /// Font size
    pub font_size: f32,
    /// Page number (1-indexed)
    pub page: u32,
    /// Whether the font is bold
    pub is_bold: bool,
    /// Whether the font is italic
    pub is_italic: bool,
    /// Whether the text is underlined (drawn rule/thin rect under the
    /// baseline — PDFs have no underline font flag, so this is detected
    /// geometrically after extraction; see `extractor::underline`).
    pub is_underline: bool,
    /// Whether the text is struck out (drawn rule/thin rect crossing the
    /// glyphs at mid x-height). Same geometric detection as underline,
    /// different vertical window; see `extractor::underline`.
    pub is_strikeout: bool,
    /// Type of item (text, image, link)
    pub item_type: ItemType,
    /// Marked Content ID from the content stream's BDC/BMC operator.
    /// Used to link this item to the PDF structure tree for tagged PDFs.
    pub mcid: Option<i64>,
    /// Signed baseline offset, in points, of a superscript/subscript glyph
    /// run from the baseline of the body text it is attached to. Zero for
    /// normal text. Positive = raised above the anchor's baseline
    /// (superscript: footnote and affiliation markers, exponents), negative =
    /// lowered (subscript: chemistry indices, math). Extraction sets it when
    /// a short run is small relative to a tightly adjacent larger neighbor
    /// and sits at a real baseline offset from it; a digit-only run beside a
    /// word is instead fused into that word as Unicode super/subscript
    /// characters ("H₂O", "word²") and never carries a shift. `y` stays the
    /// glyph's own baseline; [`TextItem::line_y`] gives the anchor's.
    pub baseline_shift: f32,
}

impl TextItem {
    /// Baseline of the visual line this item belongs to: the glyphs'
    /// baseline for normal text, the anchor's baseline for a super/subscript
    /// glyph run (`baseline_shift` below it). Line grouping compares this
    /// instead of `y` so raised and lowered markers stay on their body line
    /// and upside-down runs of different sizes share theirs.
    pub fn line_y(&self) -> f32 {
        self.baseline_y() - self.baseline_shift
    }

    /// The y of the edge the glyphs stand on or hang from: `y` (the box's
    /// bottom edge) for a run within 45° of upright, `y + height` (the top
    /// edge) for one within 45° of upside-down (`is_upside_down()`, glyph
    /// orientation included for reflected matrices). Exact for level runs;
    /// for oblique ones the baseline is not horizontal and the edge is only
    /// an approximation of it, which is what line grouping then compares.
    /// Vertical runs return the box bottom `y`.
    pub fn baseline_y(&self) -> f32 {
        if self.is_upside_down() {
            self.y + self.height
        } else {
            self.y
        }
    }

    /// `true` for a glyph run flagged as a super- or subscript of a larger
    /// neighbor (non-zero `baseline_shift`).
    pub fn is_script(&self) -> bool {
        self.baseline_shift != 0.0
    }
}

impl TextItem {
    /// Whether the run reads along the page's x axis rather than its y
    /// axis: `rotation` closer to `0`/`180` than to `90`/`270`, the same
    /// 45° split the extractor uses to vote on page rotation. Layout
    /// heuristics that reason about baselines, word gaps, and column spans
    /// walk the x axis and assume this; rotated runs (margin stamps, chart
    /// axis titles, rotated table headers) return `false` and are kept out
    /// of them. Oblique runs (diagonal watermarks, deskewed OCR lines) are
    /// deliberately still `true`: the x-axis heuristics are the closest fit
    /// the pipeline has for them, exactly as before `rotation` existed, and
    /// callers needing the precise angle read `rotation` directly.
    pub fn is_horizontal(&self) -> bool {
        let r = self.rotation.rem_euclid(360.0);
        let vertical = (r > 45.0 && r < 135.0) || (r > 225.0 && r < 315.0);
        !vertical
    }

    /// Whether the run reads along +x: `rotation` within 45° of `0`. The
    /// x-ascending walks (item merging, line assembly) assume this; an
    /// upside-down run is `is_horizontal()` but reads towards -x.
    pub fn is_upright(&self) -> bool {
        let r = self.rotation.rem_euclid(360.0);
        r <= 45.0 || r >= 315.0
    }

    /// Whether the run reads towards -x: `rotation` within 45° of `180`.
    pub fn is_upside_down(&self) -> bool {
        self.is_horizontal() && !self.is_upright()
    }

    /// The item's extent perpendicular to its reading direction: `height`
    /// for an unrotated item — identical to the historical value, and the
    /// only meaningful extent for image, link, and OCR boxes — and the
    /// rendered em (`font_size`) for any rotated run, whose axis-aligned
    /// box mixes the advance into both dimensions (a long diagonal run is
    /// not a tall line). Only content-stream runs carry a non-zero
    /// `rotation`, and they set `font_size` to exactly the em height the
    /// unrotated case reports.
    pub(crate) fn cross_extent(&self) -> f32 {
        if self.rotation == 0.0 {
            self.height
        } else {
            self.font_size
        }
    }
}

/// A line of text (grouped text items)
#[derive(Debug, Clone)]
pub struct TextLine {
    pub items: Vec<TextItem>,
    pub y: f32,
    pub page: u32,
    /// Adaptive join threshold from page-level letter-spacing detection.
    /// Default 0.10 for normal PDFs; higher for Canva-style PDFs.
    #[doc(hidden)]
    pub adaptive_threshold: f32,
}

/// Gap, as a fraction of the larger font size, from which a script glyph and
/// its normal-sized neighbor are separate words. Attached markers sit at
/// ~0 gap (kerned ones slightly negative); a word space is ≥ 0.2 em.
const SCRIPT_WORD_GAP: f32 = 0.12;
/// Spacing at the edge of a super/subscript run — the single policy shared
/// by line rendering (`TextLine::text`) and table-cell joining. `None` when
/// neither item is a script run, so the caller's ordinary rules apply.
///
/// A run's glyphs arrive pre-joined by extraction, so only the boundary
/// between a run and its neighbor is decided here, by the measured gap: a
/// footnote marker hugs the word before it ("word<sup>1</sup>"), a leading
/// affiliation marker hugs the word after it ("<sup>1,2</sup>Hong Kong"),
/// and a word space after a marker survives ("<sup>2</sup> next"). Existing
/// whitespace, hyphen junctions, open brackets before and closing
/// punctuation after a run never take a space.
pub(crate) fn script_edge_needs_space(
    prev: &TextItem,
    item: &TextItem,
    result: &str,
    text: &str,
) -> Option<bool> {
    if !(prev.is_script() || item.is_script()) {
        return None;
    }
    if stacked_fraction_slash(prev, item) {
        return Some(false);
    }
    // Different visual lines (a wrapped table cell): a run at the end of
    // one line never attaches to the start of the next, whatever the x
    // overlap says.
    if (prev.line_y() - item.line_y()).abs() > prev.font_size.max(item.font_size) * 0.5 {
        return Some(true);
    }
    let curr = text.trim_start();
    // `result` ends with the closing tag when the previous item is a run,
    // so its raw text is inspected too for hyphens and open brackets.
    if result.ends_with([' ', '-', '(', '[', '{'])
        || prev.text.ends_with(' ')
        || prev.text.trim_end().ends_with(['-', '(', '[', '{'])
        || text.starts_with(' ')
        || curr.starts_with('-')
        || curr
            .chars()
            .next()
            .is_some_and(|c| matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}'))
    {
        return Some(false);
    }
    let gap = if prev.x <= item.x {
        item.x - (prev.x + prev.width)
    } else {
        prev.x - (item.x + item.width)
    };
    Some(gap >= prev.font_size.max(item.font_size) * SCRIPT_WORD_GAP)
}

/// A stacked case fraction: a digit-only superscript run directly followed
/// by a digit-only subscript run that overlaps it horizontally — the
/// numerator over the denominator, as TeX sets "3⅓". Rendered with a slash
/// between the runs (`3 <sup>1</sup>/<sub>3</sub>`) instead of the runs
/// being glued into one number.
pub(crate) fn stacked_fraction_slash(prev: &TextItem, item: &TextItem) -> bool {
    let digits = |t: &str| !t.is_empty() && t.chars().all(char::is_numeric);
    prev.baseline_shift > 0.0
        && item.baseline_shift < 0.0
        && (prev.line_y() - item.line_y()).abs() <= prev.font_size.max(item.font_size) * 0.5
        && digits(prev.text.trim())
        && digits(item.text.trim())
        && item.x < prev.x + prev.width
        && prev.x < item.x + item.width
}

/// Share of a text span's own height that must lie inside a link rectangle
/// before the span counts as decorated by that link.
///
/// Normalising on the span, not on the rectangle, is what makes the test
/// robust: a `/Rect` is nearly always more generous than the line it
/// decorates (vertical padding, sometimes two lines), so a ratio taken on the
/// rectangle's area would be arbitrarily small for a correct match.
///
/// The value is a plateau, not a tuned constant. Measured over the 360
/// `/Link /URI` annotations of a 205-page real document, the number of links
/// with at least one qualifying span is 334 at *every* threshold from 0.3 to
/// 0.8: no span in that corpus overlaps a rectangle by an ambiguous fraction
/// of its height. 0.5 sits in the middle of that empty valley. (Contrast the
/// area ratio the Python prototype used, which is not flat at all over the
/// same corpus: 334 links at 0.05, 328 at 0.20, 323 at 0.30, 306 at 0.50.)
const ANCHOR_VERTICAL_OVERLAP: f32 = 0.5;

/// A `/Link /URI` annotation reduced to what anchoring needs: where it sits
/// and where it points.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LinkAnchor {
    /// Destination URI of the annotation's `/A` action.
    pub(crate) url: String,
    /// Left edge of the annotation's `/Rect`, in the same frame as
    /// [`TextItem::x`].
    pub(crate) x: f32,
    /// Bottom edge of the annotation's `/Rect`.
    pub(crate) y: f32,
    /// Width of the annotation's `/Rect`.
    pub(crate) width: f32,
    /// Height of the annotation's `/Rect`.
    pub(crate) height: f32,
    /// `true` when the text under the whole rectangle spells this very URL,
    /// so the reader already sees the destination.
    ///
    /// Decided over the annotation's complete anchor, not over one run of it:
    /// a producer breaks a long URL across two lines, and each half on its own
    /// looks like ordinary anchor text. See
    /// [`PageLinkAnchors::mark_urls_written_out`].
    pub(crate) writes_out_url: bool,
}

impl LinkAnchor {
    /// Middle of the rectangle.
    pub(crate) fn centre(&self) -> (f32, f32) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// Share of `item`'s height that lies inside this rectangle.
    fn vertical_overlap(&self, item: &TextItem) -> f32 {
        if item.height <= 0.0 {
            return 0.0;
        }
        let overlap = (self.y + self.height).min(item.y + item.height) - self.y.max(item.y);
        overlap.max(0.0) / item.height
    }

    /// Character range of `text` this rectangle covers, snapped to the nearest
    /// word boundary at each end, or `None` when the rectangle covers none of
    /// the span.
    ///
    /// A producer routinely merges a whole visual line into one span
    /// ("Fonte: Linkedin. Jordan Blake. Medium.") and then hangs a separate
    /// annotation on each name in it. Taking the whole span as the anchor
    /// would give all three links the same wrong text and let only one of them
    /// be emitted, so the span is cut where the rectangle cuts it.
    ///
    /// The cut assumes a uniform advance across the span, which is only
    /// approximate for a proportional face; snapping each end to the nearest
    /// word boundary absorbs the error, since a boundary is many characters
    /// away while the estimate is off by one or two.
    fn char_range(&self, item: &TextItem) -> Option<(usize, usize)> {
        if item.width <= 0.0 || self.vertical_overlap(item) < ANCHOR_VERTICAL_OVERLAP {
            return None;
        }
        let start_x = self.x.max(item.x);
        let end_x = (self.x + self.width).min(item.x + item.width);
        if end_x <= start_x {
            return None;
        }
        let chars: Vec<char> = item.text.chars().collect();
        let count = chars.len();
        if count == 0 {
            return None;
        }
        let advance = item.width / count as f32;
        let start = nearest_word_boundary(&chars, (start_x - item.x) / advance);
        let end = nearest_word_boundary(&chars, (end_x - item.x) / advance);
        if start >= end || !chars[start..end].iter().any(|c| c.is_alphanumeric()) {
            // Snapping the right edge to the *start* of the following word
            // hands back the stray characters between the two: the full stop
            // after "…@cisecurity.org", the comma between two linked names.
            // `[.](mailto:…)` is not an anchor anyone reads, and a rectangle
            // whose only claim on a span is punctuation has not found its
            // text there — it either covers the real words in another span or
            // belongs in the page's link list.
            return None;
        }
        Some((start, end))
    }
}

/// Index of the word boundary closest to `position`, measured in characters
/// from the start of `chars`. Boundaries are the two ends plus the first
/// character of every word.
fn nearest_word_boundary(chars: &[char], position: f32) -> usize {
    let mut best = 0usize;
    let mut best_distance = position.abs();
    let mut consider = |index: usize| {
        let distance = (index as f32 - position).abs();
        if distance < best_distance {
            best = index;
            best_distance = distance;
        }
    };
    for index in 1..chars.len() {
        if chars[index - 1].is_whitespace() && !chars[index].is_whitespace() {
            consider(index);
        }
    }
    consider(chars.len());
    best
}

/// The link annotations of one page, queried per text item while a line is
/// rendered.
///
/// Empty for every page of a document without hyperlinks, which is the case
/// the lookup short-circuits on.
#[derive(Debug, Default, Clone)]
pub(crate) struct PageLinkAnchors {
    anchors: Vec<LinkAnchor>,
}

/// One link's claim on a stretch of a text item: the character range of
/// `TextItem::text` it decorates and where it points.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AnchoredRange<'a> {
    pub(crate) start: usize,
    pub(crate) end: usize,
    /// Mirrors [`LinkAnchor::writes_out_url`] for the claiming annotation.
    pub(crate) writes_out_url: bool,
    /// Position of the claiming annotation in the page's anchor list, so a
    /// caller can tell two annotations apart when they share a URL.
    pub(crate) index: usize,
    pub(crate) url: &'a str,
}

impl PageLinkAnchors {
    pub(crate) fn new(anchors: Vec<LinkAnchor>) -> Self {
        Self { anchors }
    }

    /// The page's annotations, in the order `ranges_for` indexes them.
    pub(crate) fn iter(&self) -> std::slice::Iter<'_, LinkAnchor> {
        self.anchors.iter()
    }

    /// Record which annotations have their own URL as their visible text.
    ///
    /// `anchor_text` returns the text under an annotation's whole rectangle,
    /// by its position in this list. Whitespace is ignored in the comparison
    /// because the text carries the producer's line break where the URL has
    /// none.
    pub(crate) fn mark_urls_written_out(&mut self, anchor_text: impl Fn(usize) -> Option<String>) {
        for (index, anchor) in self.anchors.iter_mut().enumerate() {
            anchor.writes_out_url =
                anchor_text(index).is_some_and(|text| anchor_repeats_url(&text, &anchor.url));
        }
    }

    /// The stretches of `item` covered by this page's links, in reading order
    /// and never overlapping.
    ///
    /// A geometric answer: which annotation covers which characters. Whether
    /// the Markdown may write that annotation's destination is a separate
    /// question — [`Self::emittable_ranges_for`] — so a rectangle the output
    /// leaves undecorated still reports the words it covers through
    /// [`crate::MarkdownLink::anchor`].
    ///
    /// Two annotations that claim overlapping stretches of the same span are a
    /// contradiction in the source document; the earlier stretch wins so the
    /// emitted Markdown stays well formed, and the loser is reported as
    /// unanchored by [`crate::markdown::links`].
    pub(crate) fn ranges_for(&self, item: &TextItem) -> Vec<AnchoredRange<'_>> {
        if self.anchors.is_empty() {
            return Vec::new();
        }
        let mut ranges: Vec<AnchoredRange<'_>> = self
            .anchors
            .iter()
            .enumerate()
            .filter_map(|(index, anchor)| {
                anchor.char_range(item).map(|(start, end)| AnchoredRange {
                    start,
                    end,
                    writes_out_url: anchor.writes_out_url,
                    index,
                    url: anchor.url.as_str(),
                })
            })
            .collect();
        ranges.sort_by_key(|range| (range.start, range.end));
        let mut kept: Vec<AnchoredRange<'_>> = Vec::with_capacity(ranges.len());
        for range in ranges {
            if kept.last().is_none_or(|last| last.end <= range.start) {
                kept.push(range);
            }
        }
        kept
    }

    /// The stretches of `item` the Markdown may decorate: [`Self::ranges_for`]
    /// without the annotations whose destination the Markdown does not carry
    /// (see [`destination_allowed_in_markdown`]). Their text is emitted plain.
    pub(crate) fn emittable_ranges_for(&self, item: &TextItem) -> Vec<AnchoredRange<'_>> {
        let mut ranges = self.ranges_for(item);
        ranges.retain(|range| destination_allowed_in_markdown(range.url));
        ranges
    }
}

/// True when `anchor` is the destination itself, written out: identical to
/// `url`, ignoring the whitespace a line break puts in the text and a trailing
/// slash the producer may have dropped.
///
/// The whole destination, not a long-enough stretch of it. A partial match is
/// exactly the case a reference list produces — a URL broken over two visual
/// lines carries one annotation per line, so each annotation's text is a
/// fragment of its own destination — and leaving such a fragment plain hands
/// `format_urls` a truncated URL to linkify, which both loses the real
/// destination and invents a wrong one (`https://docs.microsoft.com/x/edge-`
/// for a link to `…/edge-policies#audiosandboxenabled`). A fragment is
/// wrapped, so the destination survives whatever its anchor looks like.
pub(crate) fn anchor_repeats_url(anchor: &str, url: &str) -> bool {
    let squeezed: String = anchor.chars().filter(|c| !c.is_whitespace()).collect();
    squeezed == url || squeezed.trim_end_matches('/') == url.trim_end_matches('/')
}

/// A `[anchor](url)` still being assembled: where its text starts in the
/// rendered line, and where it points.
///
/// It is left open until the text stops belonging to it, so a destination the
/// producer split into several runs — a URL broken after a hyphen, a name
/// broken after its first word — comes out as one link over the whole anchor
/// instead of one per run. Keeping it open is also what lets the line's own
/// spacing rules read an unpolluted tail: they inspect the last characters
/// emitted, and a `](url)` closed too early would hide the hyphen they join on.
struct OpenLink {
    url: String,
    start: usize,
    /// The annotation writes its own URL out as the visible text, so the run
    /// is emitted unwrapped. See [`LinkAnchor::writes_out_url`].
    writes_out_url: bool,
}

/// Close the link `result` has been accumulating, wrapping everything since
/// its start in `[anchor](url)`.
///
/// An anchor that already spells its own destination is left as it is:
/// `MarkdownOptions::format_urls` links a bare URL in the text, so wrapping
/// here would only produce `[https://x](https://x)`.
fn close_open_link(result: &mut String, open: &mut Option<OpenLink>) {
    let Some(OpenLink {
        url,
        start,
        writes_out_url,
    }) = open.take()
    else {
        return;
    };
    if writes_out_url {
        return;
    }
    let anchor = result[start..].to_string();
    let trimmed = anchor.trim();
    if trimmed.is_empty() {
        return;
    }
    // Whitespace at the edges of the anchor belongs to the text around it.
    let lead = anchor.len() - anchor.trim_start().len();
    let mut wrapped = String::with_capacity(anchor.len() + url.len() + 8);
    wrapped.push_str(&anchor[..lead]);
    wrapped.push_str(&inline_link(trimmed, &url));
    wrapped.push_str(&anchor[lead + trimmed.len()..]);
    result.replace_range(start.., &wrapped);
}

/// Append an item's text, opening a link at every stretch a annotation claims
/// and leaving the last one open when it reaches the item's end.
///
/// `text` is the slice of `item.text` being emitted and `leading` how many of
/// `item.text`'s characters precede it, because the ranges are indices into
/// the item's own untrimmed text. A super/subscript run is emitted unlinked:
/// its `<sup>` tags would have to nest inside the anchor, and a footnote
/// marker is not anchor text anyone reads.
fn push_item_text_with_links(
    result: &mut String,
    item: &TextItem,
    text: &str,
    leading: usize,
    ranges: &[AnchoredRange<'_>],
    open: &mut Option<OpenLink>,
) {
    if ranges.is_empty() || item.is_script() {
        push_item_text(result, item, text);
        return;
    }
    let chars: Vec<char> = text.chars().collect();
    let mut cursor = 0usize;
    for range in ranges {
        let start = range.start.saturating_sub(leading).min(chars.len());
        let end = range.end.saturating_sub(leading).min(chars.len());
        if end <= start || start < cursor {
            continue;
        }
        if start > cursor {
            close_open_link(result, open);
            result.extend(&chars[cursor..start]);
        }
        if open.as_ref().is_none_or(|link| link.url != range.url) {
            close_open_link(result, open);
            *open = Some(OpenLink {
                url: range.url.to_string(),
                start: result.len(),
                writes_out_url: range.writes_out_url,
            });
        }
        result.extend(&chars[start..end]);
        cursor = end;
    }
    if cursor < chars.len() {
        close_open_link(result, open);
        result.extend(&chars[cursor..]);
    }
}

/// Render one inline Markdown link.
///
/// The single place that knows how the syntax is written — bracket escaping in
/// the anchor and angle brackets around a destination that would otherwise
/// terminate early — so line rendering and table cells cannot disagree about
/// it.
pub(crate) fn inline_link(anchor: &str, url: &str) -> String {
    let mut result = String::with_capacity(anchor.len() + url.len() + 8);
    result.push('[');
    for character in anchor.chars() {
        if matches!(character, '[' | ']') {
            result.push('\\');
        }
        result.push(character);
    }
    result.push_str("](");
    push_link_destination(&mut result, url);
    result.push(')');
    result
}

/// Schemes a `/Link /URI` destination may carry into the Markdown.
///
/// Everything else is a navigation gesture the PDF viewer performs, not a
/// destination a reader of the Markdown can follow: `javascript:void(0)` on a
/// reference that expands in place, a bare `#` on a dead in-document jump.
/// Emitting them adds noise the reader cannot act on, and hands whatever
/// renders the Markdown downstream a script URL this crate never had a reason
/// to produce.
const MARKDOWN_LINK_SCHEMES: [&str; 4] = ["http", "https", "mailto", "tel"];

/// True when `url` may be written into the Markdown as a link destination.
///
/// The annotation itself is reported either way — [`crate::MarkdownLink::url`]
/// carries the raw URI as the file spells it — so nothing about the document
/// is hidden from a consumer; only the emitted Markdown is kept to
/// destinations a reader can follow.
pub(crate) fn destination_allowed_in_markdown(url: &str) -> bool {
    // The URI reaches here exactly as the file spells it, whitespace included
    // (`extractor::links`), and a producer that padded it must not lose the
    // destination over the padding.
    let url = url.trim();
    let Some(colon) = url.find(':') else {
        return false;
    };
    let scheme = &url[..colon];
    MARKDOWN_LINK_SCHEMES
        .iter()
        .any(|allowed| scheme.eq_ignore_ascii_case(allowed))
}

/// Append a link destination, wrapping it in angle brackets when it carries
/// characters that would terminate a bare `(...)` destination.
fn push_link_destination(result: &mut String, url: &str) {
    if url.contains(['(', ')', ' ', '<', '>']) {
        result.push('<');
        result.push_str(&url.replace('<', "%3C").replace('>', "%3E"));
        result.push('>');
        return;
    }
    result.push_str(url);
}

/// Append an item's text, wrapping a super/subscript run in its tag.
/// Shared by line rendering and table-cell joining so both emit the same
/// markup for a run.
pub(crate) fn push_item_text(result: &mut String, item: &TextItem, text: &str) {
    let tag = if item.baseline_shift > 0.0 {
        "sup"
    } else if item.baseline_shift < 0.0 {
        "sub"
    } else {
        result.push_str(text);
        return;
    };
    result.push('<');
    result.push_str(tag);
    result.push('>');
    result.push_str(text);
    result.push_str("</");
    result.push_str(tag);
    result.push('>');
}

impl TextLine {
    pub fn text(&self) -> String {
        self.text_with_formatting(false, false, false)
    }

    /// Get text with optional bold/italic/decorative markdown formatting.
    ///
    /// `format_decorations` enables both geometrically detected source
    /// decorations: underline (`<u>`) and strikeout (`<s>`).
    pub fn text_with_formatting(
        &self,
        format_bold: bool,
        format_italic: bool,
        format_decorations: bool,
    ) -> String {
        self.text_with_formatting_and_links(
            format_bold,
            format_italic,
            format_decorations,
            &PageLinkAnchors::default(),
        )
    }

    /// Get text with optional formatting, wrapping every stretch covered by a
    /// link annotation in `[anchor](url)`.
    ///
    /// The anchor markup is applied to the formatted text only. Callers that
    /// pattern-match the line (list markers, captions, folios) use
    /// [`TextLine::text`], which stays free of it.
    pub(crate) fn text_with_formatting_and_links(
        &self,
        format_bold: bool,
        format_italic: bool,
        format_decorations: bool,
        links: &PageLinkAnchors,
    ) -> String {
        if !format_bold && !format_italic && !format_decorations {
            return self.text_plain(links);
        }

        let single_char_threshold = self.adaptive_threshold;

        let mut result = String::new();
        let mut open_link: Option<OpenLink> = None;
        let mut current_bold = false;
        let mut current_italic = false;
        let mut current_underline = false;
        let mut current_strikeout = false;

        for (i, item) in self.items.iter().enumerate() {
            let text = item.text.as_str();
            let text_trimmed = text.trim();

            // Skip empty items
            if text_trimmed.is_empty() {
                continue;
            }

            // Determine spacing
            let needs_space = if i == 0 || result.is_empty() {
                false
            } else {
                let prev_item = &self.items[i - 1];
                self.needs_space_between(prev_item, item, &result, single_char_threshold)
            };

            // Preserve leading whitespace from the item text.
            // Items like " means any person" have a leading space that indicates
            // a word boundary. needs_space_between returns false for these (because
            // space_already_exists), but we still need to emit the space since
            // we push text_trimmed below (which strips it).
            let has_leading_space = text.starts_with(' ');
            let emit_space =
                needs_space || (has_leading_space && !result.is_empty() && !result.ends_with(' '));

            // A super/subscript run is wrapped in `<sup>`/`<sub>` (see
            // `text_plain`). It neither opens nor closes the other styles: a
            // footnote marker inside a bold name keeps the bold run intact
            // ("**Yibo Yan<sup>1</sup>, Jiahao Huo**") instead of splitting
            // it around the marker.
            let is_script = item.is_script();

            // Check for style changes. Source decorations are exclusive:
            // `<u>`/`<s>` content stays free of `**`/`*` markers — consumers
            // (and the eval harnesses this feeds) match tag content literally,
            // and mixed nesting breaks that. A struck-and-underlined item is
            // emitted as struck text because deletion is the stronger semantic
            // distinction in redline documents.
            let own_strikeout = format_decorations && item.is_strikeout;
            let own_underline = format_decorations && item.is_underline && !own_strikeout;
            let own_bold = format_bold && item.is_bold && !own_underline && !own_strikeout;
            let own_italic = format_italic && item.is_italic && !own_underline && !own_strikeout;
            // A script run inherits whatever body style is open around it
            // (see above) — its own bold/italic is noise (italic math indices
            // would shatter into `*<sub>t</sub>*` fragments) — but a run
            // carrying its own DECORATION, an underlined link marker in plain
            // text, keeps it: decorations are drawn ink, not font styling.
            let (item_strikeout, item_underline, item_bold, item_italic) = if is_script {
                (
                    current_strikeout,
                    current_underline,
                    current_bold,
                    current_italic,
                )
            } else {
                (own_strikeout, own_underline, own_bold, own_italic)
            };
            // A run's own decoration is emitted around the run itself and
            // closed right after it, so it never leaks onto the next run.
            let own_script_tag = if !is_script {
                None
            } else if own_strikeout && !current_strikeout {
                Some("s")
            } else if own_underline && !current_underline && !current_strikeout {
                Some("u")
            } else {
                None
            };

            // A link run ends wherever the styling changes, so an anchor
            // never has to interleave with `**`/`<u>` markers, and ends
            // wherever the next item is not its continuation.
            let ranges = links.emittable_ranges_for(item);
            let leading = text.chars().take_while(|c| c.is_whitespace()).count();
            let continues = open_link.as_ref().is_some_and(|open| {
                !is_script
                    && item_bold == current_bold
                    && item_italic == current_italic
                    && item_underline == current_underline
                    && item_strikeout == current_strikeout
                    && ranges
                        .first()
                        .is_some_and(|range| range.url == open.url && range.start <= leading)
            });
            if !continues {
                close_open_link(&mut result, &mut open_link);
            }

            // Close previous styles if they change
            if current_italic && !item_italic {
                result.push('*');
                current_italic = false;
            }
            if current_bold && !item_bold {
                result.push_str("**");
                current_bold = false;
            }
            if current_underline && !item_underline {
                result.push_str("</u>");
                current_underline = false;
            }
            if current_strikeout && !item_strikeout {
                result.push_str("</s>");
                current_strikeout = false;
            }

            // Add space: either from spacing logic or preserved from item text
            if emit_space {
                result.push(' ');
            }

            // Open new styles
            if item_underline && !current_underline {
                result.push_str("<u>");
                current_underline = true;
            }
            if item_strikeout && !current_strikeout {
                result.push_str("<s>");
                current_strikeout = true;
            }
            if item_bold && !current_bold {
                result.push_str("**");
                current_bold = true;
            }
            if item_italic && !current_italic {
                result.push('*');
                current_italic = true;
            }

            if i > 0 && stacked_fraction_slash(&self.items[i - 1], item) {
                result.push('/');
            }
            match own_script_tag {
                Some(tag) => {
                    result.push('<');
                    result.push_str(tag);
                    result.push('>');
                    push_item_text(&mut result, item, text_trimmed);
                    result.push_str("</");
                    result.push_str(tag);
                    result.push('>');
                }
                None => push_item_text_with_links(
                    &mut result,
                    item,
                    text_trimmed,
                    leading,
                    &ranges,
                    &mut open_link,
                ),
            }
        }

        // The anchor closes before the style markers, so it never wraps them.
        close_open_link(&mut result, &mut open_link);

        // Close any remaining open styles
        if current_italic {
            result.push('*');
        }
        if current_bold {
            result.push_str("**");
        }
        if current_underline {
            result.push_str("</u>");
        }
        if current_strikeout {
            result.push_str("</s>");
        }

        result
    }

    /// Get plain text without formatting.
    ///
    /// A super/subscript run (an item with a non-zero `baseline_shift`;
    /// extraction materializes each run as one item) is wrapped in
    /// `<sup>…</sup>` / `<sub>…</sub>`: without the tags the marker digits
    /// would be indistinguishable from the body text they follow
    /// ("Yibo Yan1,2,3" vs "Yibo Yan<sup>1,2,3</sup>").
    fn text_plain(&self, links: &PageLinkAnchors) -> String {
        let single_char_threshold = self.adaptive_threshold;

        let mut result = String::new();
        let mut open_link: Option<OpenLink> = None;
        for (i, item) in self.items.iter().enumerate() {
            let ranges = links.emittable_ranges_for(item);
            let continues = open_link.as_ref().is_some_and(|open| {
                !item.is_script()
                    && ranges
                        .first()
                        .is_some_and(|range| range.url == open.url && range.start == 0)
            });
            if i > 0
                && self.needs_space_between(
                    &self.items[i - 1],
                    item,
                    &result,
                    single_char_threshold,
                )
            {
                if !continues {
                    close_open_link(&mut result, &mut open_link);
                }
                result.push(' ');
            } else if !continues {
                close_open_link(&mut result, &mut open_link);
            }
            if i > 0 && stacked_fraction_slash(&self.items[i - 1], item) {
                result.push('/');
            }
            push_item_text_with_links(
                &mut result,
                item,
                item.text.as_str(),
                0,
                &ranges,
                &mut open_link,
            );
        }
        close_open_link(&mut result, &mut open_link);
        result
    }

    /// Determine if a space is needed between two items
    fn needs_space_between(
        &self,
        prev_item: &TextItem,
        item: &TextItem,
        result: &str,
        single_char_threshold: f32,
    ) -> bool {
        let text = item.text.as_str();

        // Don't add space before/after hyphens for hyphenated words
        let prev_ends_with_hyphen = result.ends_with('-');
        let curr_is_hyphen = text.trim() == "-";
        let curr_starts_with_hyphen = text.starts_with('-');

        // Check if space already exists
        let prev_ends_with_space = result.ends_with(' ');
        let curr_starts_with_space = text.starts_with(' ');
        let space_already_exists = prev_ends_with_space || curr_starts_with_space;

        // Script runs flagged by extraction share one edge-spacing policy
        // with table cells (see `script_edge_needs_space`). The blanket
        // suppression below is for unflagged size changes only.
        if let Some(needs_space) = script_edge_needs_space(prev_item, item, result, text) {
            return needs_space;
        }

        // Detect subscript/superscript: smaller font size and/or Y offset
        let font_ratio = item.font_size / prev_item.font_size;
        let reverse_font_ratio = prev_item.font_size / item.font_size;
        let y_diff = (item.y - prev_item.y).abs();

        let is_sub_super = font_ratio < 0.85 && y_diff > 1.0;
        let was_sub_super = reverse_font_ratio < 0.85 && y_diff > 1.0;

        // Use position-based spacing detection
        let should_join = should_join_items(prev_item, item, single_char_threshold);

        // Add space unless one of these conditions applies
        !(prev_ends_with_hyphen
            || curr_is_hyphen
            || curr_starts_with_hyphen
            || is_sub_super
            || was_sub_super
            || should_join
            || space_already_exists)
    }
}

#[cfg(test)]
mod formatting_tests {
    use super::{ItemType, TextItem, TextLine};

    fn item(text: &str, x: f32, width: f32, strikeout: bool) -> TextItem {
        TextItem {
            text: text.to_string(),
            x,
            y: 100.0,
            width,
            height: 12.0,
            font: "F1".to_string(),
            font_tag: String::new(),
            font_size: 12.0,
            page: 1,
            is_bold: false,
            is_italic: false,
            is_underline: false,
            is_strikeout: strikeout,
            rotation: 0.0,
            advance_known: true,
            item_type: ItemType::Text,
            mcid: None,
            baseline_shift: 0.0,
        }
    }

    fn line(items: Vec<TextItem>) -> TextLine {
        TextLine {
            items,
            y: 100.0,
            page: 1,
            adaptive_threshold: 0.1,
        }
    }

    /// A body-text item at 12pt on the shared baseline.
    fn body(text: &str, x: f32, width: f32) -> TextItem {
        item(text, x, width, false)
    }

    /// A script run at 8pt, `shift` points off the 12pt body baseline
    /// (positive = raised), as `merge_subscript_items` materializes it: one
    /// item per run.
    fn script(text: &str, x: f32, width: f32, shift: f32) -> TextItem {
        let mut it = item(text, x, width, false);
        it.font_size = 8.0;
        it.height = 8.0;
        it.y += shift;
        it.baseline_shift = shift;
        it
    }

    #[test]
    fn script_run_is_wrapped_in_one_sup_span() {
        // Author block: "Yibo Yan" + the raised "1,2" run + body ", " +
        // "Jiahao Huo". The marker run is one <sup> span attached to the
        // name, and the body comma follows without a space.
        let line = line(vec![
            body("Yibo Yan", 10.0, 48.0),
            script("1,2", 58.0, 10.0, 4.3),
            body(",", 68.0, 3.0),
            body("Jiahao Huo", 74.5, 60.0),
        ]);
        assert_eq!(line.text(), "Yibo Yan<sup>1,2</sup>, Jiahao Huo");
        assert_eq!(
            line.text_with_formatting(true, true, true),
            "Yibo Yan<sup>1,2</sup>, Jiahao Huo"
        );
    }

    #[test]
    fn word_space_after_script_run_follows_geometry() {
        let line = line(vec![
            body("word", 10.0, 24.0),
            script("2", 34.0, 4.0, 4.0),
            body("next", 41.5, 24.0),
        ]);
        assert_eq!(line.text(), "word<sup>2</sup> next");

        // Tight junction after the marker: no space ("x<sup>2</sup>y").
        let line = super::TextLine {
            items: vec![
                body("x", 10.0, 6.0),
                script("2", 16.2, 4.0, 4.0),
                body("y", 20.4, 6.0),
            ],
            y: 100.0,
            page: 1,
            adaptive_threshold: 0.1,
        };
        assert_eq!(line.text(), "x<sup>2</sup>y");
    }

    #[test]
    fn leading_script_run_attaches_to_following_word() {
        // Affiliation line: markers lead their institution, and a word space
        // before the run (after the previous institution's comma) survives.
        let line = line(vec![
            body("University,", 10.0, 60.0),
            script("1,2", 73.4, 10.2, 3.5),
            body("Hong Kong", 83.6, 54.0),
        ]);
        assert_eq!(line.text(), "University, <sup>1,2</sup>Hong Kong");
    }

    #[test]
    fn lowered_run_uses_sub_tag() {
        let line = line(vec![body("x", 10.0, 6.0), script("max", 16.0, 12.0, -2.4)]);
        assert_eq!(line.text(), "x<sub>max</sub>");
    }

    #[test]
    fn script_span_does_not_split_a_bold_run() {
        let mut name = body("Yibo Yan", 10.0, 48.0);
        name.is_bold = true;
        let mut rest = body(", Jiahao Huo", 62.0, 66.0);
        rest.is_bold = true;
        let line = line(vec![name, script("1", 58.0, 4.0, 4.3), rest]);
        assert_eq!(
            line.text_with_formatting(true, false, false),
            "**Yibo Yan<sup>1</sup>, Jiahao Huo**"
        );
        assert_eq!(line.text(), "Yibo Yan<sup>1</sup>, Jiahao Huo");
    }

    #[test]
    fn line_y_of_an_upside_down_run_is_its_baseline() {
        // A 180° run hangs from its box top: that is the baseline its line
        // groups by, a script offset still applies below it, and an upright
        // run keeps `y`.
        let mut run = item("x", 100.0, 20.0, false);
        run.y = 500.0;
        run.height = 10.0;
        run.rotation = 180.0;
        assert_eq!(run.baseline_y(), 510.0);
        assert_eq!(run.line_y(), 510.0);
        run.baseline_shift = 2.0;
        assert_eq!(run.line_y(), 508.0);
        run.rotation = 0.0;
        assert_eq!(run.baseline_y(), 500.0);
        assert_eq!(run.line_y(), 498.0);
    }

    #[test]
    fn separated_script_items_get_separate_spans() {
        // Two runs with a real gap between them (nothing else on the line)
        // are two spans with a space, never "<sup>1 2</sup>".
        let line = line(vec![
            script("1", 10.0, 4.0, 4.0),
            script("2", 40.0, 4.0, 4.0),
        ]);
        assert_eq!(line.text(), "<sup>1</sup> <sup>2</sup>");
    }

    #[test]
    fn touching_runs_of_different_size_are_separate_spans_without_space() {
        // Nested script: "n" (6pt) attached to the superscript "2" (8pt).
        let mut nested = script("n", 20.2, 3.0, 6.0);
        nested.font_size = 6.0;
        let line = line(vec![
            body("x", 10.0, 6.0),
            script("2", 16.2, 4.0, 4.0),
            nested,
        ]);
        assert_eq!(line.text(), "x<sup>2</sup><sup>n</sup>");
    }

    #[test]
    fn stacked_digit_fraction_renders_with_a_slash() {
        // "3 1/3 bits" set as a case fraction: numerator raised, denominator
        // lowered, both at the same x. Never "3 <sup>13</sup>".
        let mut num = script("1", 52.5, 3.7, 3.96);
        num.font_size = 7.4;
        let mut den = script("3", 52.5, 3.7, -4.0);
        den.font_size = 7.4;
        let line = line(vec![
            body("about 3", 10.0, 40.8),
            num,
            den,
            body("bits", 58.0, 20.0),
        ]);
        assert_eq!(line.text(), "about 3 <sup>1</sup>/<sub>3</sub> bits");
    }

    #[test]
    fn decorated_script_run_keeps_its_own_underline() {
        // An underlined (hyperlinked) footnote marker in plain text.
        let mut marker = script("1", 58.0, 4.0, 4.3);
        marker.is_underline = true;
        let line = line(vec![body("word", 10.0, 48.0), marker]);
        assert_eq!(
            line.text_with_formatting(false, false, true),
            "word<u><sup>1</sup></u>"
        );
    }

    #[test]
    fn own_decoration_does_not_leak_onto_the_next_run() {
        let mut first = script("1", 58.0, 4.0, 4.3);
        first.is_underline = true;
        let second = script("2", 80.0, 4.0, 4.3);
        let line = line(vec![body("word", 10.0, 48.0), first, second]);
        assert_eq!(
            line.text_with_formatting(false, false, true),
            "word<u><sup>1</sup></u> <sup>2</sup>"
        );
    }

    #[test]
    fn own_decoration_nests_inside_an_open_body_decoration() {
        // Underlined body text with a struck footnote marker: the strike
        // nests inside the underline instead of being dropped.
        let mut word = body("word", 10.0, 48.0);
        word.is_underline = true;
        let mut marker = script("1", 58.0, 4.0, 4.3);
        marker.is_strikeout = true;
        let line = line(vec![word, marker]);
        assert_eq!(
            line.text_with_formatting(false, false, true),
            "<u>word<s><sup>1</sup></s></u>"
        );
    }

    #[test]
    fn fraction_slash_needs_one_visual_line() {
        // Opposite-sign digit runs on different lines are not a fraction.
        let mut num = script("1", 52.5, 3.7, 3.96);
        num.font_size = 7.4;
        let mut den = script("3", 52.5, 3.7, -4.0);
        den.font_size = 7.4;
        den.y -= 12.0; // anchored to the next line's body
        assert!(!super::stacked_fraction_slash(&num, &den));
    }

    #[test]
    fn line_y_snaps_scripts_to_the_anchor_baseline() {
        let raised = script("1", 58.0, 4.0, 4.3);
        assert!((raised.y - 104.3).abs() < 1e-4);
        assert!((raised.line_y() - 100.0).abs() < 1e-4);
        assert!(raised.is_script());
        assert!(!body("Yibo Yan", 10.0, 48.0).is_script());
    }

    #[test]
    fn formatting_emits_semantic_strikeout() {
        let line = line(vec![item("deleted", 10.0, 42.0, true)]);

        assert_eq!(
            line.text_with_formatting(true, true, true),
            "<s>deleted</s>"
        );
    }

    #[test]
    fn formatting_closes_strikeout_before_live_text() {
        let line = line(vec![
            item("keep", 10.0, 24.0, false),
            item("remove", 40.0, 42.0, true),
            item("keep", 88.0, 24.0, false),
        ]);

        assert_eq!(
            line.text_with_formatting(true, true, true),
            "keep <s>remove</s> keep"
        );
    }

    #[test]
    fn formatting_coalesces_adjacent_struck_items() {
        let line = line(vec![
            item("deleted", 10.0, 42.0, true),
            item("words", 58.0, 30.0, true),
        ]);

        assert_eq!(
            line.text_with_formatting(true, true, true),
            "<s>deleted words</s>"
        );
    }

    #[test]
    fn strikeout_takes_precedence_over_other_styles() {
        let mut decorated = item("deleted", 10.0, 42.0, true);
        decorated.is_bold = true;
        decorated.is_italic = true;
        decorated.is_underline = true;
        let line = line(vec![decorated]);

        assert_eq!(
            line.text_with_formatting(true, true, true),
            "<s>deleted</s>"
        );
        assert_eq!(line.text(), "deleted");
    }

    #[test]
    fn is_horizontal_follows_the_baseline_quadrant() {
        let cases = [
            (0.0, true),
            (180.0, true),
            (90.0, false),
            (270.0, false),
            (44.0, true),
            (46.0, false),
            (134.0, false),
            (136.0, true),
            (359.5, true),
            (-90.0, false),
            (450.0, false),
        ];
        for (rotation, horizontal) in cases {
            let mut probe = item("x", 0.0, 10.0, false);
            probe.rotation = rotation;
            assert_eq!(
                probe.is_horizontal(),
                horizontal,
                "rotation {rotation} should be horizontal={horizontal}"
            );
        }
    }

    #[test]
    fn cross_extent_is_the_em_box_whatever_the_orientation() {
        let mut probe = item("x", 0.0, 10.0, false);
        probe.height = 12.0;
        assert_eq!(probe.cross_extent(), 12.0);
        probe.rotation = 90.0;
        probe.width = 12.0;
        probe.height = 200.0;
        assert_eq!(probe.cross_extent(), 12.0);
        // A long diagonal run: both box extents carry the advance, the em
        // is the font size.
        probe.rotation = 30.0;
        probe.width = 180.0;
        probe.height = 110.0;
        assert_eq!(probe.cross_extent(), 12.0);
    }

    #[test]
    fn upright_and_upside_down_split_the_horizontal_half_plane() {
        let mut probe = item("x", 0.0, 10.0, false);
        for (rotation, upright, upside_down) in [
            (0.0, true, false),
            (44.0, true, false),
            (316.0, true, false),
            (180.0, false, true),
            (136.0, false, true),
            (224.0, false, true),
            (90.0, false, false),
            (270.0, false, false),
        ] {
            probe.rotation = rotation;
            assert_eq!(probe.is_upright(), upright, "upright at {rotation}");
            assert_eq!(
                probe.is_upside_down(),
                upside_down,
                "upside_down at {rotation}"
            );
        }
    }
}

#[cfg(test)]
mod link_anchor_tests {
    use super::{anchor_repeats_url, ItemType, LinkAnchor, PageLinkAnchors, TextItem, TextLine};

    /// A 12pt span whose glyphs occupy `width` points from `x`.
    fn span(text: &str, x: f32, width: f32) -> TextItem {
        TextItem {
            text: text.to_string(),
            x,
            y: 100.0,
            width,
            height: 12.0,
            font: "F1".to_string(),
            font_tag: String::new(),
            font_size: 12.0,
            page: 1,
            is_bold: false,
            is_italic: false,
            is_underline: false,
            is_strikeout: false,
            rotation: 0.0,
            advance_known: true,
            item_type: ItemType::Text,
            mcid: None,
            baseline_shift: 0.0,
        }
    }

    fn line(items: Vec<TextItem>) -> TextLine {
        TextLine {
            items,
            y: 100.0,
            page: 1,
            adaptive_threshold: 0.1,
        }
    }

    /// A rectangle covering the span's full height, from `x` for `width`.
    fn anchor(url: &str, x: f32, width: f32) -> LinkAnchor {
        LinkAnchor {
            url: url.to_string(),
            x,
            y: 97.0,
            width,
            height: 18.0,
            writes_out_url: false,
        }
    }

    #[test]
    fn rectangle_over_a_whole_span_anchors_all_of_it() {
        let line = line(vec![span("Statista", 10.0, 48.0)]);
        let anchors = PageLinkAnchors::new(vec![anchor("https://statista.com", 8.0, 52.0)]);

        assert_eq!(
            line.text_with_formatting_and_links(false, false, false, &anchors),
            "[Statista](https://statista.com)"
        );
    }

    #[test]
    fn three_rectangles_over_one_merged_span_each_take_their_own_words() {
        // The producer merged a whole source line into one span and hung a
        // separate annotation on each name in it. 38 characters over 190pt
        // is 5pt per character: "Linkedin." starts at char 7, "Jordan
        // Blake." at char 17, "Medium." at char 31.
        let line = line(vec![span(
            "Fonte: Linkedin. Jordan Blake. Medium.",
            10.0,
            190.0,
        )]);
        let anchors = PageLinkAnchors::new(vec![
            anchor("https://linkedin.com", 45.0, 45.0),
            anchor("https://jordanblake.co.uk", 95.0, 65.0),
            anchor("https://medium.com", 165.0, 35.0),
        ]);

        assert_eq!(
            line.text_with_formatting_and_links(false, false, false, &anchors),
            "Fonte: [Linkedin.](https://linkedin.com) \
             [Jordan Blake.](https://jordanblake.co.uk) [Medium.](https://medium.com)"
        );
    }

    #[test]
    fn a_generously_drawn_rectangle_still_anchors_its_span() {
        // Producers draw a link rectangle with vertical padding: 36pt of
        // rectangle around a 12pt span. The overlap is the whole span but
        // only a third of the rectangle, so a ratio taken on the rectangle
        // would reject the match that is plainly correct.
        let line = line(vec![span("Statista", 10.0, 48.0)]);
        let anchors = PageLinkAnchors::new(vec![LinkAnchor {
            url: "https://statista.com".to_string(),
            x: 8.0,
            y: 88.0,
            width: 52.0,
            height: 36.0,
            writes_out_url: false,
        }]);

        assert_eq!(
            line.text_with_formatting_and_links(false, false, false, &anchors),
            "[Statista](https://statista.com)"
        );
    }

    #[test]
    fn a_rectangle_beside_the_span_anchors_nothing() {
        let line = line(vec![span("Statista", 10.0, 48.0)]);
        let anchors = PageLinkAnchors::new(vec![anchor("https://statista.com", 200.0, 52.0)]);

        assert!(anchors.ranges_for(&line.items[0]).is_empty());
        assert_eq!(
            line.text_with_formatting_and_links(false, false, false, &anchors),
            "Statista"
        );
    }

    #[test]
    fn a_rectangle_on_the_line_below_anchors_nothing() {
        // Same x, a full line lower: the vertical share of the span inside
        // the rectangle is nil.
        let line = line(vec![span("Statista", 10.0, 48.0)]);
        let anchors = PageLinkAnchors::new(vec![LinkAnchor {
            url: "https://statista.com".to_string(),
            x: 8.0,
            y: 79.0,
            width: 52.0,
            height: 18.0,
            writes_out_url: false,
        }]);

        assert!(anchors.ranges_for(&line.items[0]).is_empty());
    }

    #[test]
    fn an_annotation_that_writes_out_its_url_is_left_for_url_formatting() {
        // Wrapping here would emit `[https://example.com](https://example.com)`;
        // `MarkdownOptions::format_urls` links the bare URL instead.
        let line = line(vec![span("https://example.com", 10.0, 75.0)]);
        let mut anchors = PageLinkAnchors::new(vec![anchor("https://example.com", 8.0, 79.0)]);
        anchors.mark_urls_written_out(|_| Some("https://example.com".to_string()));

        assert_eq!(
            line.text_with_formatting_and_links(false, false, false, &anchors),
            "https://example.com"
        );
    }

    #[test]
    fn a_url_broken_over_two_runs_is_recognised_as_written_out() {
        // The producer split the URL after a hyphen and hung an annotation on
        // the whole of it. Decorating either half would sit between the
        // hyphen and the text that postprocessing rejoins it to.
        let mut anchors = PageLinkAnchors::new(vec![anchor(
            "https://docs.microsoft.com/DeployEdge/microsoft-edge-policies",
            8.0,
            300.0,
        )]);
        anchors.mark_urls_written_out(|_| {
            Some("https://docs.microsoft.com/DeployEdge/microsoft-edge- policies".to_string())
        });
        let line = line(vec![span(
            "https://docs.microsoft.com/DeployEdge/microsoft-edge-policies",
            10.0,
            290.0,
        )]);

        assert_eq!(
            line.text_with_formatting_and_links(false, false, false, &anchors),
            "https://docs.microsoft.com/DeployEdge/microsoft-edge-policies"
        );
    }

    #[test]
    fn a_word_that_merely_ends_the_path_stays_a_link() {
        // "Home" is a fifth of the URL that ends in it: the reader is not
        // being shown the destination, so the anchor must be wrapped.
        let mut anchors =
            PageLinkAnchors::new(vec![anchor("https://example.com/en/Home", 8.0, 30.0)]);
        anchors.mark_urls_written_out(|_| Some("Home".to_string()));
        let line = line(vec![span("Home", 10.0, 26.0)]);

        assert_eq!(
            line.text_with_formatting_and_links(false, false, false, &anchors),
            "[Home](https://example.com/en/Home)"
        );
    }

    #[test]
    fn a_url_shown_shortened_still_becomes_a_link() {
        let line = line(vec![span("example.com", 10.0, 35.0)]);
        let anchors = PageLinkAnchors::new(vec![anchor("https://www.example.com/shop", 8.0, 39.0)]);

        assert_eq!(
            line.text_with_formatting_and_links(false, false, false, &anchors),
            "[example.com](https://www.example.com/shop)"
        );
    }

    #[test]
    fn a_destination_with_brackets_is_wrapped_in_angle_brackets() {
        let line = line(vec![span("Roma", 10.0, 24.0)]);
        let anchors = PageLinkAnchors::new(vec![anchor(
            "https://en.wikipedia.org/wiki/Rome_(city)",
            8.0,
            28.0,
        )]);

        assert_eq!(
            line.text_with_formatting_and_links(false, false, false, &anchors),
            "[Roma](<https://en.wikipedia.org/wiki/Rome_(city)>)"
        );
    }

    #[test]
    fn a_destination_the_reader_cannot_follow_is_not_written_into_the_markdown() {
        // `javascript:` is a gesture the viewer performs, not a place; a bare
        // `#` is a dead in-document jump. The words stay, unwrapped, and the
        // raw URI is still reported through `MarkdownLink::url`.
        let line = line(vec![span("Revisiting Models", 10.0, 90.0)]);
        for url in ["javascript:void(0)", "#", "data:text/html,<b>x</b>"] {
            let anchors = PageLinkAnchors::new(vec![anchor(url, 8.0, 94.0)]);
            assert_eq!(
                line.text_with_formatting_and_links(false, false, false, &anchors),
                "Revisiting Models",
                "{url} must not reach the markdown"
            );
        }
        // The schemes a reader can follow still do — including a `/URI` the
        // producer padded with whitespace, which the destination syntax then
        // wraps in angle brackets as it wraps any destination with a space.
        for (url, expected) in [
            ("https://a.it", "[Revisiting Models](https://a.it)"),
            ("HTTP://a.it", "[Revisiting Models](HTTP://a.it)"),
            ("mailto:a@b.it", "[Revisiting Models](mailto:a@b.it)"),
            ("tel:+3901", "[Revisiting Models](tel:+3901)"),
            (" https://a.it", "[Revisiting Models](< https://a.it>)"),
        ] {
            let anchors = PageLinkAnchors::new(vec![anchor(url, 8.0, 94.0)]);
            assert_eq!(
                line.text_with_formatting_and_links(false, false, false, &anchors),
                expected,
                "{url} must reach the markdown"
            );
        }
    }

    #[test]
    fn brackets_in_the_anchor_are_escaped() {
        let line = line(vec![span("see [3]", 10.0, 35.0)]);
        let anchors = PageLinkAnchors::new(vec![anchor("https://example.com", 8.0, 39.0)]);

        assert_eq!(
            line.text_with_formatting_and_links(false, false, false, &anchors),
            "[see \\[3\\]](https://example.com)"
        );
    }

    #[test]
    fn bold_formatting_survives_around_an_anchor() {
        let mut bold = span("Statista", 10.0, 48.0);
        bold.is_bold = true;
        let line = line(vec![bold]);
        let anchors = PageLinkAnchors::new(vec![anchor("https://statista.com", 8.0, 52.0)]);

        assert_eq!(
            line.text_with_formatting_and_links(true, false, false, &anchors),
            "**[Statista](https://statista.com)**"
        );
    }

    #[test]
    fn one_rectangle_over_two_runs_is_one_link() {
        // The producer emitted the anchor as two items. They belong to one
        // annotation, so they come out as one link, not two.
        let line = line(vec![span("Jordan", 10.0, 42.0), span("Blake.", 56.0, 28.0)]);
        let anchors = PageLinkAnchors::new(vec![anchor("https://jordanblake.co.uk", 8.0, 78.0)]);

        assert_eq!(
            line.text_with_formatting_and_links(false, false, false, &anchors),
            "[Jordan Blake.](https://jordanblake.co.uk)"
        );
    }

    #[test]
    fn a_link_run_ends_where_the_styling_changes() {
        // One rectangle over "Fonte Statista", the second word bold. The run
        // is cut at the style boundary so the anchor never has to wrap a
        // `**` marker it does not own; the destination is repeated instead.
        let plain = span("Fonte", 10.0, 30.0);
        let mut bold = span("Statista", 44.0, 48.0);
        bold.is_bold = true;
        let line = line(vec![plain, bold]);
        let anchors = PageLinkAnchors::new(vec![anchor("https://statista.com", 8.0, 86.0)]);

        assert_eq!(
            line.text_with_formatting_and_links(true, false, false, &anchors),
            "[Fonte](https://statista.com) **[Statista](https://statista.com)**"
        );
    }

    #[test]
    fn a_multibyte_anchor_is_clipped_on_characters_not_bytes() {
        // "Perché" and "Società" carry two-byte characters. A rectangle over
        // the second word must cut on characters: a byte offset would land
        // mid-character and panic.
        let line = line(vec![span("Perché Società è qui", 10.0, 105.0)]);
        let anchors = PageLinkAnchors::new(vec![anchor("https://societa.example", 45.0, 40.0)]);

        assert_eq!(
            line.text_with_formatting_and_links(false, false, false, &anchors),
            "Perché [Società](https://societa.example) è qui"
        );
    }

    #[test]
    fn a_rectangle_claiming_only_punctuation_anchors_nothing() {
        // The right edge snaps to the start of the next word, so a rectangle
        // ending just past "org" hands back the full stop that follows it.
        // `[.](mailto:…)` is noise, not an anchor.
        let line = line(vec![span("feedback@cisecurity.org . If", 10.0, 140.0)]);
        let anchors =
            PageLinkAnchors::new(vec![anchor("mailto:feedback@cisecurity.org", 128.0, 10.0)]);

        assert_eq!(
            line.text_with_formatting_and_links(false, false, false, &anchors),
            "feedback@cisecurity.org . If"
        );
    }

    #[test]
    fn a_page_without_annotations_renders_exactly_as_before() {
        let mut bold = span("Statista", 44.0, 48.0);
        bold.is_bold = true;
        let line = line(vec![span("Fonte:", 10.0, 32.0), bold]);

        assert_eq!(
            line.text_with_formatting_and_links(true, true, true, &PageLinkAnchors::default()),
            "Fonte: **Statista**"
        );
        assert_eq!(line.text(), "Fonte: Statista");
    }

    /// A rectangle over the span's full width whose vertical extent covers
    /// `share` of the 12pt span sitting at y = 100.
    fn overlapping_anchor(url: &str, share: f32) -> LinkAnchor {
        LinkAnchor {
            url: url.to_string(),
            x: 8.0,
            y: 100.0,
            width: 60.0,
            height: 12.0 * share,
            writes_out_url: false,
        }
    }

    #[test]
    fn a_span_barely_inside_the_rectangle_is_not_anchored() {
        // Pins the low side of `ANCHOR_VERTICAL_OVERLAP`: a third of a span
        // inside the rectangle is the neighbouring line clipped by a
        // generous `/Rect`, not the line the annotation decorates.
        let line = line(vec![span("Statista", 10.0, 48.0)]);
        let anchors = PageLinkAnchors::new(vec![overlapping_anchor("https://statista.com", 0.35)]);

        assert_eq!(
            line.text_with_formatting_and_links(false, false, false, &anchors),
            "Statista"
        );
    }

    #[test]
    fn a_span_mostly_inside_the_rectangle_is_anchored() {
        // Pins the high side: a producer draws the rectangle on the glyph
        // box, not on the font's full ascent, so two thirds of the span
        // inside it is a match and must not be turned away.
        let line = line(vec![span("Statista", 10.0, 48.0)]);
        let anchors = PageLinkAnchors::new(vec![overlapping_anchor("https://statista.com", 0.65)]);

        assert_eq!(
            line.text_with_formatting_and_links(false, false, false, &anchors),
            "[Statista](https://statista.com)"
        );
    }

    #[test]
    fn an_anchor_that_is_only_part_of_its_destination_is_not_the_url_written_out() {
        // The producer broke the URL after the hyphen and hung one annotation
        // on each visual line. Each half spells most of a plausible URL and
        // none of them spells this one, so neither may be left for
        // `format_urls` to linkify into a destination that does not exist.
        let url =
            "https://docs.microsoft.com/DeployEdge/microsoft-edge-policies#audiosandboxenabled";
        assert!(!anchor_repeats_url(
            "https://docs.microsoft.com/DeployEdge/microsoft-edge-",
            url
        ));
        assert!(!anchor_repeats_url("policies#audiosandboxenabled", url));
        // The two halves together do spell it, and that is the only shape
        // that counts as the destination written out.
        assert!(anchor_repeats_url(
            "https://docs.microsoft.com/DeployEdge/microsoft-edge- policies#audiosandboxenabled",
            url
        ));
    }

    #[test]
    fn a_url_is_recognised_as_its_own_anchor_across_a_trailing_slash() {
        assert!(anchor_repeats_url(
            "https://example.com",
            "https://example.com/"
        ));
        assert!(anchor_repeats_url(
            " https://example.com/ ",
            "https://example.com"
        ));
        assert!(!anchor_repeats_url("example", "https://www.example.com"));
    }

    #[test]
    fn a_mailto_shown_without_its_scheme_is_not_its_own_anchor() {
        // The address covers most of the destination but the scheme is never
        // visible, so leaving it plain would drop the destination.
        assert!(!anchor_repeats_url(
            "info@example.com",
            "mailto:info@example.com"
        ));
    }
}
