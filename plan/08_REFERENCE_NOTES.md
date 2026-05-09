# Reference Notes

## Standards and concepts to study

- ISO 32000-2 / PDF 2.0: object model, page tree, content streams, fonts, structure tree, annotations, metadata. Treat PDF 2.0 support as a compatibility target, not a claim until tested.
- Tagged PDF: semantic structure, logical reading order, headings, paragraphs, lists, tables, figures.
- PDF/UA: accessibility-oriented semantic requirements and tagged content expectations.
- CMaps and `/ToUnicode`: text extraction correctness.
- Xref streams and object streams: required for many modern PDFs.
- Resource-limit design for hostile binary inputs.
- PDF content stream operators: text and graphics state handling.

## Useful implementation references

These are useful as references or optional dev-only comparison targets. They should not be used as core dependencies if the project goal is to build the PDF engine from scratch.

- Apache PDFBox: mature Java library with text extraction and PDF manipulation capabilities.
- PDFium: mature C++ PDF engine used by Chromium, useful as a reference for rendering/extraction behavior.
- `pdfium-render`: Rust wrapper around PDFium, useful only as optional dev/reference tooling.
- `lopdf`: low-level Rust PDF manipulation crate, useful to study API shape and edge cases, not as a core dependency.
- `pdf-rs` / `pdf` crates: useful to study parser coverage and public API design.

## Internal references to maintain

Create these docs inside the repository as implementation progresses:

```text
docs/pdf_parser_notes.md
docs/content_stream_operator_coverage.md
docs/font_and_tounicode_coverage.md
docs/semantic_heuristics.md
docs/diff_algorithm_notes.md
docs/diagnostic_codes.md
docs/corpus_results.md
docs/resource_limits.md
docs/compatibility_gate.md
```

## Diagnostic coverage checklist

- unsupported xref stream;
- unsupported object stream;
- unsupported filter;
- encrypted PDF;
- missing root catalog;
- missing page tree;
- malformed page tree;
- missing contents;
- malformed content stream;
- unknown content operator;
- missing font resource;
- unsupported font subtype;
- missing ToUnicode;
- malformed ToUnicode CMap;
- extraction produced no text;
- structure tree missing;
- structure tree malformed;
- MCID mapping failed.

## Research questions

1. How much semantic quality can be achieved before full rendering?
2. Which layout heuristics are stable across invoices/contracts/reports?
3. Can semantic anchors survive line wrapping changes?
4. How reliable is moved-block detection across page breaks?
5. How should the engine distinguish “text moved” from “delete + insert”?
6. How should low-confidence extraction affect severity?
7. Can table candidate detection be useful without full ruled-line rendering?
8. How should tagged PDF and visual heuristics be merged when they disagree?

## Recommended reading order for agents

1. `00_README_PLAN.md`
2. `01_ARCHITECTURE.md`
3. `02_DATA_MODEL_AND_DIFF_IR.md`
4. `03_PARALLEL_WORKSTREAMS.md`
5. `04_MILESTONES_AND_TICKETS.md`
6. `05_TESTING_STRATEGY.md`
7. `06_AGENT_EXECUTION_GUIDE.md`
8. `07_ROADMAP.md`
