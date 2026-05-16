use criterion::{Criterion, criterion_group, criterion_main};
use diff_core::{DiffConfig, diff_semantic_documents};
use pdf_semantic::{SemanticAnchor, SemanticDocument, SemanticNode, SemanticNodeKind};
use spdfdiff_types::Provenance;

fn fifty_page_semantic_document(fingerprint: &str, changed: bool) -> SemanticDocument {
    let mut nodes = Vec::new();
    for page_index in 0..50 {
        for paragraph_index in 0..4 {
            let text = if changed && page_index == 25 && paragraph_index == 2 {
                "Benchmark paragraph with revised content".to_owned()
            } else {
                format!("Benchmark paragraph page {page_index} block {paragraph_index}")
            };
            nodes.push(SemanticNode {
                id: format!("n{page_index:02}_{paragraph_index:02}"),
                kind: SemanticNodeKind::Paragraph,
                page_index,
                bbox: None,
                normalized_text: Some(text),
                anchor: SemanticAnchor::unknown(),
                source: vec![Provenance::unknown()],
                confidence: 1.0,
            });
        }
    }
    SemanticDocument {
        fingerprint: fingerprint.to_owned(),
        nodes,
        diagnostics: Vec::new(),
    }
}

fn bench_fifty_page_diff(c: &mut Criterion) {
    let old = fifty_page_semantic_document("old", false);
    let new = fifty_page_semantic_document("new", true);
    c.bench_function("diff_50_page_semantic_documents", |b| {
        b.iter(|| diff_semantic_documents(&old, &new, DiffConfig::default()));
    });
}

criterion_group!(benches, bench_fifty_page_diff);
criterion_main!(benches);
