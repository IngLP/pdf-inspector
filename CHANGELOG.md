# Changelog

Notable changes to pdf-inspector. Every distribution (Rust crate, Python
package, Node package and platform packages, WebAssembly package) shares one
version. A separate release pull request bumps the manifests with
`scripts/version.py` and renames the `Unreleased` section below to that
version and date. Earlier releases are described in their
[GitHub releases](https://github.com/firecrawl/pdf-inspector/releases).

## [Unreleased]

### Added

- Hyperlink annotations reach the Markdown. A `/Link /URI` rectangle is
  matched to the text it decorates and that text becomes the anchor:
  `Fonte: [Statista, 2024](https://statista.com/...)`. A span is claimed when
  at least half its own height falls inside the rectangle, and only the part
  of it the rectangle covers is anchored, so a producer that merges a whole
  source line into one run ("Fonte: Linkedin. Jordan Blake. Medium.") and
  hangs a separate annotation on each name still gets three correct anchors.
  Anchors inside a detected table's cells are wrapped too. A rectangle whose
  claim on a span is punctuation alone (`[.](mailto:…)`, the comma between two
  linked names) anchors nothing. Text that spells its destination *in full* is
  left alone for `format_urls` — but only while `format_urls` is on, and only
  when it really is the whole destination: a URL the producer broke over two
  lines and hung one annotation on each half is anchored, so the destination
  survives instead of `format_urls` linkifying a URL truncated at the break.
  `format_urls` no longer linkifies inside a destination a `<` has opened —
  the `<https://…>` autolink of the page list, and the `](<https://…>)` form a
  destination carrying a parenthesis or a space takes.
- Only `http`, `https`, `mailto` and `tel` destinations are written into the
  Markdown. `javascript:void(0)` on a reference that expands in place and a
  bare `#` on a dead in-document jump are viewer gestures, not places a reader
  of the Markdown can go; the words under such a rectangle are emitted
  unwrapped, and the annotation is still reported in full — the raw URI in
  `MarkdownLink::url`, the words in `anchor`, with `anchored_inline` false.
- A link whose rectangle covers no text — a clickable logo, a social icon, a
  bitmap chart — is listed at the foot of its page under
  `**Links on this page**` as `- <url>`, with the image under it named when
  there is one, so no destination is dropped. The list carries destinations,
  not annotations: a URL another rectangle on the page already anchors, and a
  second rectangle on a URL the list already names, add no entry.
- `PageMarkdown::links` (Python `PageMarkdown.links`, a list of
  `MarkdownLink` with `url`, `rect`, `page`, `anchor` and `anchored_inline`)
  reports every annotation of a page and what became of it, so a consumer
  never has to parse the Markdown back. Reported for pages routed to OCR too,
  whose annotations are exact file data. `anchored_inline` is `true` when a
  rendered line or table cell carried the anchor; `false` says only that none
  did, and `anchor` is often set alongside it.

- `TextItem::baseline_shift`: signed offset, in points, of a superscript or
  subscript glyph run from the baseline of the body text it is attached to
  (positive = raised, negative = lowered, `0` for normal text). Exposed as
  `baseline_shift` in the Python bindings and `pdf2md --items-json`, and as
  `baselineShift` in the Node bindings, so consumers can emit `<sup>`/`<sub>`
  themselves. `TextItem::line_y()` returns the body baseline a run belongs to
  and `TextItem::is_script()` tells flagged runs apart.
- `TextLine::text()` and `text_with_formatting()` wrap flagged runs in
  `<sup>…</sup>` / `<sub>…</sub>` (`Yibo Yan<sup>1,2,3</sup>`,
  `V<sub>f</sub>`, `10<sup>–15</sup>`), with word spacing decided by the
  measured gap at the run's edges. Table cells render runs the same way
  through one shared cell-text module, and items are assigned to cells by the
  body baseline they belong to, so `V<sub>f</sub>` and `$1,234<sup>1</sup>`
  survive inside tables too.
- `tests/fixtures/cropbox_offset_origin.pdf`, a page whose CropBox origin is
  not `(0, 0)`, with Rust, Node and Python tests pinning the shared coordinate
  frame described under Changed.

### Fixed

- Raised and lowered marker glyphs no longer form their own line. Line
  grouping — the Markdown pipeline and `extract_text_in_regions`
  (`extractTextInRegions`) alike — compares baselines through `line_y()`, so
  affiliation markers 4–7pt above an author line, footnote references after
  a sentence, and unit exponents (`kg/m³`) stay on the line they annotate.
  Previously the whole marker run of an author block came out as an orphan
  `,2,3,2,4,*` line above the names.
- Script detection is geometric: a run is a sub/superscript when it is
  0.4–0.75× the size of a tightly adjacent neighbor and sits at a real
  baseline offset from it. Multi-glyph runs (`1` `,` `2` `,` `3`, `2,*`,
  `1)`, `th`, `max`) are recognised as one run; markers that LEAD their word
  (`¹Hong Kong University`, `<sup>3,4</sup>Some Institute`) attach to the
  following word; markers after closing punctuation (`sentence.²`) and after
  digits (`$1,234<sup>1</sup>`) are no longer glued on as body text.
- Digit-only runs beside a word keep fusing as Unicode super/subscript
  characters (`H₂O`, `word²`, `See note¹²`); level small runs (small caps,
  same-baseline size changes) are no longer mistaken for subscripts.

### Changed

- **Coordinate frame of positioned output — consumer action may be required.**
  `extract_text_with_positions*` (Rust), `extractTextWithPositions` (Node),
  `extract_text_with_positions[_bytes]` (Python) and `pdf2md --items-json` now
  report `x`/`y` relative to the page's visible page box — `CropBox ∩ MediaBox`,
  else the MediaBox — with the box's lower-left corner as the origin. Image,
  link and form-field items shift the same way. Previously the values were raw
  content-stream coordinates, so on pages whose CropBox (or MediaBox) origin is
  not `(0, 0)` every item was displaced from anything rendered from the CropBox,
  and consumers intersecting items with rendered regions silently selected the
  wrong text.
- The region APIs interpret their inputs relative to the same box:
  `extract_text_in_regions*`, `extract_tables_in_regions*`,
  `detect_vector_grid_in_region*` and the TSR crop bboxes
  (`TsrTableInput.crop_pdf_pt_bbox`) are top-left-origin PDF points relative to
  the visible page box, and `StructuredCell.page_pt_bbox` is returned in it.
  These previously flipped `y` with the MediaBox height and ignored the box
  origin.
- Pages whose CropBox equals the MediaBox and whose MediaBox origin is `(0, 0)`
  — the vast majority — produce identical output. Consumers that compensated
  for the CropBox origin themselves must drop that adjustment. `/Rotate` is
  still not applied.
- Rust: `TextItem` gained the required public field `baseline_shift`, so code
  that builds a `TextItem` with a struct literal must add it (`0.0` for normal
  text). This follows the precedent of `font_tag` in 1.16.0; the Python and
  Node bindings are unaffected.
- Rust: `PageMarkdown` gained the required public field `links`, so code that
  builds a `PageMarkdown` with a struct literal must add it (`Vec::new()` for
  a page with no annotations, or when `MarkdownOptions::include_links` is
  off). Reading a `PageMarkdown` the library returned is unaffected, as are
  the Python and Node bindings. Same precedent as `TextItem` above: the struct
  stays constructible rather than becoming `#[non_exhaustive]`.
- Snapshot fixtures `thermo-freon12` and `shannon-entropy-p1-2` updated for
  the corrected script handling (`Freon<sup>®</sup>`, `V<sub>f</sub>`,
  `2<sup>N</sup>`, `¹Nyquist`, `log<sub>b</sub> a`).
