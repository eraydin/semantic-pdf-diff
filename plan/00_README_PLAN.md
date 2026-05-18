# Semantic PDF Diff Engine — Implementation Plan

## 1. Product definition

`Semantic PDF Diff Engine` is a Rust library and CLI that compares two PDF files by document meaning, not only by bytes, page images, or raw extracted text.

The engine converts each PDF into a normalized intermediate representation, then reports changes at multiple layers:

1. **Structural diff** — pages, headings, paragraphs, lists, tables, figures, annotations, forms, metadata.
2. **Textual diff** — inserted, deleted, moved, or modified text with page and bounding-box evidence.
3. **Layout diff** — position, reading order, block movement, column changes, table geometry, figure placement.
4. **Object-level diff** — PDF object, stream, resource, font, image, annotation, outline, and metadata changes.
5. **AI-ready semantic summary** — machine-readable JSON with stable IDs, evidence spans, severity, and confidence.

The goal is not to build a full PDF editor first. The goal is to build a reliable document-understanding core that can later become part of a PDF editor, review tool, legal diff system, or AI document workflow.

### Important scope correction

The implementation should separate the **first vertical slice** from the **public MVP**:

- **Vertical slice / v0.1.0:** classic xref-table PDFs, Flate content streams, simple fonts, `/ToUnicode`, block-level text diff, stable JSON, and simple Markdown output.
- **Public MVP / alpha:** enough modern-PDF compatibility to process a meaningful real-world corpus, especially xref streams and object streams, or the product will overpromise.

Do not claim broad `PDF 1.4–2.0` support until xref streams, object streams, and common font cases are implemented or explicitly diagnosed.

## 2. Innovation angle

Most PDF diff tools fall into one of these categories:

- pixel/image diff;
- raw text diff;
- object-level PDF diff;
- visual side-by-side comparison.

This engine should combine all of them into a **layered semantic diff**:

```text
PDF bytes
  -> PDF object graph
  -> page/content stream model
  -> glyph and drawing operation stream
  -> layout blocks
  -> semantic document tree
  -> stable semantic anchors
  -> layered diff output
```

The unique part is the `SemanticDiffIR`: every reported change includes:

- stable semantic node ID;
- before/after page number;
- before/after bounding boxes;
- normalized text evidence;
- raw PDF object provenance where possible;
- confidence score;
- reason code explaining how the engine matched or changed a node.

This makes the result useful for AI agents because agents can reason over exact evidence instead of vague screenshots or lossy extracted text.

## 3. MVP scope

The MVP should support common digitally generated PDFs first. Scanned/OCR-only
PDFs are supported only through the external OCR adapter path and should not be
treated as broad visual-rendering compatibility.

### In scope for MVP

- Parse non-encrypted, digitally generated PDFs in a controlled subset.
- Parse classic xref tables and trailers for the first vertical slice.
- Add xref stream and object stream support before calling the product a public real-world MVP.
- Parse indirect objects, dictionaries, arrays, names, strings, streams.
- Decode Flate streams.
- Resolve page tree.
- Read page resources.
- Interpret enough text operators to extract glyphs and text positions.
- Use `/ToUnicode` CMaps where available.
- Build page-level text blocks using layout heuristics.
- Build a best-effort semantic tree: pages, blocks, paragraphs, heading candidates, table candidates, figures/placeholders.
- Diff normalized text blocks and semantic blocks.
- Detect additions, deletions, edits, moves, and layout-only changes.
- Emit stable JSON and simple Markdown reports for the vertical slice.
- Add HTML reporting with deterministic inline SVG overlays during the
  layout-aware v0.3 phase.
- Provide a CLI: `spdfdiff diff old.pdf new.pdf --format json|md|html`.

### Out of scope for MVP

- Perfect PDF rendering.
- Full JavaScript/action support.
- Full encryption support.
- Full Type 0/CID font coverage beyond common `/ToUnicode` cases and selected fallback encodings.
- Embedded OCR models or broad scanned-document compatibility beyond the
  external OCR adapter.
- Digital signature validation.
- Full table reconstruction in arbitrary PDFs.
- Editing/writing modified PDFs.
- AI/LLM inference inside the engine.
- Legal/business judgement such as declaring a clause change legally material without a caller-provided classifier.

## 4. Design principles

1. **No dependency on third-party PDF libraries for the core engine.** General-purpose crates are allowed: `serde`, `thiserror`, `flate2`, `unicode-normalization`, `memmap2`, `smallvec`, `rayon`, `insta`, `proptest`, etc.
2. **Every semantic result must preserve provenance.** A text block should point back to page, content stream, operator range, and source PDF object IDs where possible.
3. **Layered fallback.** Prefer tagged PDF structure when available; otherwise use layout heuristics; otherwise fall back to text sequence diff; optionally fall back to visual diff later.
4. **Deterministic output.** Same inputs must produce byte-identical JSON output unless explicitly configured otherwise.
5. **AI-agent friendly tasks.** Every module must have clear boundaries, fixtures, acceptance tests, and no hidden cross-agent state.
6. **Fail soft.** Invalid or unsupported PDF features should produce partial output plus diagnostics, not a total crash.

## 5. Target users

- Developers building document review systems.
- Legal/compliance teams comparing contracts and generated reports.
- AI workflow builders who need precise change evidence.
- PDF editor developers who need semantic document intelligence.
- CI systems that validate generated PDF output.

## 6. Expected first demo

Command:

```bash
spdfdiff diff examples/contract_v1.pdf examples/contract_v2.pdf --format json --output diff.json
```

Output should show:

- changed paragraphs;
- moved clauses;
- changed dates, prices, party names, or section titles as candidate value changes, not as legal conclusions;
- layout-only changes separately from textual changes;
- page and bounding-box evidence in JSON for changed blocks;
- simple Markdown summary for human review;
- HTML/SVG evidence overlays in the layout-aware report phase;
- JSON output that an AI agent can consume.

## 7. Success criteria for MVP

The MVP is acceptable when it can:

- parse and diff at least 100 synthetic PDFs generated by the internal test generator;
- parse and diff at least 30 curated real-world digitally generated PDFs after the compatibility gate; before that, report unsupported modern-PDF features clearly;
- extract text with page coordinates for common fonts using `/ToUnicode`;
- produce stable JSON snapshots for golden tests;
- identify block-level add/delete/edit/move changes with reasonable precision;
- complete a 50-page vs 50-page diff in under 5 seconds on a normal laptop for text/layout-only mode on supported PDFs;
- return useful diagnostics for unsupported features.

## 8. Repository structure

```text
semantic-pdf-diff/
  Cargo.toml
  AGENTS.md
  crates/
    spdfdiff_types/       # shared IDs, geometry, provenance, diagnostics, and report-facing IR
    pdf_core/             # low-level parser, object graph, xref, streams
    pdf_content/          # content stream tokenizer and operator interpreter
    pdf_text/             # font decoding, ToUnicode, glyph extraction
    pdf_semantic/         # layout segmentation and semantic tree builder
    diff_core/            # matching, edit distance, move detection, severity
    diff_report/          # JSON, Markdown, AI JSON, HTML reports, SVG overlays
    spdfdiff_cli/         # command-line binary
  tests/
    fixtures/
      synthetic/
      real_world/
      malformed/
    golden/
  tools/
    pdf_fixture_gen/      # minimal PDF generator for tests
    corpus_runner/        # internal batch runner used by `spdfdiff corpus`
  docs/
    architecture.md
    diff_ir.md
    testing.md
    roadmap.md
```

## 9. Recommended execution order

1. Build workspace, shared types, diagnostics, and baseline resource limits.
2. Build low-level parser and object resolver.
3. Build synthetic PDF fixture generator.
4. Build page tree resolver.
5. Build content stream tokenizer.
6. Build text extraction with positions.
7. Build layout blocks.
8. Build semantic tree.
9. Build block matching and diff output.
10. Build JSON and Markdown reports, then basic HTML.
11. Add fuzzing, corpus runner, benchmarks, and unsupported-feature diagnostics.

## 10. Final note for agents

Do not start by building a GUI, editor, or full renderer. Start by making the engine produce a high-quality `DiffDocument` JSON object. A future desktop app or web UI can be built on top of that.
