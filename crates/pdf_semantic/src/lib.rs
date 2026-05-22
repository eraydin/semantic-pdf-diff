use pdf_text::TextRun;
use spdfdiff_types::{Diagnostic, ObjectId, Provenance, Rect};

const LINE_BASELINE_TOLERANCE: f32 = 3.0;
const SAME_LINE_MAX_GAP: f32 = 260.0;
const PARAGRAPH_GAP_MULTIPLIER: f32 = 1.8;
const COLUMN_X_GAP: f32 = 260.0;
const COLUMN_MIN_VERTICAL_OVERLAP: f32 = 12.0;
const REPEATED_POSITION_TOLERANCE: f32 = 12.0;
const PAGE_EDGE_BAND: f32 = 80.0;
const TABLE_COLUMN_TOLERANCE: f32 = 8.0;

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticDocument {
    pub fingerprint: String,
    pub nodes: Vec<SemanticNode>,
    pub diagnostics: Vec<Diagnostic>,
    pub tagged_structure: Option<TaggedStructureSummary>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticNode {
    pub id: String,
    pub kind: SemanticNodeKind,
    pub page_index: usize,
    pub bbox: Option<Rect>,
    pub normalized_text: Option<String>,
    pub table: Option<TableStructure>,
    pub anchor: SemanticAnchor,
    pub source: Vec<Provenance>,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticAnchor {
    pub strong_text_hash: String,
    pub weak_text_signature: String,
    pub geometry_bucket: String,
    pub heading_context: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TaggedStructureSummary {
    pub root_object_id: Option<ObjectId>,
    pub role_map: Vec<TaggedRoleMapSummary>,
    pub element_count: usize,
    pub mcid_count: usize,
    pub parent_tree_entries: usize,
    pub structure_types: Vec<String>,
    pub elements: Vec<TaggedStructureElementSummary>,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaggedRoleMapSummary {
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TaggedStructureElementSummary {
    pub structure_type: String,
    pub mapped_structure_type: Option<String>,
    pub mcids: Vec<usize>,
    pub children: Vec<TaggedStructureElementSummary>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableStructure {
    pub rows: Vec<TableRow>,
    pub column_x_positions: Vec<f32>,
    pub border_hints: Vec<TableBorderHint>,
    pub repeated_header_rows: Vec<usize>,
    pub continuation_group: Option<String>,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableBorderHint {
    pub page_index: usize,
    pub bbox: Rect,
    pub source: Vec<Provenance>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableRow {
    pub cells: Vec<TableCell>,
    pub bbox: Rect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableCell {
    pub text: String,
    pub bbox: Rect,
    pub source: Vec<Provenance>,
    pub row_span: usize,
    pub column_span: usize,
    pub is_placeholder: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticNodeKind {
    Page,
    HeadingCandidate,
    Paragraph,
    ListCandidate,
    TableCandidate,
    FigureCandidate,
    HeaderCandidate,
    FooterCandidate,
    PageTemplateCandidate,
    UnknownBlock,
}

#[derive(Debug, Clone)]
struct TextLine {
    page_index: usize,
    bbox: Rect,
    text: String,
    source: Vec<Provenance>,
    cells: Vec<TableCell>,
}

#[derive(Debug, Clone)]
struct TableRowDraft {
    anchor_y: f32,
    row: TableRow,
}

#[derive(Debug, Clone)]
struct ColumnBand {
    anchor_x: f32,
    line_count: usize,
    min_y: f32,
    max_y: f32,
}

#[derive(Debug, Clone)]
struct PageVerticalProfile {
    min_y: f32,
    max_y: f32,
}

#[derive(Debug, Clone)]
struct RepeatedLayoutSignature {
    template_signature: String,
    page_index: usize,
    x: f32,
    y: f32,
}

#[derive(Debug, Clone)]
struct RepeatedTableHeaderCandidate {
    node_index: usize,
    page_index: usize,
    header_signature: String,
    column_x_positions: Vec<f32>,
}

impl TextLine {
    fn height(&self) -> f32 {
        self.bbox.height().max(1.0)
    }
}

#[must_use]
pub fn build_semantic_document(
    fingerprint: impl Into<String>,
    runs: &[TextRun],
    diagnostics: Vec<Diagnostic>,
) -> SemanticDocument {
    build_semantic_document_with_table_hints(fingerprint, runs, diagnostics, Vec::new())
}

#[must_use]
pub fn build_semantic_document_with_table_hints(
    fingerprint: impl Into<String>,
    runs: &[TextRun],
    diagnostics: Vec<Diagnostic>,
    table_border_hints: Vec<TableBorderHint>,
) -> SemanticDocument {
    build_semantic_document_with_tagged_structure_and_table_hints(
        fingerprint,
        runs,
        diagnostics,
        None,
        table_border_hints,
    )
}

#[must_use]
pub fn build_semantic_document_with_tagged_structure(
    fingerprint: impl Into<String>,
    runs: &[TextRun],
    diagnostics: Vec<Diagnostic>,
    tagged_structure: Option<TaggedStructureSummary>,
) -> SemanticDocument {
    build_semantic_document_with_tagged_structure_and_table_hints(
        fingerprint,
        runs,
        diagnostics,
        tagged_structure,
        Vec::new(),
    )
}

#[must_use]
pub fn build_semantic_document_with_tagged_structure_and_table_hints(
    fingerprint: impl Into<String>,
    runs: &[TextRun],
    diagnostics: Vec<Diagnostic>,
    tagged_structure: Option<TaggedStructureSummary>,
    table_border_hints: Vec<TableBorderHint>,
) -> SemanticDocument {
    let tagged_nodes = tagged_structure
        .as_ref()
        .map_or_else(Vec::new, |structure| {
            build_tagged_nodes_from_runs(runs, &structure.elements)
        });
    let mut nodes = if tagged_nodes.is_empty() {
        build_layout_nodes(runs)
    } else {
        let mapped_mcids = tagged_structure
            .as_ref()
            .map_or_else(Vec::new, |structure| {
                structure_mcid_list(&structure.elements)
            });
        let unmatched_runs = runs
            .iter()
            .filter(|run| {
                run.marked_content
                    .as_ref()
                    .and_then(|marked| marked.mcid)
                    .is_none_or(|mcid| !mapped_mcids.contains(&mcid))
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut nodes = tagged_nodes;
        nodes.extend(build_layout_nodes(&unmatched_runs));
        reindex_nodes(&mut nodes);
        nodes
    };
    attach_table_border_hints(&mut nodes, &table_border_hints);
    assign_semantic_anchors(&mut nodes);

    SemanticDocument {
        fingerprint: fingerprint.into(),
        nodes,
        diagnostics,
        tagged_structure,
    }
}

fn build_layout_nodes(runs: &[TextRun]) -> Vec<SemanticNode> {
    let lines = cluster_lines(runs);
    let ordered_lines = order_lines_for_flow(&lines);
    let mut nodes = cluster_paragraphs(&ordered_lines);
    classify_heading_candidates(&mut nodes);
    classify_repeated_page_template_candidates(&mut nodes);
    classify_table_candidates(&mut nodes);
    classify_repeated_table_headers(&mut nodes);
    classify_list_candidates(&mut nodes);
    nodes
}

fn build_tagged_nodes_from_runs(
    runs: &[TextRun],
    elements: &[TaggedStructureElementSummary],
) -> Vec<SemanticNode> {
    let mut nodes = Vec::new();
    append_tagged_nodes_from_elements(runs, elements, &mut nodes);
    nodes
}

fn append_tagged_nodes_from_elements(
    runs: &[TextRun],
    elements: &[TaggedStructureElementSummary],
    nodes: &mut Vec<SemanticNode>,
) {
    for element in elements {
        if let Some(node) = tagged_element_to_node(nodes.len(), runs, element) {
            nodes.push(node);
        }
        append_tagged_nodes_from_elements(runs, &element.children, nodes);
    }
}

fn tagged_element_to_node(
    index: usize,
    runs: &[TextRun],
    element: &TaggedStructureElementSummary,
) -> Option<SemanticNode> {
    if element.mcids.is_empty() {
        return None;
    }
    let element_runs = runs
        .iter()
        .filter(|run| {
            run.marked_content
                .as_ref()
                .and_then(|marked| marked.mcid)
                .is_some_and(|mcid| element.mcids.contains(&mcid))
        })
        .collect::<Vec<_>>();
    if element_runs.is_empty() {
        return None;
    }
    let mut text = String::new();
    let mut source = Vec::new();
    let mut bbox = element_runs[0].bbox;
    let mut cells = Vec::new();
    for run in element_runs {
        append_text(&mut text, &run.normalized_text);
        bbox = union_rect(bbox, run.bbox);
        source.push(run.source.clone());
        cells.push(table_cell_from_run(run));
    }
    let kind = tagged_structure_kind(effective_tagged_structure_type(element));
    let table = if kind == SemanticNodeKind::TableCandidate {
        table_structure_from_cells(&cells)
    } else {
        None
    };
    Some(SemanticNode {
        id: format!("tag{index:04}"),
        kind,
        page_index: source
            .first()
            .and_then(|source| source.page_index)
            .unwrap_or(0),
        bbox: Some(bbox),
        normalized_text: Some(text),
        table,
        anchor: SemanticAnchor::unknown(),
        source,
        confidence: 0.9,
    })
}

fn tagged_structure_kind(structure_type: &str) -> SemanticNodeKind {
    match structure_type {
        "H" | "H1" | "H2" | "H3" | "H4" | "H5" | "H6" => SemanticNodeKind::HeadingCandidate,
        "L" | "LI" | "Lbl" | "LBody" => SemanticNodeKind::ListCandidate,
        "Table" | "TR" | "TH" | "TD" | "THead" | "TBody" | "TFoot" => {
            SemanticNodeKind::TableCandidate
        }
        "Figure" | "Formula" | "Form" => SemanticNodeKind::FigureCandidate,
        "Header" => SemanticNodeKind::HeaderCandidate,
        "Footer" => SemanticNodeKind::FooterCandidate,
        "P" | "Span" => SemanticNodeKind::Paragraph,
        _ => SemanticNodeKind::UnknownBlock,
    }
}

fn effective_tagged_structure_type(element: &TaggedStructureElementSummary) -> &str {
    element
        .mapped_structure_type
        .as_deref()
        .unwrap_or(&element.structure_type)
}

fn reindex_nodes(nodes: &mut [SemanticNode]) {
    for (index, node) in nodes.iter_mut().enumerate() {
        node.id = format!("n{index:04}");
    }
}

fn structure_mcid_list(elements: &[TaggedStructureElementSummary]) -> Vec<usize> {
    let mut mcids = Vec::new();
    for element in elements {
        mcids.extend(element.mcids.iter().copied());
        mcids.extend(structure_mcid_list(&element.children));
    }
    mcids.sort_unstable();
    mcids.dedup();
    mcids
}

impl SemanticDocument {
    #[must_use]
    pub fn with_tagged_structure(mut self, tagged_structure: TaggedStructureSummary) -> Self {
        self.tagged_structure = Some(tagged_structure);
        self
    }
}

fn cluster_lines(runs: &[TextRun]) -> Vec<TextLine> {
    let mut ordered_runs = runs
        .iter()
        .filter(|run| !run.normalized_text.is_empty())
        .collect::<Vec<_>>();
    ordered_runs.sort_by(|left, right| {
        let left_page = left.source.page_index.unwrap_or(0);
        let right_page = right.source.page_index.unwrap_or(0);
        left_page
            .cmp(&right_page)
            .then_with(|| {
                right
                    .bbox
                    .y0
                    .partial_cmp(&left.bbox.y0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                left.bbox
                    .x0
                    .partial_cmp(&right.bbox.x0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    let mut lines: Vec<TextLine> = Vec::new();
    for run in ordered_runs {
        let page_index = run.source.page_index.unwrap_or(0);
        if let Some(line) = lines.iter_mut().find(|line| {
            line.page_index == page_index
                && (line.bbox.y0 - run.bbox.y0).abs() <= LINE_BASELINE_TOLERANCE
                && can_merge_run_into_line(line, run.bbox)
        }) {
            append_text(&mut line.text, &run.normalized_text);
            line.bbox = union_rect(line.bbox, run.bbox);
            line.source.push(run.source.clone());
            line.cells.push(table_cell_from_run(run));
        } else {
            lines.push(TextLine {
                page_index,
                bbox: run.bbox,
                text: run.normalized_text.clone(),
                source: vec![run.source.clone()],
                cells: vec![table_cell_from_run(run)],
            });
        }
    }
    lines
}

fn can_merge_run_into_line(line: &TextLine, run_bbox: Rect) -> bool {
    let horizontal_gap = if run_bbox.x0 >= line.bbox.x1 {
        run_bbox.x0 - line.bbox.x1
    } else if line.bbox.x0 >= run_bbox.x1 {
        line.bbox.x0 - run_bbox.x1
    } else {
        0.0
    };
    horizontal_gap <= SAME_LINE_MAX_GAP
}

fn order_lines_for_flow(lines: &[TextLine]) -> Vec<TextLine> {
    let mut page_indices = lines.iter().map(|line| line.page_index).collect::<Vec<_>>();
    page_indices.sort_unstable();
    page_indices.dedup();

    let mut ordered = Vec::new();
    for page_index in page_indices {
        let page_lines = lines
            .iter()
            .filter(|line| line.page_index == page_index)
            .cloned()
            .collect::<Vec<_>>();
        let columns = infer_column_bands(&page_lines);
        let mut indexed_lines = page_lines
            .into_iter()
            .map(|line| {
                let column_index = column_index_for_line(&line, &columns);
                (column_index, line)
            })
            .collect::<Vec<_>>();
        indexed_lines.sort_by(|(left_column, left), (right_column, right)| {
            left_column
                .cmp(right_column)
                .then_with(|| {
                    right
                        .bbox
                        .y0
                        .partial_cmp(&left.bbox.y0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| {
                    left.bbox
                        .x0
                        .partial_cmp(&right.bbox.x0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        ordered.extend(indexed_lines.into_iter().map(|(_, line)| line));
    }
    ordered
}

fn infer_column_bands(lines: &[TextLine]) -> Vec<ColumnBand> {
    if lines.len() < 4 {
        return vec![ColumnBand {
            anchor_x: 0.0,
            line_count: lines.len(),
            min_y: 0.0,
            max_y: 0.0,
        }];
    }
    let mut x_positions = lines.iter().map(|line| line.bbox.x0).collect::<Vec<_>>();
    x_positions.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));

    let mut bands = Vec::new();
    let mut current_sum = 0.0;
    let mut current_count = 0usize;
    let mut current_min_y = f32::MAX;
    let mut current_max_y = f32::MIN;
    let mut last_x = x_positions[0];
    for x in x_positions {
        if current_count > 0 && (x - last_x).abs() > COLUMN_X_GAP {
            bands.push(ColumnBand {
                anchor_x: current_sum / current_count as f32,
                line_count: current_count,
                min_y: current_min_y,
                max_y: current_max_y,
            });
            current_sum = 0.0;
            current_count = 0;
            current_min_y = f32::MAX;
            current_max_y = f32::MIN;
        }
        for line in lines
            .iter()
            .filter(|line| (line.bbox.x0 - x).abs() < f32::EPSILON)
        {
            current_min_y = current_min_y.min(line.bbox.y0);
            current_max_y = current_max_y.max(line.bbox.y1);
        }
        current_sum += x;
        current_count += 1;
        last_x = x;
    }
    if current_count > 0 {
        bands.push(ColumnBand {
            anchor_x: current_sum / current_count as f32,
            line_count: current_count,
            min_y: current_min_y,
            max_y: current_max_y,
        });
    }

    if bands.len() == 2
        && bands.iter().all(|band| band.line_count >= 2)
        && column_bands_vertically_overlap(&bands)
    {
        bands
    } else {
        vec![ColumnBand {
            anchor_x: 0.0,
            line_count: lines.len(),
            min_y: 0.0,
            max_y: 0.0,
        }]
    }
}

fn column_bands_vertically_overlap(bands: &[ColumnBand]) -> bool {
    let [left, right] = bands else {
        return false;
    };
    let overlap = left.max_y.min(right.max_y) - left.min_y.max(right.min_y);
    overlap >= COLUMN_MIN_VERTICAL_OVERLAP
}

fn column_index_for_line(line: &TextLine, columns: &[ColumnBand]) -> usize {
    columns
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| {
            (line.bbox.x0 - left.anchor_x)
                .abs()
                .partial_cmp(&(line.bbox.x0 - right.anchor_x).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn cluster_paragraphs(lines: &[TextLine]) -> Vec<SemanticNode> {
    let mut nodes = Vec::new();
    let mut current: Option<TextLine> = None;

    for line in lines {
        if let Some(paragraph) = &mut current {
            let vertical_gap = paragraph.bbox.y0 - line.bbox.y1;
            let gap_limit = paragraph.height().max(line.height()) * PARAGRAPH_GAP_MULTIPLIER;
            let table_overlap =
                vertical_gap < 0.0 && can_merge_overlapping_table_lines(paragraph, line);
            if paragraph.page_index == line.page_index
                && ((vertical_gap >= 0.0 && vertical_gap <= gap_limit) || table_overlap)
            {
                append_text(&mut paragraph.text, &line.text);
                paragraph.bbox = union_rect(paragraph.bbox, line.bbox);
                paragraph.source.extend(line.source.clone());
                paragraph.cells.extend(line.cells.clone());
                continue;
            }

            nodes.push(line_to_node(nodes.len(), paragraph));
        }
        current = Some(line.clone());
    }

    if let Some(paragraph) = &current {
        nodes.push(line_to_node(nodes.len(), paragraph));
    }
    nodes
}

fn can_merge_overlapping_table_lines(upper: &TextLine, lower: &TextLine) -> bool {
    if upper.page_index != lower.page_index || upper.bbox.y0 < lower.bbox.y0 {
        return false;
    }
    let cell_count = upper.cells.len() + lower.cells.len();
    cell_count >= 3
        && upper
            .cells
            .iter()
            .chain(lower.cells.iter())
            .all(is_short_table_cell)
}

fn is_short_table_cell(cell: &TableCell) -> bool {
    cell.text.split_whitespace().count() <= 2 && cell.text.len() <= 32
}

fn line_to_node(index: usize, line: &TextLine) -> SemanticNode {
    SemanticNode {
        id: format!("n{index:04}"),
        kind: SemanticNodeKind::Paragraph,
        page_index: line.page_index,
        bbox: Some(line.bbox),
        normalized_text: Some(line.text.clone()),
        table: table_structure_from_cells(&line.cells),
        anchor: SemanticAnchor::unknown(),
        source: line.source.clone(),
        confidence: 0.7,
    }
}

impl SemanticAnchor {
    #[must_use]
    pub fn unknown() -> Self {
        Self {
            strong_text_hash: "text:0000000000000000".into(),
            weak_text_signature: "weak:0000000000000000".into(),
            geometry_bucket: "page-unknown:x-unknown:y-unknown".into(),
            heading_context: None,
        }
    }
}

fn classify_heading_candidates(nodes: &mut [SemanticNode]) {
    let mut heights = nodes
        .iter()
        .filter_map(|node| node.bbox.map(Rect::height))
        .filter(|height| *height > 0.0)
        .collect::<Vec<_>>();
    if heights.len() < 2 {
        return;
    }
    heights.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let median_height = heights[(heights.len() - 1) / 2];

    for node in nodes {
        let Some(text) = node.normalized_text.as_deref() else {
            continue;
        };
        let Some(bbox) = node.bbox else {
            continue;
        };
        if is_heading_candidate(text, bbox.height(), median_height) {
            node.kind = SemanticNodeKind::HeadingCandidate;
            node.confidence = 0.65;
        }
    }
}

fn classify_repeated_page_template_candidates(nodes: &mut [SemanticNode]) {
    let page_count = nodes
        .iter()
        .map(|node| node.page_index)
        .max()
        .map_or(0, |max_page| max_page + 1);
    if page_count < 2 {
        return;
    }
    let profiles = page_vertical_profiles(nodes);
    let signatures = repeated_layout_signatures(nodes);

    for node in nodes {
        if !matches!(
            node.kind,
            SemanticNodeKind::Paragraph | SemanticNodeKind::HeadingCandidate
        ) {
            continue;
        }
        let Some(text) = node.normalized_text.as_deref() else {
            continue;
        };
        let Some(bbox) = node.bbox else {
            continue;
        };
        let template_signature = repeated_region_signature(text);
        if template_signature.is_empty() {
            continue;
        }
        let repeat_count = signatures
            .iter()
            .filter(|signature| {
                signature.template_signature == template_signature
                    && signature.page_index != node.page_index
                    && (signature.x - bbox.x0).abs() <= REPEATED_POSITION_TOLERANCE
                    && (signature.y - bbox.y0).abs() <= REPEATED_POSITION_TOLERANCE
            })
            .count()
            + 1;
        if repeat_count < 2 {
            continue;
        }
        let Some(profile) = profiles.get(node.page_index) else {
            continue;
        };
        let center_y = (bbox.y0 + bbox.y1) / 2.0;
        let has_page_edge_span = profile.max_y - profile.min_y >= PAGE_EDGE_BAND * 4.0;
        if has_page_edge_span && center_y >= profile.max_y - PAGE_EDGE_BAND {
            node.kind = SemanticNodeKind::HeaderCandidate;
            node.confidence = 0.72;
        } else if has_page_edge_span && center_y <= profile.min_y + PAGE_EDGE_BAND {
            node.kind = SemanticNodeKind::FooterCandidate;
            node.confidence = 0.72;
        } else if page_count >= 3 && repeat_count >= page_count.saturating_sub(1).max(2) {
            node.kind = SemanticNodeKind::PageTemplateCandidate;
            node.confidence = 0.58;
        }
    }
}

fn page_vertical_profiles(nodes: &[SemanticNode]) -> Vec<PageVerticalProfile> {
    let page_count = nodes
        .iter()
        .map(|node| node.page_index)
        .max()
        .map_or(0, |max_page| max_page + 1);
    let mut profiles = vec![
        PageVerticalProfile {
            min_y: f32::MAX,
            max_y: f32::MIN,
        };
        page_count
    ];
    for node in nodes {
        let Some(bbox) = node.bbox else {
            continue;
        };
        let profile = &mut profiles[node.page_index];
        profile.min_y = profile.min_y.min(bbox.y0);
        profile.max_y = profile.max_y.max(bbox.y1);
    }
    for profile in &mut profiles {
        if profile.min_y == f32::MAX {
            profile.min_y = 0.0;
            profile.max_y = 0.0;
        }
    }
    profiles
}

fn repeated_layout_signatures(nodes: &[SemanticNode]) -> Vec<RepeatedLayoutSignature> {
    nodes
        .iter()
        .filter_map(|node| {
            let text = repeated_region_signature(node.normalized_text.as_deref()?);
            let bbox = node.bbox?;
            (!text.is_empty()).then_some(RepeatedLayoutSignature {
                template_signature: text,
                page_index: node.page_index,
                x: bbox.x0,
                y: bbox.y0,
            })
        })
        .collect()
}

fn is_heading_candidate(text: &str, height: f32, median_height: f32) -> bool {
    let text = text.trim();
    if text.is_empty() || text.len() > 80 || text.ends_with('.') {
        return false;
    }
    let larger_than_body = height >= median_height * 1.2;
    let heading_shape = text
        .chars()
        .next()
        .is_some_and(|character| character.is_uppercase() || character.is_ascii_digit());
    larger_than_body && heading_shape
}

fn classify_table_candidates(nodes: &mut [SemanticNode]) {
    for node in nodes {
        if node.kind != SemanticNodeKind::Paragraph {
            continue;
        }
        if let Some(table) = &node.table {
            node.kind = SemanticNodeKind::TableCandidate;
            node.confidence = table.confidence;
        }
    }
}

fn table_cell_from_run(run: &TextRun) -> TableCell {
    TableCell {
        text: run.normalized_text.clone(),
        bbox: run.bbox,
        source: vec![run.source.clone()],
        row_span: 1,
        column_span: 1,
        is_placeholder: false,
    }
}

fn table_structure_from_cells(cells: &[TableCell]) -> Option<TableStructure> {
    if cells.len() < 3 {
        return None;
    }
    if cells.iter().any(|cell| !is_short_table_cell(cell)) {
        return None;
    }

    let typical_cell_height = median_cell_height(cells)?;
    let mut ordered_cells = cells.to_vec();
    ordered_cells.sort_by(|left, right| {
        table_row_anchor_y(right, typical_cell_height)
            .partial_cmp(&table_row_anchor_y(left, typical_cell_height))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                left.bbox
                    .x0
                    .partial_cmp(&right.bbox.x0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    let mut row_drafts: Vec<TableRowDraft> = Vec::new();
    for cell in ordered_cells {
        let anchor_y = table_row_anchor_y(&cell, typical_cell_height);
        if let Some(draft) = row_drafts
            .iter_mut()
            .find(|draft| (draft.anchor_y - anchor_y).abs() <= LINE_BASELINE_TOLERANCE)
        {
            draft.anchor_y = (draft.anchor_y + anchor_y) / 2.0;
            draft.row.bbox = union_rect(draft.row.bbox, cell.bbox);
            draft.row.cells.push(cell);
        } else {
            row_drafts.push(TableRowDraft {
                anchor_y,
                row: TableRow {
                    bbox: cell.bbox,
                    cells: vec![cell],
                },
            });
        }
    }

    if row_drafts.len() < 2 {
        return None;
    }

    for draft in &mut row_drafts {
        draft.row.cells.sort_by(|left, right| {
            left.bbox
                .x0
                .partial_cmp(&right.bbox.x0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    let row_anchors = row_drafts
        .iter()
        .map(|draft| draft.anchor_y)
        .collect::<Vec<_>>();
    let mut rows = row_drafts
        .into_iter()
        .map(|draft| draft.row)
        .collect::<Vec<_>>();

    let column_x_positions = infer_table_columns(&rows)?;
    if column_x_positions.len() < 2 {
        return None;
    }

    for (row_index, row) in rows.iter_mut().enumerate() {
        row.cells = align_row_cells(row, &column_x_positions, row_index, &row_anchors)?;
    }
    apply_span_coverage(&mut rows, &column_x_positions)?;

    for row in &rows {
        let sparse_gap_count = row
            .cells
            .iter()
            .filter(|cell| cell.is_placeholder && cell.row_span == 1 && cell.column_span == 1)
            .count();
        if sparse_gap_count > 1 {
            return None;
        }
    }

    Some(TableStructure {
        rows,
        column_x_positions,
        border_hints: Vec::new(),
        repeated_header_rows: Vec::new(),
        continuation_group: None,
        confidence: 0.65,
    })
}

fn classify_repeated_table_headers(nodes: &mut [SemanticNode]) {
    let candidates = repeated_table_header_candidates(nodes);
    let mut visited = vec![false; candidates.len()];
    for start_index in 0..candidates.len() {
        if visited[start_index] {
            continue;
        }
        let seed = &candidates[start_index];
        let mut group_indices = vec![start_index];
        for (candidate_index, candidate) in candidates.iter().enumerate().skip(start_index + 1) {
            if candidate.page_index != seed.page_index
                && candidate.header_signature == seed.header_signature
                && table_columns_compatible(&candidate.column_x_positions, &seed.column_x_positions)
            {
                group_indices.push(candidate_index);
            }
        }
        if group_indices.len() < 2 {
            continue;
        }
        let continuation_group =
            table_continuation_group_id(&seed.header_signature, &seed.column_x_positions);
        for group_index in group_indices {
            visited[group_index] = true;
            let node_index = candidates[group_index].node_index;
            let Some(node) = nodes.get_mut(node_index) else {
                continue;
            };
            let Some(table) = &mut node.table else {
                continue;
            };
            if !table.repeated_header_rows.contains(&0) {
                table.repeated_header_rows.push(0);
            }
            table.repeated_header_rows.sort_unstable();
            table.continuation_group = Some(continuation_group.clone());
            table.confidence = table.confidence.max(0.72);
            node.confidence = node.confidence.max(table.confidence);
        }
    }
}

fn repeated_table_header_candidates(nodes: &[SemanticNode]) -> Vec<RepeatedTableHeaderCandidate> {
    nodes
        .iter()
        .enumerate()
        .filter_map(|(node_index, node)| {
            if node.kind != SemanticNodeKind::TableCandidate {
                return None;
            }
            let table = node.table.as_ref()?;
            let header_signature = table_header_signature(table)?;
            Some(RepeatedTableHeaderCandidate {
                node_index,
                page_index: node.page_index,
                header_signature,
                column_x_positions: table.column_x_positions.clone(),
            })
        })
        .collect()
}

fn table_header_signature(table: &TableStructure) -> Option<String> {
    if table.rows.len() < 2 || table.column_x_positions.len() < 2 {
        return None;
    }
    let header = table.rows.first()?;
    let mut parts = Vec::new();
    for cell in &header.cells {
        if cell.is_placeholder {
            return None;
        }
        let normalized = normalize_anchor_text(&cell.text);
        if normalized.is_empty() {
            return None;
        }
        parts.push(normalized);
    }
    (parts.len() >= 2).then(|| parts.join("|"))
}

fn table_columns_compatible(left: &[f32], right: &[f32]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right.iter())
            .all(|(left, right)| (*left - *right).abs() <= TABLE_COLUMN_TOLERANCE)
}

fn table_continuation_group_id(header_signature: &str, column_x_positions: &[f32]) -> String {
    let column_count = column_x_positions.len();
    format!(
        "table-continuation:{:016x}",
        stable_hash(format!("{header_signature}|columns:{column_count}"))
    )
}

fn median_cell_height(cells: &[TableCell]) -> Option<f32> {
    let mut heights = cells
        .iter()
        .map(|cell| cell.bbox.height())
        .filter(|height| *height > 0.0)
        .collect::<Vec<_>>();
    if heights.is_empty() {
        return None;
    }
    heights.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    Some(heights[(heights.len() - 1) / 2].max(1.0))
}

fn table_row_anchor_y(cell: &TableCell, typical_cell_height: f32) -> f32 {
    if cell.bbox.height() >= typical_cell_height * 1.5 {
        cell.bbox.y1 - typical_cell_height
    } else {
        cell.bbox.y0
    }
}

fn infer_table_columns(rows: &[TableRow]) -> Option<Vec<f32>> {
    let widest_row_columns = rows.iter().map(|row| row.cells.len()).max()?;
    if widest_row_columns < 2 {
        return None;
    }

    let mut x_positions = rows
        .iter()
        .flat_map(|row| row.cells.iter().map(|cell| cell.bbox.x0))
        .collect::<Vec<_>>();
    x_positions.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));

    let mut columns: Vec<f32> = Vec::new();
    for x in x_positions {
        if let Some(existing) = columns
            .iter_mut()
            .find(|column| (**column - x).abs() <= TABLE_COLUMN_TOLERANCE)
        {
            *existing = (*existing + x) / 2.0;
        } else {
            columns.push(x);
        }
    }

    (columns.len() == widest_row_columns).then_some(columns)
}

fn align_row_cells(
    row: &TableRow,
    column_x_positions: &[f32],
    row_index: usize,
    row_anchors: &[f32],
) -> Option<Vec<TableCell>> {
    let mut aligned: Vec<Option<TableCell>> = vec![None; column_x_positions.len()];
    for cell in &row.cells {
        let column_index = nearest_column_index(cell.bbox.x0, column_x_positions)?;
        let column_span = infer_column_span(cell, column_index, column_x_positions);
        let end_column = column_index + column_span;
        if end_column > aligned.len()
            || aligned[column_index..end_column]
                .iter()
                .any(Option::is_some)
        {
            return None;
        }
        let mut spanned_cell = cell.clone();
        spanned_cell.row_span = infer_row_span(cell, row_index, row_anchors);
        spanned_cell.column_span = column_span;
        aligned[column_index] = Some(spanned_cell);
        for (covered_column, slot) in aligned
            .iter_mut()
            .enumerate()
            .take(end_column)
            .skip(column_index + 1)
        {
            *slot = Some(covered_table_cell(row, column_x_positions, covered_column));
        }
    }

    Some(
        aligned
            .into_iter()
            .enumerate()
            .map(|(column_index, cell)| {
                cell.unwrap_or_else(|| blank_table_cell(row, column_x_positions, column_index))
            })
            .collect(),
    )
}

fn apply_span_coverage(rows: &mut [TableRow], column_x_positions: &[f32]) -> Option<()> {
    for row_index in 0..rows.len() {
        for column_index in 0..column_x_positions.len() {
            let Some(cell) = rows
                .get(row_index)
                .and_then(|row| row.cells.get(column_index))
                .cloned()
            else {
                continue;
            };
            if cell.is_placeholder || cell.row_span <= 1 {
                continue;
            }
            let row_end = row_index + cell.row_span;
            let column_end = column_index + cell.column_span.max(1);
            if row_end > rows.len() || column_end > column_x_positions.len() {
                return None;
            }
            for covered_row in row_index..row_end {
                for covered_column in column_index..column_end {
                    if covered_row == row_index && covered_column == column_index {
                        continue;
                    }
                    let covered = rows
                        .get_mut(covered_row)
                        .and_then(|row| row.cells.get_mut(covered_column))?;
                    if !covered.is_placeholder {
                        return None;
                    }
                    *covered = spanned_placeholder_cell(covered);
                }
            }
        }
    }
    Some(())
}

fn infer_column_span(cell: &TableCell, column_index: usize, column_x_positions: &[f32]) -> usize {
    let mut column_span = 1;
    for next_column_x in column_x_positions.iter().skip(column_index + 1) {
        if cell.bbox.x1 >= *next_column_x - TABLE_COLUMN_TOLERANCE {
            column_span += 1;
        } else {
            break;
        }
    }
    column_span
}

fn infer_row_span(cell: &TableCell, row_index: usize, row_anchors: &[f32]) -> usize {
    let mut row_span = 1;
    for next_row_anchor in row_anchors.iter().skip(row_index + 1) {
        if cell.bbox.y0 <= *next_row_anchor + LINE_BASELINE_TOLERANCE {
            row_span += 1;
        } else {
            break;
        }
    }
    row_span
}

fn nearest_column_index(x: f32, column_x_positions: &[f32]) -> Option<usize> {
    column_x_positions
        .iter()
        .enumerate()
        .map(|(index, column_x)| (index, (x - column_x).abs()))
        .min_by(|left, right| {
            left.1
                .partial_cmp(&right.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .and_then(|(index, distance)| (distance <= TABLE_COLUMN_TOLERANCE).then_some(index))
}

fn blank_table_cell(row: &TableRow, column_x_positions: &[f32], column_index: usize) -> TableCell {
    let x0 = column_x_positions[column_index];
    let x1 = column_x_positions
        .get(column_index + 1)
        .copied()
        .unwrap_or(row.bbox.x1)
        .max(x0);
    TableCell {
        text: String::new(),
        bbox: Rect {
            x0,
            y0: row.bbox.y0,
            x1,
            y1: row.bbox.y1,
        },
        source: Vec::new(),
        row_span: 1,
        column_span: 1,
        is_placeholder: true,
    }
}

fn covered_table_cell(
    row: &TableRow,
    column_x_positions: &[f32],
    column_index: usize,
) -> TableCell {
    let mut cell = blank_table_cell(row, column_x_positions, column_index);
    cell.column_span = 0;
    cell.row_span = 0;
    cell
}

fn spanned_placeholder_cell(cell: &TableCell) -> TableCell {
    let mut cell = cell.clone();
    cell.row_span = 0;
    cell.column_span = 0;
    cell.is_placeholder = true;
    cell.source.clear();
    cell.text.clear();
    cell
}

fn attach_table_border_hints(nodes: &mut [SemanticNode], hints: &[TableBorderHint]) {
    if hints.is_empty() {
        return;
    }

    let mut ordered_hints = hints.to_vec();
    ordered_hints.sort_by(|left, right| {
        left.page_index
            .cmp(&right.page_index)
            .then_with(|| {
                left.bbox
                    .x0
                    .partial_cmp(&right.bbox.x0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                left.bbox
                    .y0
                    .partial_cmp(&right.bbox.y0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                left.bbox
                    .x1
                    .partial_cmp(&right.bbox.x1)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                left.bbox
                    .y1
                    .partial_cmp(&right.bbox.y1)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    for node in nodes {
        if node.kind != SemanticNodeKind::TableCandidate {
            continue;
        }
        let Some(node_bbox) = node.bbox else {
            continue;
        };
        let Some(table) = &mut node.table else {
            continue;
        };
        table.border_hints = ordered_hints
            .iter()
            .filter(|hint| {
                hint.page_index == node.page_index
                    && rects_overlap(expand_rect(node_bbox, 4.0), hint.bbox)
            })
            .cloned()
            .collect();
        if !table.border_hints.is_empty() {
            table.confidence = table.confidence.max(0.75);
            node.confidence = table.confidence;
        }
    }
}

fn classify_list_candidates(nodes: &mut [SemanticNode]) {
    for node in nodes {
        if node.kind != SemanticNodeKind::Paragraph {
            continue;
        }
        let Some(text) = node.normalized_text.as_deref() else {
            continue;
        };
        if is_list_candidate(text) {
            node.kind = SemanticNodeKind::ListCandidate;
            node.confidence = 0.6;
        }
    }
}

fn is_list_candidate(text: &str) -> bool {
    let text = text.trim_start();
    is_bullet_list_marker(text) || is_numbered_list_marker(text)
}

fn is_bullet_list_marker(text: &str) -> bool {
    text.strip_prefix('-')
        .or_else(|| text.strip_prefix('*'))
        .or_else(|| text.strip_prefix('+'))
        .is_some_and(|remaining| remaining.starts_with(char::is_whitespace))
}

fn is_numbered_list_marker(text: &str) -> bool {
    let mut chars = text.char_indices();
    let mut digit_end = None;
    for (index, character) in &mut chars {
        if character.is_ascii_digit() {
            digit_end = Some(index + character.len_utf8());
        } else {
            break;
        }
    }
    let Some(digit_end) = digit_end else {
        return false;
    };
    if digit_end > 3 {
        return false;
    }
    let remaining = &text[digit_end..];
    let Some(after_marker) = remaining
        .strip_prefix('.')
        .or_else(|| remaining.strip_prefix(')'))
    else {
        return false;
    };
    after_marker.starts_with(char::is_whitespace)
}

fn assign_semantic_anchors(nodes: &mut [SemanticNode]) {
    let mut current_heading_context = None;
    for node in nodes {
        node.anchor = build_anchor(node, current_heading_context.as_deref());
        if node.kind == SemanticNodeKind::HeadingCandidate {
            current_heading_context = Some(node.anchor.strong_text_hash.clone());
        }
    }
}

fn build_anchor(node: &SemanticNode, heading_context: Option<&str>) -> SemanticAnchor {
    let text = node.normalized_text.as_deref().unwrap_or_default();
    SemanticAnchor {
        strong_text_hash: format!("text:{:016x}", stable_hash(normalize_anchor_text(text))),
        weak_text_signature: format!("weak:{:016x}", stable_hash(weak_signature_text(text))),
        geometry_bucket: geometry_bucket(node.page_index, node.bbox),
        heading_context: heading_context.map(ToOwned::to_owned),
    }
}

fn normalize_anchor_text(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn repeated_region_signature(text: &str) -> String {
    let mut in_digit_run = false;
    let mut signature = String::new();
    for character in normalize_anchor_text(text).chars() {
        if character.is_ascii_digit() {
            if !in_digit_run {
                signature.push('#');
                in_digit_run = true;
            }
        } else {
            in_digit_run = false;
            signature.push(character);
        }
    }
    signature
}

fn weak_signature_text(text: &str) -> String {
    let tokens = normalize_anchor_text(text)
        .split_whitespace()
        .filter(|token| token.chars().any(char::is_alphabetic))
        .take(8)
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        normalize_anchor_text(text)
    } else {
        tokens.join(" ")
    }
}

fn geometry_bucket(page_index: usize, bbox: Option<Rect>) -> String {
    let Some(bbox) = bbox else {
        return format!("page-{page_index}:x-unknown:y-unknown");
    };
    format!(
        "page-{page_index}:x-{:04}:y-{:04}",
        bucket_coordinate(bbox.x0),
        bucket_coordinate(bbox.y0)
    )
}

fn bucket_coordinate(value: f32) -> i32 {
    (value / 50.0).floor() as i32
}

fn stable_hash(text: String) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x00000100000001b3;
    let mut hash = FNV_OFFSET;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

fn append_text(target: &mut String, next: &str) {
    if target.is_empty()
        || target.ends_with(char::is_whitespace)
        || next.starts_with(char::is_whitespace)
    {
        target.push_str(next);
    } else {
        target.push(' ');
        target.push_str(next);
    }
}

fn union_rect(left: Rect, right: Rect) -> Rect {
    Rect {
        x0: left.x0.min(right.x0),
        y0: left.y0.min(right.y0),
        x1: left.x1.max(right.x1),
        y1: left.y1.max(right.y1),
    }
}

fn expand_rect(rect: Rect, amount: f32) -> Rect {
    Rect {
        x0: rect.x0 - amount,
        y0: rect.y0 - amount,
        x1: rect.x1 + amount,
        y1: rect.y1 + amount,
    }
}

fn rects_overlap(left: Rect, right: Rect) -> bool {
    left.x0 <= right.x1 && left.x1 >= right.x0 && left.y0 <= right.y1 && left.y1 >= right.y0
}

#[cfg(test)]
mod tests {
    use super::*;
    use spdfdiff_types::{LineSegment, Point};

    #[test]
    fn turns_text_runs_into_paragraph_nodes() {
        let run = text_run("run1", "Hello", 0, rect(10.0, 20.0, 40.0, 32.0));
        let document = build_semantic_document("fixture", &[run], Vec::new());

        assert_eq!(document.nodes.len(), 1);
        assert_eq!(document.nodes[0].kind, SemanticNodeKind::Paragraph);
        assert_eq!(document.nodes[0].normalized_text.as_deref(), Some("Hello"));
        let _ = LineSegment {
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 1.0, y: 1.0 },
        };
    }

    #[test]
    fn groups_same_line_runs_left_to_right() {
        let runs = vec![
            text_run("run2", "world", 0, rect(50.0, 20.0, 80.0, 32.0)),
            text_run("run1", "Hello", 0, rect(10.0, 20.0, 40.0, 32.0)),
        ];
        let document = build_semantic_document("fixture", &runs, Vec::new());

        assert_eq!(document.nodes.len(), 1);
        assert_eq!(
            document.nodes[0].normalized_text.as_deref(),
            Some("Hello world")
        );
        assert_eq!(document.nodes[0].source.len(), 2);
    }

    #[test]
    fn groups_multiline_paragraph_by_vertical_gap() {
        let runs = vec![
            text_run("run1", "First line", 0, rect(10.0, 100.0, 80.0, 112.0)),
            text_run("run2", "second line", 0, rect(10.0, 84.0, 90.0, 96.0)),
        ];
        let document = build_semantic_document("fixture", &runs, Vec::new());

        assert_eq!(document.nodes.len(), 1);
        assert_eq!(
            document.nodes[0].normalized_text.as_deref(),
            Some("First line second line")
        );
        assert_eq!(
            document.nodes[0].bbox.unwrap(),
            rect(10.0, 84.0, 90.0, 112.0)
        );
    }

    #[test]
    fn orders_multicolumn_flow_by_column_before_vertical_position() {
        let runs = vec![
            text_run(
                "right-top",
                "Right column top",
                0,
                rect(500.0, 120.0, 590.0, 132.0),
            ),
            text_run(
                "left-bottom",
                "left continues",
                0,
                rect(10.0, 100.0, 110.0, 112.0),
            ),
            text_run(
                "right-bottom",
                "right continues",
                0,
                rect(500.0, 100.0, 600.0, 112.0),
            ),
            text_run(
                "left-top",
                "Left column top",
                0,
                rect(10.0, 120.0, 100.0, 132.0),
            ),
        ];
        let document = build_semantic_document("fixture", &runs, Vec::new());

        assert_eq!(document.nodes.len(), 2);
        assert_eq!(
            document.nodes[0].normalized_text.as_deref(),
            Some("Left column top left continues")
        );
        assert_eq!(
            document.nodes[1].normalized_text.as_deref(),
            Some("Right column top right continues")
        );
    }

    #[test]
    fn classifies_repeated_headers_and_footers() {
        let runs = vec![
            text_run(
                "h1",
                "Confidential Report",
                0,
                rect(10.0, 760.0, 160.0, 772.0),
            ),
            text_run("b1", "First page body", 0, rect(10.0, 600.0, 140.0, 612.0)),
            text_run("f1", "Company Footer", 0, rect(10.0, 20.0, 130.0, 32.0)),
            text_run(
                "h2",
                "Confidential Report",
                1,
                rect(10.0, 760.0, 160.0, 772.0),
            ),
            text_run("b2", "Second page body", 1, rect(10.0, 600.0, 150.0, 612.0)),
            text_run("f2", "Company Footer", 1, rect(10.0, 20.0, 130.0, 32.0)),
        ];
        let document = build_semantic_document("fixture", &runs, Vec::new());

        assert!(
            document
                .nodes
                .iter()
                .filter(|node| node.kind == SemanticNodeKind::HeaderCandidate)
                .count()
                >= 2
        );
        assert!(
            document
                .nodes
                .iter()
                .filter(|node| node.kind == SemanticNodeKind::FooterCandidate)
                .count()
                >= 2
        );
    }

    #[test]
    fn classifies_repeated_page_regions_with_variable_page_numbers() {
        let runs = vec![
            text_run(
                "f1",
                "Confidential - 2025 | Page 1",
                0,
                rect(10.0, 20.0, 180.0, 32.0),
            ),
            text_run("b1", "First page body", 0, rect(10.0, 600.0, 140.0, 612.0)),
            text_run(
                "f2",
                "Confidential - 2025 | Page 2",
                1,
                rect(10.0, 20.0, 180.0, 32.0),
            ),
            text_run("b2", "Second page body", 1, rect(10.0, 600.0, 150.0, 612.0)),
        ];
        let document = build_semantic_document("fixture", &runs, Vec::new());

        assert_eq!(
            document
                .nodes
                .iter()
                .filter(|node| node.kind == SemanticNodeKind::FooterCandidate)
                .count(),
            2
        );
    }

    #[test]
    fn classifies_repeated_page_template_content_away_from_edges() {
        let runs = vec![
            text_run("w1", "DRAFT", 0, rect(240.0, 360.0, 300.0, 372.0)),
            text_run("b1", "First page body", 0, rect(10.0, 620.0, 140.0, 632.0)),
            text_run("w2", "DRAFT", 1, rect(240.0, 360.0, 300.0, 372.0)),
            text_run("b2", "Second page body", 1, rect(10.0, 620.0, 150.0, 632.0)),
            text_run("w3", "DRAFT", 2, rect(240.0, 360.0, 300.0, 372.0)),
            text_run("b3", "Third page body", 2, rect(10.0, 620.0, 140.0, 632.0)),
        ];
        let document = build_semantic_document("fixture", &runs, Vec::new());

        assert_eq!(
            document
                .nodes
                .iter()
                .filter(|node| node.kind == SemanticNodeKind::PageTemplateCandidate)
                .count(),
            3
        );
    }

    #[test]
    fn keeps_separate_paragraphs_when_gap_is_large() {
        let runs = vec![
            text_run(
                "run1",
                "First paragraph",
                0,
                rect(10.0, 100.0, 100.0, 112.0),
            ),
            text_run("run2", "Second paragraph", 0, rect(10.0, 40.0, 110.0, 52.0)),
        ];
        let document = build_semantic_document("fixture", &runs, Vec::new());

        assert_eq!(document.nodes.len(), 2);
        assert_eq!(
            document.nodes[0].normalized_text.as_deref(),
            Some("First paragraph")
        );
        assert_eq!(
            document.nodes[1].normalized_text.as_deref(),
            Some("Second paragraph")
        );
    }

    #[test]
    fn detects_controlled_heading_candidate() {
        let runs = vec![
            text_run("heading", "1. Scope", 0, rect(10.0, 120.0, 80.0, 140.0)),
            text_run(
                "body",
                "This paragraph explains the scope.",
                0,
                rect(10.0, 40.0, 180.0, 52.0),
            ),
        ];
        let document = build_semantic_document("fixture", &runs, Vec::new());

        assert_eq!(document.nodes.len(), 2);
        assert_eq!(document.nodes[0].kind, SemanticNodeKind::HeadingCandidate);
        assert_eq!(document.nodes[0].confidence, 0.65);
        assert_eq!(document.nodes[1].kind, SemanticNodeKind::Paragraph);
    }

    #[test]
    fn assigns_stable_semantic_anchors() {
        let runs = vec![text_run(
            "run1",
            "Payment is due in 30 days",
            0,
            rect(72.0, 120.0, 180.0, 132.0),
        )];
        let first = build_semantic_document("first", &runs, Vec::new());
        let second = build_semantic_document("second", &runs, Vec::new());

        assert_eq!(first.nodes[0].anchor, second.nodes[0].anchor);
        assert!(first.nodes[0].anchor.strong_text_hash.starts_with("text:"));
        assert!(
            first.nodes[0]
                .anchor
                .weak_text_signature
                .starts_with("weak:")
        );
        assert_eq!(
            first.nodes[0].anchor.geometry_bucket,
            "page-0:x-0001:y-0002"
        );
    }

    #[test]
    fn text_edit_changes_strong_hash_but_keeps_weak_signature() {
        let old = build_semantic_document(
            "old",
            &[text_run(
                "old",
                "Payment is due in 30 days",
                0,
                rect(72.0, 120.0, 180.0, 132.0),
            )],
            Vec::new(),
        );
        let new = build_semantic_document(
            "new",
            &[text_run(
                "new",
                "Payment is due in 15 days",
                0,
                rect(72.0, 120.0, 180.0, 132.0),
            )],
            Vec::new(),
        );

        assert_ne!(
            old.nodes[0].anchor.strong_text_hash,
            new.nodes[0].anchor.strong_text_hash
        );
        assert_eq!(
            old.nodes[0].anchor.weak_text_signature,
            new.nodes[0].anchor.weak_text_signature
        );
    }

    #[test]
    fn paragraph_anchor_keeps_heading_context() {
        let document = build_semantic_document(
            "fixture",
            &[
                text_run("heading", "1. Scope", 0, rect(10.0, 120.0, 80.0, 140.0)),
                text_run("body", "Body text", 0, rect(10.0, 40.0, 80.0, 52.0)),
            ],
            Vec::new(),
        );

        assert_eq!(document.nodes[0].anchor.heading_context, None);
        assert_eq!(
            document.nodes[1].anchor.heading_context,
            Some(document.nodes[0].anchor.strong_text_hash.clone())
        );
    }

    #[test]
    fn detects_basic_numbered_list_candidate() {
        let document = build_semantic_document(
            "fixture",
            &[
                text_run("item1", "1. First item", 0, rect(10.0, 120.0, 90.0, 132.0)),
                text_run("body", "Body paragraph.", 0, rect(10.0, 40.0, 100.0, 52.0)),
            ],
            Vec::new(),
        );

        assert_eq!(document.nodes[0].kind, SemanticNodeKind::ListCandidate);
        assert_eq!(document.nodes[0].confidence, 0.6);
        assert_eq!(document.nodes[1].kind, SemanticNodeKind::Paragraph);
    }

    #[test]
    fn detects_basic_bullet_list_candidate() {
        let document = build_semantic_document(
            "fixture",
            &[text_run(
                "item",
                "- Bullet item",
                0,
                rect(10.0, 120.0, 90.0, 132.0),
            )],
            Vec::new(),
        );

        assert_eq!(document.nodes[0].kind, SemanticNodeKind::ListCandidate);
    }

    #[test]
    fn detects_simple_text_table_candidate() {
        let runs = vec![
            text_run("a1", "A1", 0, rect(10.0, 100.0, 20.0, 112.0)),
            text_run("a2", "A2", 0, rect(70.0, 100.0, 80.0, 112.0)),
            text_run("b1", "B1", 0, rect(10.0, 84.0, 20.0, 96.0)),
            text_run("b2", "B2", 0, rect(70.0, 84.0, 80.0, 96.0)),
        ];
        let document = build_semantic_document("fixture", &runs, Vec::new());

        assert_eq!(document.nodes.len(), 1);
        assert_eq!(document.nodes[0].kind, SemanticNodeKind::TableCandidate);
        assert_eq!(document.nodes[0].confidence, 0.65);
        assert_eq!(
            document.nodes[0].normalized_text.as_deref(),
            Some("A1 A2 B1 B2")
        );
        let table = document.nodes[0]
            .table
            .as_ref()
            .expect("aligned runs should preserve table evidence");
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.column_x_positions, vec![10.0, 70.0]);
        assert_eq!(table.rows[0].cells[0].text, "A1");
        assert_eq!(table.rows[0].cells[1].text, "A2");
        assert_eq!(table.rows[1].cells[0].text, "B1");
        assert_eq!(table.rows[1].cells[1].text, "B2");
    }

    #[test]
    fn reconstructs_sparse_text_table_with_blank_cell() {
        let runs = vec![
            text_run("a1", "A1", 0, rect(10.0, 100.0, 20.0, 112.0)),
            text_run("a2", "A2", 0, rect(70.0, 100.0, 80.0, 112.0)),
            text_run("b2", "B2", 0, rect(70.0, 84.0, 80.0, 96.0)),
        ];
        let document = build_semantic_document("fixture", &runs, Vec::new());

        assert_eq!(document.nodes.len(), 1);
        assert_eq!(document.nodes[0].kind, SemanticNodeKind::TableCandidate);
        let table = document.nodes[0]
            .table
            .as_ref()
            .expect("sparse aligned runs should preserve table evidence");
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.column_x_positions, vec![10.0, 70.0]);
        assert_eq!(table.rows[0].cells[0].text, "A1");
        assert_eq!(table.rows[0].cells[1].text, "A2");
        assert_eq!(table.rows[1].cells[0].text, "");
        assert_eq!(table.rows[1].cells[1].text, "B2");
        assert!(table.rows[1].cells[0].source.is_empty());
    }

    #[test]
    fn reconstructs_column_spanning_text_table_cell() {
        let runs = vec![
            text_run("header", "Total", 0, rect(10.0, 116.0, 82.0, 128.0)),
            text_run("a1", "A1", 0, rect(10.0, 100.0, 20.0, 112.0)),
            text_run("a2", "A2", 0, rect(70.0, 100.0, 80.0, 112.0)),
        ];
        let document = build_semantic_document("fixture", &runs, Vec::new());

        assert_eq!(document.nodes.len(), 1);
        assert_eq!(document.nodes[0].kind, SemanticNodeKind::TableCandidate);
        let table = document.nodes[0]
            .table
            .as_ref()
            .expect("spanning aligned runs should preserve table evidence");
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.column_x_positions, vec![10.0, 70.0]);
        assert_eq!(table.rows[0].cells[0].text, "Total");
        assert_eq!(table.rows[0].cells[0].column_span, 2);
        assert_eq!(table.rows[0].cells[1].text, "");
        assert!(table.rows[0].cells[1].is_placeholder);
        assert_eq!(table.rows[0].cells[1].column_span, 0);
        assert_eq!(table.rows[1].cells[0].column_span, 1);
        assert_eq!(table.rows[1].cells[1].column_span, 1);
    }

    #[test]
    fn reconstructs_row_spanning_text_table_cell() {
        let runs = vec![
            text_run("group", "Group", 0, rect(10.0, 84.0, 20.0, 128.0)),
            text_run("a2", "A2", 0, rect(70.0, 116.0, 80.0, 128.0)),
            text_run("b2", "B2", 0, rect(70.0, 84.0, 80.0, 96.0)),
        ];
        let document = build_semantic_document("fixture", &runs, Vec::new());

        assert_eq!(document.nodes.len(), 1);
        assert_eq!(document.nodes[0].kind, SemanticNodeKind::TableCandidate);
        let table = document.nodes[0]
            .table
            .as_ref()
            .expect("row-spanning aligned runs should preserve table evidence");
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.column_x_positions, vec![10.0, 70.0]);
        assert_eq!(table.rows[0].cells[0].text, "Group");
        assert_eq!(table.rows[0].cells[0].row_span, 2);
        assert_eq!(table.rows[0].cells[0].column_span, 1);
        assert_eq!(table.rows[1].cells[0].text, "");
        assert!(table.rows[1].cells[0].is_placeholder);
        assert_eq!(table.rows[1].cells[0].row_span, 0);
        assert_eq!(table.rows[1].cells[0].column_span, 0);
        assert_eq!(table.rows[1].cells[1].text, "B2");
    }

    #[test]
    fn reconstructs_complex_merged_text_table_cell() {
        let runs = vec![
            text_run("merged", "Total", 0, rect(10.0, 84.0, 82.0, 128.0)),
            text_run("a3", "A3", 0, rect(130.0, 116.0, 140.0, 128.0)),
            text_run("b3", "B3", 0, rect(130.0, 84.0, 140.0, 96.0)),
            text_run("c1", "C1", 0, rect(10.0, 68.0, 20.0, 80.0)),
            text_run("c2", "C2", 0, rect(70.0, 68.0, 80.0, 80.0)),
            text_run("c3", "C3", 0, rect(130.0, 68.0, 140.0, 80.0)),
        ];
        let document = build_semantic_document("fixture", &runs, Vec::new());

        assert_eq!(document.nodes.len(), 1);
        assert_eq!(document.nodes[0].kind, SemanticNodeKind::TableCandidate);
        let table = document.nodes[0]
            .table
            .as_ref()
            .expect("merged aligned runs should preserve table evidence");
        assert_eq!(table.rows.len(), 3);
        assert_eq!(table.column_x_positions, vec![10.0, 70.0, 130.0]);
        assert_eq!(table.rows[0].cells[0].text, "Total");
        assert_eq!(table.rows[0].cells[0].row_span, 2);
        assert_eq!(table.rows[0].cells[0].column_span, 2);
        assert_eq!(table.rows[0].cells[2].text, "A3");
        assert_eq!(table.rows[1].cells[0].text, "");
        assert_eq!(table.rows[1].cells[1].text, "");
        assert_eq!(table.rows[1].cells[2].text, "B3");
        for (row_index, column_index) in [(0, 1), (1, 0), (1, 1)] {
            assert!(table.rows[row_index].cells[column_index].is_placeholder);
            assert_eq!(table.rows[row_index].cells[column_index].row_span, 0);
            assert_eq!(table.rows[row_index].cells[column_index].column_span, 0);
        }
        assert_eq!(table.rows[2].cells[0].text, "C1");
        assert_eq!(table.rows[2].cells[1].text, "C2");
        assert_eq!(table.rows[2].cells[2].text, "C3");
    }

    #[test]
    fn marks_repeated_header_rows_across_page_split_tables() {
        let runs = vec![
            text_run("p1-h1", "Item", 0, rect(10.0, 116.0, 30.0, 128.0)),
            text_run("p1-h2", "Qty", 0, rect(70.0, 116.0, 90.0, 128.0)),
            text_run("p1-a1", "Paper", 0, rect(10.0, 100.0, 36.0, 112.0)),
            text_run("p1-a2", "4", 0, rect(70.0, 100.0, 76.0, 112.0)),
            text_run("p2-h1", "Item", 1, rect(12.0, 116.0, 32.0, 128.0)),
            text_run("p2-h2", "Qty", 1, rect(72.0, 116.0, 92.0, 128.0)),
            text_run("p2-a1", "Ink", 1, rect(12.0, 100.0, 28.0, 112.0)),
            text_run("p2-a2", "2", 1, rect(72.0, 100.0, 78.0, 112.0)),
        ];
        let document = build_semantic_document("fixture", &runs, Vec::new());

        assert_eq!(document.nodes.len(), 2);
        let first_table = document.nodes[0]
            .table
            .as_ref()
            .expect("first page should preserve table evidence");
        let second_table = document.nodes[1]
            .table
            .as_ref()
            .expect("second page should preserve table evidence");
        assert_eq!(document.nodes[0].kind, SemanticNodeKind::TableCandidate);
        assert_eq!(document.nodes[1].kind, SemanticNodeKind::TableCandidate);
        assert_eq!(first_table.repeated_header_rows, vec![0]);
        assert_eq!(second_table.repeated_header_rows, vec![0]);
        assert_eq!(
            first_table.continuation_group,
            second_table.continuation_group
        );
        assert!(first_table.continuation_group.is_some());
        assert!(first_table.confidence >= 0.72);
        assert!(second_table.confidence >= 0.72);
    }

    #[test]
    fn keeps_too_sparse_text_grid_as_paragraph_not_table() {
        let runs = vec![
            text_run("a1", "A1", 0, rect(10.0, 116.0, 20.0, 128.0)),
            text_run("a2", "A2", 0, rect(70.0, 116.0, 80.0, 128.0)),
            text_run("a3", "A3", 0, rect(130.0, 116.0, 140.0, 128.0)),
            text_run("b1", "B1", 0, rect(10.0, 100.0, 20.0, 112.0)),
        ];
        let document = build_semantic_document("fixture", &runs, Vec::new());

        assert_eq!(document.nodes[0].kind, SemanticNodeKind::Paragraph);
        assert!(document.nodes[0].table.is_none());
    }

    #[test]
    fn attaches_border_hints_to_matching_table_candidate() {
        let runs = vec![
            text_run("a1", "A1", 0, rect(10.0, 100.0, 20.0, 112.0)),
            text_run("a2", "A2", 0, rect(70.0, 100.0, 80.0, 112.0)),
            text_run("b1", "B1", 0, rect(10.0, 84.0, 20.0, 96.0)),
            text_run("b2", "B2", 0, rect(70.0, 84.0, 80.0, 96.0)),
        ];
        let document = build_semantic_document_with_table_hints(
            "fixture",
            &runs,
            Vec::new(),
            vec![TableBorderHint {
                page_index: 0,
                bbox: rect(8.0, 82.0, 82.0, 114.0),
                source: vec![Provenance {
                    page_index: Some(0),
                    content_op_index: Some(4),
                    ..Provenance::unknown()
                }],
            }],
        );

        let table = document.nodes[0]
            .table
            .as_ref()
            .expect("aligned runs should preserve table evidence");
        assert_eq!(document.nodes[0].kind, SemanticNodeKind::TableCandidate);
        assert_eq!(document.nodes[0].confidence, 0.75);
        assert_eq!(table.border_hints.len(), 1);
        assert_eq!(table.border_hints[0].bbox, rect(8.0, 82.0, 82.0, 114.0));
    }

    #[test]
    fn keeps_single_run_short_text_as_paragraph_not_table() {
        let document = build_semantic_document(
            "fixture",
            &[text_run(
                "run",
                "A1 A2 B1 B2",
                0,
                rect(10.0, 100.0, 120.0, 112.0),
            )],
            Vec::new(),
        );

        assert_eq!(document.nodes[0].kind, SemanticNodeKind::Paragraph);
    }

    #[test]
    fn keeps_misaligned_short_runs_as_paragraph_not_table() {
        let runs = vec![
            text_run("a1", "A1", 0, rect(10.0, 100.0, 20.0, 112.0)),
            text_run("a2", "A2", 0, rect(70.0, 100.0, 80.0, 112.0)),
            text_run("b1", "B1", 0, rect(10.0, 84.0, 20.0, 96.0)),
            text_run("b2", "B2", 0, rect(95.0, 84.0, 105.0, 96.0)),
        ];
        let document = build_semantic_document("fixture", &runs, Vec::new());

        assert_eq!(document.nodes[0].kind, SemanticNodeKind::Paragraph);
        assert!(document.nodes[0].table.is_none());
    }

    #[test]
    fn prefers_tagged_structure_nodes_when_mcids_map_to_text_runs() {
        let runs = vec![
            tagged_text_run(
                "body",
                "Body first",
                0,
                rect(10.0, 40.0, 80.0, 52.0),
                "P",
                1,
            ),
            tagged_text_run(
                "heading",
                "Tagged Heading",
                0,
                rect(10.0, 120.0, 120.0, 132.0),
                "H1",
                0,
            ),
        ];
        let tagged_structure = TaggedStructureSummary {
            root_object_id: None,
            role_map: Vec::new(),
            element_count: 2,
            mcid_count: 2,
            parent_tree_entries: 1,
            structure_types: vec!["H1".to_owned(), "P".to_owned()],
            elements: vec![
                TaggedStructureElementSummary {
                    structure_type: "H1".to_owned(),
                    mapped_structure_type: None,
                    mcids: vec![0],
                    children: Vec::new(),
                },
                TaggedStructureElementSummary {
                    structure_type: "P".to_owned(),
                    mapped_structure_type: None,
                    mcids: vec![1],
                    children: Vec::new(),
                },
            ],
            confidence: 0.9,
        };
        let document = build_semantic_document_with_tagged_structure(
            "fixture",
            &runs,
            Vec::new(),
            Some(tagged_structure),
        );

        assert_eq!(document.nodes.len(), 2);
        assert_eq!(document.nodes[0].kind, SemanticNodeKind::HeadingCandidate);
        assert_eq!(
            document.nodes[0].normalized_text.as_deref(),
            Some("Tagged Heading")
        );
        assert_eq!(document.nodes[0].confidence, 0.9);
        assert_eq!(document.nodes[1].kind, SemanticNodeKind::Paragraph);
        assert_eq!(
            document.nodes[1].normalized_text.as_deref(),
            Some("Body first")
        );
    }

    #[test]
    fn uses_mapped_tagged_role_for_semantic_node_kind() {
        let runs = vec![tagged_text_run(
            "heading",
            "Mapped Heading",
            0,
            rect(10.0, 120.0, 120.0, 132.0),
            "ChapterTitle",
            0,
        )];
        let tagged_structure = TaggedStructureSummary {
            root_object_id: None,
            role_map: vec![TaggedRoleMapSummary {
                source: "ChapterTitle".to_owned(),
                target: "H1".to_owned(),
            }],
            element_count: 1,
            mcid_count: 1,
            parent_tree_entries: 1,
            structure_types: vec!["ChapterTitle".to_owned(), "H1".to_owned()],
            elements: vec![TaggedStructureElementSummary {
                structure_type: "ChapterTitle".to_owned(),
                mapped_structure_type: Some("H1".to_owned()),
                mcids: vec![0],
                children: Vec::new(),
            }],
            confidence: 0.9,
        };
        let document = build_semantic_document_with_tagged_structure(
            "fixture",
            &runs,
            Vec::new(),
            Some(tagged_structure),
        );

        assert_eq!(document.nodes.len(), 1);
        assert_eq!(document.nodes[0].kind, SemanticNodeKind::HeadingCandidate);
        assert_eq!(
            document.nodes[0].normalized_text.as_deref(),
            Some("Mapped Heading")
        );
    }

    fn text_run(id: &str, text: &str, page_index: usize, bbox: Rect) -> TextRun {
        TextRun {
            id: id.into(),
            text: text.into(),
            normalized_text: text.into(),
            glyphs: Vec::new(),
            bbox,
            source: Provenance {
                page_index: Some(page_index),
                ..Provenance::unknown()
            },
            marked_content: None,
        }
    }

    fn tagged_text_run(
        id: &str,
        text: &str,
        page_index: usize,
        bbox: Rect,
        tag: &str,
        mcid: usize,
    ) -> TextRun {
        TextRun {
            marked_content: Some(pdf_text::MarkedContentRef {
                tag: tag.to_owned(),
                mcid: Some(mcid),
            }),
            ..text_run(id, text, page_index, bbox)
        }
    }

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect { x0, y0, x1, y1 }
    }
}
