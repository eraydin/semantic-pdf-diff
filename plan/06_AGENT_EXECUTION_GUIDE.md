# AI Agent Execution Guide

## 1. Mission

Implement `Semantic PDF Diff Engine` in Rust as a layered, test-first engine. The first usable target is a CLI and library that produce stable JSON semantic diffs between two digitally generated PDFs.

## 2. Non-negotiable constraints

- Do not use third-party PDF libraries in core implementation.
- Do not start with a GUI.
- Do not build a full renderer in MVP.
- Do not hide unsupported PDF features; emit diagnostics.
- Do not discard provenance.
- Do not introduce nondeterministic output.
- Do not claim broad real-world PDF compatibility until xref streams/object streams and corpus metrics exist.
- Do not parse untrusted PDF streams without resource limits.
- Do not couple diff logic directly to raw PDF object internals.

## 3. Coding standards

Use Rust 2024 for new code unless compatibility constraints require Rust 2021. Recommended MSRV: Rust 1.85 or newer, because Rust 2024 is stable there.

Recommended dependencies:

```toml
[workspace.dependencies]
thiserror = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
flate2 = "1"
unicode-normalization = "0.1"
smallvec = "1"
rayon = { version = "1", optional = true }
clap = { version = "4", features = ["derive"] }
insta = "1"
proptest = "1"
criterion = "0.8"
```

PDF-specific crates are not allowed in core crates.

Shared public types belong in `spdfdiff_types`. Do not duplicate public geometry, provenance, diagnostic, ID, or report-facing IR structs in downstream crates.

Use the repo-local `spdfdiff-orchestrator` skill when coordinating parallel agents, changing shared public APIs, or integrating cross-crate work. The orchestrator owns sequencing and boundary checks; specialist agents own implementation inside their assigned crates or folders.

## 4. Branching model for agents

Suggested branches:

```text
agent/parser-core
agent/fixture-generator
agent/content-tokenizer
agent/text-extraction
agent/semantic-layout
agent/diff-core
agent/report-cli
agent/testing-fuzzing
agent/tagged-pdf
agent/object-diff
```

Each branch should include:

- code;
- tests;
- snapshots if relevant;
- updated docs if public API changes.

## 5. PR size guideline

Good PR:

- one module or one behavior;
- under 800 changed lines when possible;
- includes tests;
- explains unsupported cases.

Bad PR:

- parser + text extraction + report UI in one change;
- public IR changes without snapshots;
- panic-based parser behavior;
- untested heuristics.

## 6. Shared terminology

Use these terms consistently:

| Term | Meaning |
|---|---|
| Object graph | Parsed PDF indirect objects and references |
| Content program | Tokenized/interpreted page drawing operations |
| Glyph token | One decoded glyph or raw text unit with position |
| Text run | Sequence of glyphs with common style/line context |
| Layout block | Visual block inferred from positioned text/rules/images |
| Semantic node | Meaning-oriented node: paragraph, heading, table, figure, etc. |
| Anchor | Stable matching signature for a semantic node |
| Diff document | Final machine-readable result |
| Diagnostic | Structured explanation of unsupported or degraded behavior |

## 7. Compatibility honesty rule

Agents must label work as one of:

- `vertical-slice`: controlled fixtures only;
- `compatibility-gate`: modern PDF constructs such as xref/object streams;
- `public-alpha`: corpus-backed behavior with documented limitations.

A feature is not public-alpha-ready until it has tests, diagnostics, and corpus evidence.

## 8. Minimum useful vertical slice

Agents should aim to integrate this vertical slice early:

```text
minimal_old.pdf + minimal_new.pdf
  -> parse object graph
  -> resolve page
  -> parse content stream
  -> extract text runs
  -> build paragraph blocks
  -> diff text blocks
  -> emit JSON
```

The first vertical slice can support only:

- one page;
- one font;
- one content stream;
- simple `Tj` text;
- no compression or only Flate;
- paragraph insert/delete/modify.

After that, widen feature coverage.

## 9. Integration contract between crates

### `pdf_core` -> `pdf_content`

Must provide:

- page list;
- page resources;
- decoded content stream bytes;
- object provenance.

### `pdf_content` -> `pdf_text`

Must provide:

- ordered content operations;
- text-state changes;
- raw shown text bytes;
- transformation matrices;
- content operation indices.

### `pdf_text` -> `pdf_semantic`

Must provide:

- text runs;
- glyphs;
- bounding boxes;
- font/style hints;
- page index;
- provenance.

### `pdf_semantic` -> `diff_core`

Must provide:

- semantic nodes;
- normalized text;
- page spans;
- bounding boxes;
- anchors;
- confidence;
- diagnostics.

### `diff_core` -> `diff_report`

Must provide:

- stable `DiffDocument`;
- changes with evidence;
- summary counts;
- diagnostics.

## 10. Heuristic policy

Heuristics are allowed, but each heuristic must expose confidence.

Examples:

```rust
pub struct HeuristicResult<T> {
    pub value: T,
    pub confidence: f32,
    pub reasons: Vec<String>,
}
```

Rules:

- Confidence must be between `0.0` and `1.0`.
- Low-confidence semantic guesses should use `UnknownBlock` or `Candidate` kinds.
- Reports should expose low-confidence changes.

## 11. Diagnostics policy

Every unsupported feature needs a stable code.

Example:

```rust
Diagnostic::warning(
    "MISSING_TOUNICODE",
    "Font F3 has no ToUnicode map; text extraction may be incomplete",
)
```

Do not use vague diagnostics like `Something went wrong`.

## 12. Snapshot policy

Snapshots must be deterministic.

Do not include:

- absolute file paths;
- timestamps;
- random IDs;
- nondeterministic hash-map ordering;
- machine-specific timing.

## 13. Suggested first 12 commits

1. Workspace and crate skeleton, including `spdfdiff_types`.
2. Shared diagnostics, error type, and baseline `ResourceLimits`.
3. Test fixture generator with minimal one-page PDF.
4. Primitive PDF object parser.
5. Xref/trailer parser.
6. Page tree resolver.
7. Content stream tokenizer for text operators.
8. Text extraction for simple `Tj` with ToUnicode.
9. Semantic paragraph block builder.
10. Diff engine for insert/delete/modify and JSON/Markdown CLI output.
11. Resource limits and hostile malformed fixtures.
12. Xref-stream/object-stream compatibility spike.

## 14. Definition of done for agent tickets

A ticket is done when:

- code compiles;
- unit tests pass;
- relevant snapshots updated;
- public API documented;
- unsupported cases produce diagnostics;
- no unrelated files changed;
- integration boundary remains stable.
