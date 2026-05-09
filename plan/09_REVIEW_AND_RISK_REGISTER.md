# Review and Risk Register — Semantic PDF Diff Engine

## Review verdict

The idea is technically solid and more differentiated than a normal PDF text diff, but the original plan needed one major correction: **do not equate the first MVP vertical slice with broad real-world PDF support**.

A realistic implementation should ship in stages:

1. **Vertical slice:** controlled PDFs, classic xref, Flate streams, `/ToUnicode`, paragraph diff.
2. **Compatibility gate:** xref streams, object streams, resource limits, corpus metrics.
3. **Semantic expansion:** tagged PDF, better layout, tables, annotations, object-level diff.

## Key corrections made

- Split `vertical-slice` and `public-alpha` readiness.
- Added xref stream and object stream support as a compatibility gate, not a late optional feature.
- Added parser safety and resource limits.
- Tightened claims around `PDF 1.4–2.0` support.
- Reframed legal/business severity as caller-provided classification, not a hard-coded engine decision.
- Reserved `Critical` severity for caller-provided/domain classifiers; the default engine classifier remains neutral.
- Added deterministic identity rules for semantic nodes and report output.
- Added compatibility-gate tests for modern PDFs.
- Updated the agent guide to use Rust 2024 with MSRV 1.85+ unless compatibility requires otherwise.

## Highest technical risks

| Risk | Impact | Mitigation |
|---|---:|---|
| Modern PDFs use xref streams/object streams | High | Implement compatibility gate before public alpha |
| Text extraction without `/ToUnicode` is unreliable | High | Support selected fallback encodings, emit confidence diagnostics |
| Glyph positions differ from real renderers | Medium/High | Treat geometry as approximate, compare with optional reference tools |
| Semantic layout heuristics produce false confidence | High | Confidence scores, `UnknownBlock`, golden fixtures, corpus review |
| PDF inputs can be hostile | High | Resource limits, fuzzing, decompression bounds, recursion caps |
| Agents change IR inconsistently | Medium | Golden snapshots and explicit IR-change rule |
| Object-level differences create noise | Medium | Keep object diff separate from semantic diff |
| “Critical” severity implies legal judgement | Medium/High | Default neutral severity; domain classifiers are caller-provided |

## MVP go/no-go checklist

The project is ready for a first internal demo when:

- synthetic fixtures parse end-to-end;
- old/new paragraph diffs produce stable JSON;
- text positions are available for changed blocks;
- diagnostics are included in output;
- no malformed fixture panics.

The project is ready for public alpha only when:

- xref streams are supported;
- object streams are supported;
- resource limits are enforced;
- a curated real-world corpus report exists;
- unsupported features produce stable diagnostic codes;
- docs clearly state what is and is not supported.

## Recommended first implementation path

1. Create workspace, shared types, shared diagnostics, and baseline resource limits.
2. Build deterministic PDF fixture generator.
3. Implement classic parser and page tree.
4. Implement content tokenizer and text operator interpreter.
5. Extract text with `/ToUnicode` and approximate positions.
6. Build paragraph blocks and semantic anchors.
7. Diff semantic blocks and emit JSON.
8. Add resource limits and hostile tests.
9. Implement xref streams and object streams.
10. Start corpus runner and compatibility metrics.

## Strategic conclusion

This is a good project if the first product is positioned as an **AI-ready semantic diff engine**, not as a general full PDF engine. The differentiator is evidence-preserving semantic diff output: stable node IDs, normalized text, bounding boxes, provenance, confidence, and diagnostics. The risky part is PDF compatibility, so the plan must force compatibility gates early instead of hiding them in a later roadmap.
