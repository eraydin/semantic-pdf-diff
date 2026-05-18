# semantic-pdf-diff fuzz targets

Standalone fuzz targets for hostile parser and content-tokenizer inputs.

Install `cargo-fuzz`, then run from the repository root:

```powershell
cargo fuzz run parse_pdf
cargo fuzz run parse_object
cargo fuzz run parse_content_stream
```

The targets use intentionally small `ResourceLimits` so malformed inputs cannot
exercise unbounded allocations before the parser returns an error or diagnostic.
