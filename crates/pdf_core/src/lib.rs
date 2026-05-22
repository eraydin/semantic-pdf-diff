use flate2::read::ZlibDecoder;
use spdfdiff_types::{ByteRange, Diagnostic, ObjectId, ParseConfig, PdfDiffError, ResourceLimits};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfVersion {
    pub major: u8,
    pub minor: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PdfDocument {
    pub version: PdfVersion,
    pub objects: Vec<PdfObject>,
    pub pages: Vec<PdfPage>,
    pub diagnostics: Vec<Diagnostic>,
    pub incremental_update: Option<IncrementalUpdateInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncrementalUpdateInfo {
    pub revision_count: usize,
    pub selected_startxref_offset: Option<usize>,
    pub prior_startxref_offsets: Vec<usize>,
    pub trailer_prev_offsets: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfObject {
    pub id: ObjectId,
    pub body: String,
    pub stream: Option<PdfStream>,
    pub byte_range: ByteRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfStream {
    pub bytes: Vec<u8>,
    pub raw_bytes: Vec<u8>,
    pub byte_range: ByteRange,
    pub declared_length: Option<usize>,
    pub filters: Vec<String>,
    pub decode_params: Vec<Option<String>>,
    pub decoded: bool,
}

impl PdfStream {
    fn with_metadata(
        mut self,
        declared_length: Option<usize>,
        filters: Vec<String>,
        decode_params: Vec<Option<String>>,
    ) -> Self {
        self.declared_length = declared_length;
        self.filters = filters;
        self.decode_params = decode_params;
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PdfPage {
    pub page_index: usize,
    pub object_id: ObjectId,
    pub content_object_id: ObjectId,
    pub content_object_ids: Vec<ObjectId>,
    pub media_box: Option<spdfdiff_types::Rect>,
    pub crop_box: Option<spdfdiff_types::Rect>,
    pub rotation: i32,
    pub resources_object_id: Option<ObjectId>,
    pub has_resources: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaggedStructure {
    pub root_object_id: Option<ObjectId>,
    pub role_map: Vec<TaggedRoleMapEntry>,
    pub roots: Vec<TaggedStructureElement>,
    pub parent_tree: Vec<TaggedParentTreeEntry>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaggedRoleMapEntry {
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaggedStructureElement {
    pub object_id: Option<ObjectId>,
    pub structure_type: String,
    pub mapped_structure_type: Option<String>,
    pub mcids: Vec<usize>,
    pub children: Vec<TaggedStructureElement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaggedParentTreeEntry {
    pub struct_parent: usize,
    pub element_object_ids: Vec<ObjectId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedIndirectObject {
    pub id: ObjectId,
    pub value: PdfPrimitive,
    pub stream: Option<PdfStream>,
    pub byte_range: ByteRange,
    pub value_byte_range: ByteRange,
    pub embedded_source: Option<EmbeddedObjectSource>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddedObjectSource {
    pub object_stream_id: ObjectId,
    pub object_index: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct XrefTable {
    pub start_offset: usize,
    pub entries: Vec<XrefEntry>,
    pub trailer: PdfPrimitive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XrefEntry {
    pub object_id: ObjectId,
    pub byte_offset: usize,
    pub in_use: bool,
    pub kind: XrefEntryKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XrefEntryKind {
    Free,
    InUse,
    Compressed {
        object_stream: ObjectId,
        object_index: usize,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectStore {
    pub xref: XrefTable,
    pub objects: Vec<ParsedIndirectObject>,
}

impl ObjectStore {
    #[must_use]
    pub fn get(&self, id: ObjectId) -> Option<&ParsedIndirectObject> {
        self.objects.iter().find(|object| object.id == id)
    }

    pub fn resolve_reference_chain(
        &self,
        id: ObjectId,
        limits: ResourceLimits,
    ) -> Result<&ParsedIndirectObject, PdfDiffError> {
        let mut current = id;
        let mut seen = Vec::new();
        for depth in 0..=limits.max_indirect_depth {
            if seen.contains(&current) {
                return Err(resource_limit_error(
                    "RESOURCE_LIMIT_REFERENCE_CYCLE",
                    format!("reference cycle detected at object {}", current.number),
                ));
            }
            seen.push(current);
            let object = self.get(current).ok_or_else(|| {
                PdfDiffError::InvalidInput(format!("object {} was not found", current.number))
            })?;
            let PdfPrimitive::Reference(next) = object.value else {
                return Ok(object);
            };
            if depth == limits.max_indirect_depth {
                return Err(resource_limit_error(
                    "RESOURCE_LIMIT_REFERENCE_DEPTH",
                    format!(
                        "reference depth exceeds limit of {}",
                        limits.max_indirect_depth
                    ),
                ));
            }
            current = next;
        }
        Err(resource_limit_error(
            "RESOURCE_LIMIT_REFERENCE_DEPTH",
            format!(
                "reference depth exceeds limit of {}",
                limits.max_indirect_depth
            ),
        ))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedPrimitive {
    pub value: PdfPrimitive,
    pub byte_range: ByteRange,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PdfPrimitive {
    Null,
    Boolean(bool),
    Integer(i64),
    Real(f64),
    Name(String),
    LiteralString(Vec<u8>),
    HexString(Vec<u8>),
    Array(Vec<PdfPrimitive>),
    Dictionary(Vec<(String, PdfPrimitive)>),
    Reference(ObjectId),
}

impl PdfDocument {
    pub fn parse(bytes: &[u8]) -> Result<Self, PdfDiffError> {
        Self::parse_with_config(bytes, ParseConfig::default())
    }

    pub fn parse_with_config(bytes: &[u8], config: ParseConfig) -> Result<Self, PdfDiffError> {
        config.limits.check_file_size(bytes.len())?;
        let mut document = parse_header(bytes)?;
        if keyword_count(bytes, b"/Encrypt") > 0 {
            return Err(PdfDiffError::UnsupportedPdf(
                "UNSUPPORTED_ENCRYPTION: encrypted or protected PDF is not supported".into(),
            ));
        }
        document.incremental_update = incremental_update_info(bytes);
        append_incremental_update_diagnostics(
            document.incremental_update.as_ref(),
            &mut document.diagnostics,
        );
        document.objects = match parse_object_store(bytes, config) {
            Ok(store) => pdf_objects_from_object_store(store.objects),
            Err(error) => {
                if has_xref_surface(bytes) {
                    document.diagnostics.push(Diagnostic::warning(
                        "XREF_RECOVERY_USED",
                        format!(
                            "xref/object-store parsing failed ({error}); recovered by scanning indirect objects"
                        ),
                    ));
                }
                parse_indirect_objects(bytes, &config)?
            }
        };
        if document.objects.len() > config.limits.max_objects {
            return Err(resource_limit_error(
                "RESOURCE_LIMIT_OBJECT_COUNT",
                format!(
                    "file has {} indirect objects, limit is {}",
                    document.objects.len(),
                    config.limits.max_objects
                ),
            ));
        }
        document
            .diagnostics
            .extend(scan_unsupported_features(&document.objects));
        document.pages =
            resolve_pages(&document.objects, config.limits, &mut document.diagnostics)?;
        Ok(document)
    }

    #[must_use]
    pub fn first_page_content(&self) -> Option<PageContent<'_>> {
        self.first_page_contents()?.into_iter().next()
    }

    #[must_use]
    pub fn first_page_contents(&self) -> Option<Vec<PageContent<'_>>> {
        let page = self.pages.first()?;
        let contents = self.contents_for_page(page);
        if contents.is_empty() {
            None
        } else {
            Some(contents)
        }
    }

    #[must_use]
    pub fn page_contents(&self) -> Vec<PageContent<'_>> {
        self.pages
            .iter()
            .flat_map(|page| self.contents_for_page(page))
            .collect()
    }

    fn contents_for_page(&self, page: &PdfPage) -> Vec<PageContent<'_>> {
        page.content_object_ids
            .iter()
            .filter_map(|content_object_id| {
                let object = self
                    .objects
                    .iter()
                    .find(|object| object.id == *content_object_id)?;
                let stream = object.stream.as_ref()?;
                Some(PageContent {
                    page_index: page.page_index,
                    page_object_id: page.object_id,
                    stream_object_id: object.id,
                    bytes: &stream.bytes,
                    byte_range: stream.byte_range,
                })
            })
            .collect()
    }

    #[must_use]
    pub fn tagged_structure(&self, config: ParseConfig) -> TaggedStructure {
        let mut diagnostics = Vec::new();
        let Some(root_object_id) = struct_tree_root_id(self, config, &mut diagnostics) else {
            return TaggedStructure {
                root_object_id: None,
                role_map: Vec::new(),
                roots: Vec::new(),
                parent_tree: Vec::new(),
                diagnostics,
            };
        };
        let role_map = parse_structure_role_map(self, root_object_id, config, &mut diagnostics);
        let mut seen = Vec::new();
        let mut context = StructureParseContext {
            config,
            role_map: &role_map,
            diagnostics: &mut diagnostics,
        };
        let parsed = parse_structure_reference(self, root_object_id, 0, &mut seen, &mut context);
        let parent_tree = parse_parent_tree(self, root_object_id, config, &mut diagnostics);
        TaggedStructure {
            root_object_id: Some(root_object_id),
            role_map,
            roots: parsed.elements,
            parent_tree,
            diagnostics,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct StructureParseResult {
    elements: Vec<TaggedStructureElement>,
    mcids: Vec<usize>,
}

struct StructureParseContext<'a> {
    config: ParseConfig,
    role_map: &'a [TaggedRoleMapEntry],
    diagnostics: &'a mut Vec<Diagnostic>,
}

fn struct_tree_root_id(
    document: &PdfDocument,
    config: ParseConfig,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<ObjectId> {
    for object in &document.objects {
        let Ok(value) = parse_primitive(object.body.as_bytes(), config).map(|parsed| parsed.value)
        else {
            continue;
        };
        if !matches!(
            dictionary_value(&value, "Type"),
            Some(PdfPrimitive::Name(name)) if name == "Catalog"
        ) {
            continue;
        }
        match dictionary_value(&value, "StructTreeRoot") {
            Some(PdfPrimitive::Reference(id)) => return Some(*id),
            Some(_) => diagnostics.push(
                Diagnostic::warning(
                    "TAGGED_STRUCTURE_MALFORMED",
                    "catalog /StructTreeRoot is not an indirect reference",
                )
                .with_object(object.id),
            ),
            None => {}
        }
    }
    None
}

fn parse_structure_reference(
    document: &PdfDocument,
    object_id: ObjectId,
    depth: usize,
    seen: &mut Vec<ObjectId>,
    context: &mut StructureParseContext<'_>,
) -> StructureParseResult {
    if depth > context.config.limits.max_indirect_depth {
        context.diagnostics.push(
            Diagnostic::warning(
                "TAGGED_STRUCTURE_DEPTH_LIMIT",
                format!(
                    "tagged structure exceeds reference depth limit of {}",
                    context.config.limits.max_indirect_depth
                ),
            )
            .with_object(object_id),
        );
        return StructureParseResult::default();
    }
    if seen.contains(&object_id) {
        context.diagnostics.push(
            Diagnostic::warning(
                "TAGGED_STRUCTURE_CYCLE",
                format!(
                    "tagged structure contains a cycle at object {}",
                    object_id.number
                ),
            )
            .with_object(object_id),
        );
        return StructureParseResult::default();
    }
    let Some(object) = document
        .objects
        .iter()
        .find(|object| object.id == object_id)
    else {
        context.diagnostics.push(
            Diagnostic::warning(
                "TAGGED_STRUCTURE_MISSING_OBJECT",
                format!(
                    "tagged structure references missing object {}",
                    object_id.number
                ),
            )
            .with_object(object_id),
        );
        return StructureParseResult::default();
    };
    let value =
        match parse_primitive(object.body.as_bytes(), context.config).map(|parsed| parsed.value) {
            Ok(value) => value,
            Err(error) => {
                context.diagnostics.push(
                    Diagnostic::warning(
                        "TAGGED_STRUCTURE_MALFORMED",
                        format!(
                            "tagged structure object {} could not be parsed: {error}",
                            object_id.number
                        ),
                    )
                    .with_object(object_id),
                );
                return StructureParseResult::default();
            }
        };
    seen.push(object_id);
    let result = parse_structure_value(document, &value, Some(object_id), depth + 1, seen, context);
    seen.pop();
    result
}

fn parse_structure_value(
    document: &PdfDocument,
    value: &PdfPrimitive,
    object_id: Option<ObjectId>,
    depth: usize,
    seen: &mut Vec<ObjectId>,
    context: &mut StructureParseContext<'_>,
) -> StructureParseResult {
    match value {
        PdfPrimitive::Reference(id) => {
            parse_structure_reference(document, *id, depth, seen, context)
        }
        PdfPrimitive::Array(items) => {
            let mut result = StructureParseResult::default();
            for item in items {
                let child = parse_structure_value(document, item, None, depth, seen, context);
                result.elements.extend(child.elements);
                result.mcids.extend(child.mcids);
            }
            result
        }
        PdfPrimitive::Integer(mcid) => match usize::try_from(*mcid) {
            Ok(mcid) => StructureParseResult {
                elements: Vec::new(),
                mcids: vec![mcid],
            },
            Err(_) => {
                if let Some(object_id) = object_id {
                    context.diagnostics.push(
                        Diagnostic::warning(
                            "TAGGED_STRUCTURE_MALFORMED",
                            "tagged structure contains a negative MCID",
                        )
                        .with_object(object_id),
                    );
                }
                StructureParseResult::default()
            }
        },
        PdfPrimitive::Dictionary(_) => {
            parse_structure_dictionary(document, value, object_id, depth, seen, context)
        }
        PdfPrimitive::Null
        | PdfPrimitive::Boolean(_)
        | PdfPrimitive::Real(_)
        | PdfPrimitive::Name(_)
        | PdfPrimitive::LiteralString(_)
        | PdfPrimitive::HexString(_) => StructureParseResult::default(),
    }
}

fn parse_structure_dictionary(
    document: &PdfDocument,
    value: &PdfPrimitive,
    object_id: Option<ObjectId>,
    depth: usize,
    seen: &mut Vec<ObjectId>,
    context: &mut StructureParseContext<'_>,
) -> StructureParseResult {
    if let Some(PdfPrimitive::Integer(mcid)) = dictionary_value(value, "MCID") {
        return match usize::try_from(*mcid) {
            Ok(mcid) => StructureParseResult {
                elements: Vec::new(),
                mcids: vec![mcid],
            },
            Err(_) => StructureParseResult::default(),
        };
    }

    let children = dictionary_value(value, "K")
        .map(|children| parse_structure_value(document, children, None, depth + 1, seen, context))
        .unwrap_or_default();

    if let Some(PdfPrimitive::Name(structure_type)) = dictionary_value(value, "S") {
        return StructureParseResult {
            elements: vec![TaggedStructureElement {
                object_id,
                structure_type: structure_type.clone(),
                mapped_structure_type: mapped_structure_type(structure_type, context.role_map),
                mcids: children.mcids,
                children: children.elements,
            }],
            mcids: Vec::new(),
        };
    }

    children
}

fn parse_structure_role_map(
    document: &PdfDocument,
    root_object_id: ObjectId,
    config: ParseConfig,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<TaggedRoleMapEntry> {
    let Some(root_object) = document
        .objects
        .iter()
        .find(|object| object.id == root_object_id)
    else {
        return Vec::new();
    };
    let Ok(root_value) =
        parse_primitive(root_object.body.as_bytes(), config).map(|parsed| parsed.value)
    else {
        return Vec::new();
    };
    let Some(role_map_value) = dictionary_value(&root_value, "RoleMap") else {
        return Vec::new();
    };
    let PdfPrimitive::Dictionary(entries) = role_map_value else {
        diagnostics.push(
            Diagnostic::warning(
                "TAGGED_ROLE_MAP_MALFORMED",
                "tagged /RoleMap is not a dictionary",
            )
            .with_object(root_object_id),
        );
        return Vec::new();
    };
    let mut role_map = Vec::new();
    for (source, target) in entries {
        match target {
            PdfPrimitive::Name(target) => role_map.push(TaggedRoleMapEntry {
                source: source.clone(),
                target: target.clone(),
            }),
            _ => diagnostics.push(
                Diagnostic::warning(
                    "TAGGED_ROLE_MAP_MALFORMED",
                    format!("tagged /RoleMap entry /{source} does not map to a name"),
                )
                .with_object(root_object_id),
            ),
        }
    }
    role_map.sort_by(|left, right| {
        left.source
            .cmp(&right.source)
            .then_with(|| left.target.cmp(&right.target))
    });
    role_map.dedup_by(|left, right| left.source == right.source && left.target == right.target);
    role_map
}

fn mapped_structure_type(structure_type: &str, role_map: &[TaggedRoleMapEntry]) -> Option<String> {
    role_map
        .iter()
        .find(|entry| entry.source == structure_type)
        .map(|entry| entry.target.clone())
        .filter(|target| target != structure_type)
}

fn parse_parent_tree(
    document: &PdfDocument,
    root_object_id: ObjectId,
    config: ParseConfig,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<TaggedParentTreeEntry> {
    let Some(root_object) = document
        .objects
        .iter()
        .find(|object| object.id == root_object_id)
    else {
        return Vec::new();
    };
    let Ok(root_value) =
        parse_primitive(root_object.body.as_bytes(), config).map(|parsed| parsed.value)
    else {
        return Vec::new();
    };
    let Some(parent_tree_value) = dictionary_value(&root_value, "ParentTree") else {
        return Vec::new();
    };
    let mut seen = Vec::new();
    let mut entries = parse_parent_tree_value(
        document,
        parent_tree_value,
        config,
        0,
        &mut seen,
        diagnostics,
    );
    entries.sort_by_key(|entry| entry.struct_parent);
    entries.dedup_by_key(|entry| entry.struct_parent);
    entries
}

fn parse_parent_tree_value(
    document: &PdfDocument,
    value: &PdfPrimitive,
    config: ParseConfig,
    depth: usize,
    seen: &mut Vec<ObjectId>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<TaggedParentTreeEntry> {
    match value {
        PdfPrimitive::Reference(object_id) => {
            if depth > config.limits.max_indirect_depth || seen.contains(object_id) {
                diagnostics.push(
                    Diagnostic::warning(
                        "TAGGED_PARENT_TREE_LIMIT",
                        "tagged parent tree exceeded reference limits or contained a cycle",
                    )
                    .with_object(*object_id),
                );
                return Vec::new();
            }
            let Some(object) = document
                .objects
                .iter()
                .find(|object| object.id == *object_id)
            else {
                diagnostics.push(
                    Diagnostic::warning(
                        "TAGGED_PARENT_TREE_MISSING_OBJECT",
                        format!(
                            "tagged parent tree references missing object {}",
                            object_id.number
                        ),
                    )
                    .with_object(*object_id),
                );
                return Vec::new();
            };
            let Ok(parsed) =
                parse_primitive(object.body.as_bytes(), config).map(|parsed| parsed.value)
            else {
                diagnostics.push(
                    Diagnostic::warning(
                        "TAGGED_PARENT_TREE_MALFORMED",
                        format!(
                            "tagged parent tree object {} could not be parsed",
                            object_id.number
                        ),
                    )
                    .with_object(*object_id),
                );
                return Vec::new();
            };
            seen.push(*object_id);
            let entries =
                parse_parent_tree_value(document, &parsed, config, depth + 1, seen, diagnostics);
            seen.pop();
            entries
        }
        PdfPrimitive::Dictionary(_) => {
            parse_parent_tree_dictionary(document, value, config, depth + 1, seen, diagnostics)
        }
        PdfPrimitive::Null
        | PdfPrimitive::Boolean(_)
        | PdfPrimitive::Integer(_)
        | PdfPrimitive::Real(_)
        | PdfPrimitive::Name(_)
        | PdfPrimitive::LiteralString(_)
        | PdfPrimitive::HexString(_)
        | PdfPrimitive::Array(_) => Vec::new(),
    }
}

fn parse_parent_tree_dictionary(
    document: &PdfDocument,
    value: &PdfPrimitive,
    config: ParseConfig,
    depth: usize,
    seen: &mut Vec<ObjectId>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<TaggedParentTreeEntry> {
    let mut entries = dictionary_value(value, "Nums")
        .map(parse_parent_tree_nums)
        .unwrap_or_default();
    if let Some(PdfPrimitive::Array(kids)) = dictionary_value(value, "Kids") {
        for kid in kids {
            entries.extend(parse_parent_tree_value(
                document,
                kid,
                config,
                depth + 1,
                seen,
                diagnostics,
            ));
        }
    }
    entries
}

fn parse_parent_tree_nums(value: &PdfPrimitive) -> Vec<TaggedParentTreeEntry> {
    let PdfPrimitive::Array(items) = value else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for pair in items.chunks(2) {
        let [PdfPrimitive::Integer(key), value] = pair else {
            continue;
        };
        let Ok(struct_parent) = usize::try_from(*key) else {
            continue;
        };
        let element_object_ids = parent_tree_value_object_ids(value);
        if !element_object_ids.is_empty() {
            entries.push(TaggedParentTreeEntry {
                struct_parent,
                element_object_ids,
            });
        }
    }
    entries
}

fn parent_tree_value_object_ids(value: &PdfPrimitive) -> Vec<ObjectId> {
    match value {
        PdfPrimitive::Reference(object_id) => vec![*object_id],
        PdfPrimitive::Array(items) => items
            .iter()
            .filter_map(|item| match item {
                PdfPrimitive::Reference(object_id) => Some(*object_id),
                _ => None,
            })
            .collect(),
        PdfPrimitive::Null
        | PdfPrimitive::Boolean(_)
        | PdfPrimitive::Integer(_)
        | PdfPrimitive::Real(_)
        | PdfPrimitive::Name(_)
        | PdfPrimitive::LiteralString(_)
        | PdfPrimitive::HexString(_)
        | PdfPrimitive::Dictionary(_) => Vec::new(),
    }
}

fn incremental_update_info(bytes: &[u8]) -> Option<IncrementalUpdateInfo> {
    let revision_count = keyword_count(bytes, b"startxref");
    let startxref_offsets = startxref_offsets(bytes);
    let trailer_prev_offsets = trailer_prev_offsets(bytes);
    if revision_count <= 1 && trailer_prev_offsets.is_empty() {
        return None;
    }

    Some(IncrementalUpdateInfo {
        revision_count,
        selected_startxref_offset: startxref_offsets.last().copied(),
        prior_startxref_offsets: startxref_offsets
            .get(..startxref_offsets.len().saturating_sub(1))
            .unwrap_or_default()
            .to_vec(),
        trailer_prev_offsets,
    })
}

fn append_incremental_update_diagnostics(
    info: Option<&IncrementalUpdateInfo>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(info) = info else {
        return;
    };
    if info.revision_count > 1 {
        diagnostics.push(Diagnostic::info(
            "INCREMENTAL_UPDATE_DETECTED",
            format!(
                "PDF contains {} startxref sections; the latest revision was selected",
                info.revision_count
            ),
        ));
    }
    if !info.trailer_prev_offsets.is_empty() {
        diagnostics.push(Diagnostic::info(
            "PRIOR_REVISION_PRESENT",
            "xref trailer contains /Prev; prior revision offsets are exposed in parser metadata",
        ));
    }
}

fn startxref_offsets(bytes: &[u8]) -> Vec<usize> {
    let mut offsets = Vec::new();
    let mut cursor = 0usize;
    while let Some(marker) = find_keyword(bytes, cursor, b"startxref") {
        let mut parser = XrefParser::new(bytes, marker + b"startxref".len());
        parser.skip_ascii_whitespace();
        if let Ok(offset) = parser.read_usize("startxref offset") {
            offsets.push(offset);
        }
        cursor = marker + b"startxref".len();
    }
    offsets
}

fn trailer_prev_offsets(bytes: &[u8]) -> Vec<usize> {
    let mut offsets = Vec::new();
    let mut cursor = 0usize;
    while let Some(marker) = find_keyword(bytes, cursor, b"/Prev") {
        cursor = marker + b"/Prev".len();
        skip_ascii_whitespace_bytes(bytes, &mut cursor);
        let start = cursor;
        while matches!(bytes.get(cursor), Some(byte) if byte.is_ascii_digit()) {
            cursor += 1;
        }
        if cursor > start {
            if let Ok(value) = std::str::from_utf8(&bytes[start..cursor])
                .unwrap_or_default()
                .parse::<usize>()
            {
                offsets.push(value);
            }
        }
    }
    offsets
}

fn keyword_count(bytes: &[u8], keyword: &[u8]) -> usize {
    bytes
        .windows(keyword.len())
        .filter(|window| *window == keyword)
        .count()
}

fn has_xref_surface(bytes: &[u8]) -> bool {
    keyword_count(bytes, b"startxref") > 0 || keyword_count(bytes, b"xref") > 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageContent<'a> {
    pub page_index: usize,
    pub page_object_id: ObjectId,
    pub stream_object_id: ObjectId,
    pub bytes: &'a [u8],
    pub byte_range: ByteRange,
}

#[must_use]
pub fn default_resource_limits() -> ResourceLimits {
    ResourceLimits::default()
}

pub fn parse_primitive(bytes: &[u8], config: ParseConfig) -> Result<ParsedPrimitive, PdfDiffError> {
    config.limits.check_file_size(bytes.len())?;
    let mut parser = PrimitiveParser::new(bytes, config);
    let start = parser.skip_whitespace_and_comments();
    let value = parser.parse_value(0)?;
    let end = parser.index;
    parser.skip_whitespace_and_comments();
    if !parser.is_eof() {
        return Err(PdfDiffError::InvalidInput(
            "trailing bytes after PDF primitive".into(),
        ));
    }
    Ok(ParsedPrimitive {
        value,
        byte_range: ByteRange::new(start, end),
    })
}

fn resource_limit_error(code: &str, message: impl Into<String>) -> PdfDiffError {
    PdfDiffError::ResourceLimitExceeded(format!("{code}: {}", message.into()))
}

pub fn parse_indirect_object(
    bytes: &[u8],
    config: ParseConfig,
) -> Result<ParsedIndirectObject, PdfDiffError> {
    config.limits.check_file_size(bytes.len())?;
    let mut parser = PrimitiveParser::new(bytes, config);
    let object_start = parser.skip_whitespace_and_comments();
    let object_number = parser.parse_unsigned_object_number("object number")?;
    parser.skip_whitespace_and_comments();
    let generation = parser.parse_unsigned_generation()?;
    parser.skip_whitespace_and_comments();
    parser.expect_keyword(b"obj")?;
    let value_start = parser.skip_whitespace_and_comments();

    let parts = split_indirect_object_value_and_stream(bytes, value_start, config)?;
    let parsed = parse_primitive(parts.value_bytes, config)?;
    let stream = parts
        .stream
        .map(|raw_stream| {
            decode_stream(
                raw_stream.with_metadata(
                    stream_length_from_primitive(&parsed.value),
                    stream_filters_from_primitive(&parsed.value),
                    decode_params_from_primitive(&parsed.value),
                ),
                config,
            )
        })
        .transpose()?;
    let mut end_parser = PrimitiveParser::new(&bytes[parts.value_end..], config);
    end_parser.skip_whitespace_and_comments();
    end_parser.expect_keyword(b"endobj")?;
    let object_end = parts.value_end + end_parser.index;
    end_parser.skip_whitespace_and_comments();
    if parts.value_end + end_parser.index < bytes.len() {
        return Err(PdfDiffError::InvalidInput(
            "trailing bytes after indirect object".into(),
        ));
    }

    Ok(ParsedIndirectObject {
        id: ObjectId {
            number: object_number,
            generation,
        },
        value: parsed.value,
        stream,
        byte_range: ByteRange::new(object_start, object_end),
        value_byte_range: ByteRange::new(
            parts.value_offset + parsed.byte_range.start,
            parts.value_offset + parsed.byte_range.end,
        ),
        embedded_source: None,
    })
}

pub fn parse_xref_table(bytes: &[u8], config: ParseConfig) -> Result<XrefTable, PdfDiffError> {
    config.limits.check_file_size(bytes.len())?;
    let start_offset = locate_startxref(bytes)?;
    if bytes
        .get(start_offset..)
        .is_some_and(|remaining| !remaining.starts_with(b"xref"))
    {
        return parse_xref_stream_at(bytes, start_offset, config);
    }
    let mut parser = XrefParser::new(bytes, start_offset);
    parser.expect_keyword(b"xref")?;
    let mut entries = Vec::new();

    loop {
        parser.skip_ascii_whitespace();
        if parser.starts_with(b"trailer") {
            parser.expect_keyword(b"trailer")?;
            break;
        }
        let first_object = parser.read_usize("xref subsection first object")?;
        parser.skip_inline_whitespace();
        let count = parser.read_usize("xref subsection count")?;
        parser.skip_line_break();
        for index in 0..count {
            let byte_offset = parser.read_fixed_width_usize(10, "xref byte offset")?;
            parser.skip_inline_whitespace();
            let generation = parser.read_fixed_width_u16(5, "xref generation")?;
            parser.skip_inline_whitespace();
            let in_use = match parser.next_byte() {
                Some(b'n') => true,
                Some(b'f') => false,
                Some(byte) => {
                    return Err(PdfDiffError::InvalidInput(format!(
                        "invalid xref entry flag 0x{byte:02x}"
                    )));
                }
                None => {
                    return Err(PdfDiffError::InvalidInput(
                        "truncated xref entry flag".into(),
                    ));
                }
            };
            parser.skip_line_break();
            entries.push(XrefEntry {
                object_id: ObjectId {
                    number: u32::try_from(first_object + index).map_err(|_| {
                        PdfDiffError::InvalidInput("xref object number is out of range".into())
                    })?,
                    generation,
                },
                byte_offset,
                in_use,
                kind: if in_use {
                    XrefEntryKind::InUse
                } else {
                    XrefEntryKind::Free
                },
            });
        }
    }

    if entries.len() > config.limits.max_objects {
        return Err(resource_limit_error(
            "RESOURCE_LIMIT_OBJECT_COUNT",
            format!(
                "xref table has {} entries, limit is {}",
                entries.len(),
                config.limits.max_objects
            ),
        ));
    }

    let trailer_start = parser.skip_ascii_whitespace();
    let startxref_marker = find_keyword(bytes, trailer_start, b"startxref").ok_or_else(|| {
        PdfDiffError::InvalidInput("xref trailer is missing startxref marker".into())
    })?;
    let trailer = parse_primitive(&bytes[trailer_start..startxref_marker], config)?.value;

    Ok(XrefTable {
        start_offset,
        entries,
        trailer,
    })
}

pub fn parse_object_store(bytes: &[u8], config: ParseConfig) -> Result<ObjectStore, PdfDiffError> {
    let xref = parse_xref_table(bytes, config)?;
    let mut objects = Vec::new();
    for entry in xref
        .entries
        .iter()
        .filter(|entry| entry.kind == XrefEntryKind::InUse && entry.object_id.number != 0)
    {
        if entry.byte_offset >= bytes.len() {
            return Err(PdfDiffError::InvalidInput(format!(
                "xref entry for object {} points outside the file",
                entry.object_id.number
            )));
        }
        let object_end = find_keyword(bytes, entry.byte_offset, b"endobj")
            .map(|offset| offset + b"endobj".len())
            .ok_or_else(|| {
                PdfDiffError::InvalidInput(format!(
                    "xref entry for object {} points to an unterminated object",
                    entry.object_id.number
                ))
            })?;
        let mut object = parse_indirect_object(&bytes[entry.byte_offset..object_end], config)?;
        offset_indirect_object_ranges(&mut object, entry.byte_offset);
        if object.id != entry.object_id {
            return Err(PdfDiffError::InvalidInput(format!(
                "xref entry for object {} resolved to object {}",
                entry.object_id.number, object.id.number
            )));
        }
        objects.push(object);
    }
    let mut embedded_objects = extract_object_streams(&objects, config)?;
    objects.append(&mut embedded_objects);
    objects.sort_by_key(|object| (object.id.number, object.id.generation));
    Ok(ObjectStore { xref, objects })
}

fn offset_indirect_object_ranges(object: &mut ParsedIndirectObject, offset: usize) {
    object.byte_range = offset_range(object.byte_range, offset);
    object.value_byte_range = offset_range(object.value_byte_range, offset);
    if let Some(stream) = &mut object.stream {
        stream.byte_range = offset_range(stream.byte_range, offset);
    }
}

fn offset_range(range: ByteRange, offset: usize) -> ByteRange {
    ByteRange::new(range.start + offset, range.end + offset)
}

fn extract_object_streams(
    objects: &[ParsedIndirectObject],
    config: ParseConfig,
) -> Result<Vec<ParsedIndirectObject>, PdfDiffError> {
    let mut embedded = Vec::new();
    for object in objects {
        if !matches!(
            dictionary_value(&object.value, "Type"),
            Some(PdfPrimitive::Name(name)) if name == "ObjStm"
        ) {
            continue;
        }
        embedded.extend(extract_object_stream(object, config)?);
    }
    Ok(embedded)
}

fn extract_object_stream(
    object: &ParsedIndirectObject,
    config: ParseConfig,
) -> Result<Vec<ParsedIndirectObject>, PdfDiffError> {
    let Some(stream) = &object.stream else {
        return Err(PdfDiffError::InvalidInput(format!(
            "object stream {} does not contain a stream",
            object.id.number
        )));
    };
    if !stream.decoded {
        return Err(PdfDiffError::InvalidInput(format!(
            "object stream {} could not be decoded",
            object.id.number
        )));
    }
    let object_count = required_dictionary_usize(&object.value, "N")?;
    let first_object_offset = required_dictionary_usize(&object.value, "First")?;
    if object_count > config.limits.max_objects {
        return Err(resource_limit_error(
            "RESOURCE_LIMIT_OBJECT_COUNT",
            format!(
                "object stream {} contains {object_count} objects, limit is {}",
                object.id.number, config.limits.max_objects
            ),
        ));
    }
    let header = stream.bytes.get(..first_object_offset).ok_or_else(|| {
        PdfDiffError::InvalidInput(format!(
            "object stream {} has /First outside the decoded stream",
            object.id.number
        ))
    })?;
    let offsets = parse_object_stream_offsets(header, object_count, object.id)?;
    let mut embedded = Vec::new();
    for (index, (object_number, relative_offset)) in offsets.iter().copied().enumerate() {
        let object_start = first_object_offset
            .checked_add(relative_offset)
            .ok_or_else(|| PdfDiffError::InvalidInput("object stream offset overflow".into()))?;
        let object_end = offsets
            .get(index + 1)
            .map_or(stream.bytes.len(), |(_, next_offset)| {
                first_object_offset + *next_offset
            });
        if object_start > object_end || object_end > stream.bytes.len() {
            return Err(PdfDiffError::InvalidInput(format!(
                "object stream {} has malformed embedded object offsets",
                object.id.number
            )));
        }
        let object_bytes = trim_trailing_ascii(&stream.bytes[object_start..object_end]);
        let parsed = parse_primitive(object_bytes, config)?;
        embedded.push(ParsedIndirectObject {
            id: ObjectId {
                number: object_number,
                generation: 0,
            },
            value: parsed.value,
            stream: None,
            byte_range: object
                .stream
                .as_ref()
                .map_or(object.byte_range, |source_stream| source_stream.byte_range),
            value_byte_range: ByteRange::new(
                object_start + parsed.byte_range.start,
                object_start + parsed.byte_range.end,
            ),
            embedded_source: Some(EmbeddedObjectSource {
                object_stream_id: object.id,
                object_index: index,
            }),
        });
    }
    Ok(embedded)
}

fn parse_object_stream_offsets(
    bytes: &[u8],
    object_count: usize,
    object_stream_id: ObjectId,
) -> Result<Vec<(u32, usize)>, PdfDiffError> {
    let header = std::str::from_utf8(bytes).map_err(|_| {
        PdfDiffError::InvalidInput(format!(
            "object stream {} has a non-UTF-8 offset table",
            object_stream_id.number
        ))
    })?;
    let mut parts = header.split_ascii_whitespace();
    let mut offsets = Vec::new();
    for _ in 0..object_count {
        let object_number = parts
            .next()
            .ok_or_else(|| {
                PdfDiffError::InvalidInput(format!(
                    "object stream {} offset table is truncated",
                    object_stream_id.number
                ))
            })?
            .parse::<u32>()
            .map_err(|_| {
                PdfDiffError::InvalidInput(format!(
                    "object stream {} has invalid embedded object number",
                    object_stream_id.number
                ))
            })?;
        let offset = parts
            .next()
            .ok_or_else(|| {
                PdfDiffError::InvalidInput(format!(
                    "object stream {} offset table is truncated",
                    object_stream_id.number
                ))
            })?
            .parse::<usize>()
            .map_err(|_| {
                PdfDiffError::InvalidInput(format!(
                    "object stream {} has invalid embedded object offset",
                    object_stream_id.number
                ))
            })?;
        offsets.push((object_number, offset));
    }
    Ok(offsets)
}

fn required_dictionary_usize(value: &PdfPrimitive, key: &str) -> Result<usize, PdfDiffError> {
    let Some(PdfPrimitive::Integer(number)) = dictionary_value(value, key) else {
        return Err(PdfDiffError::InvalidInput(format!(
            "dictionary is missing integer /{key}"
        )));
    };
    usize::try_from(*number)
        .map_err(|_| PdfDiffError::InvalidInput(format!("dictionary /{key} is out of range")))
}

fn parse_xref_stream_at(
    bytes: &[u8],
    start_offset: usize,
    config: ParseConfig,
) -> Result<XrefTable, PdfDiffError> {
    let object_end = find_keyword(bytes, start_offset, b"endobj")
        .map(|offset| offset + b"endobj".len())
        .ok_or_else(|| PdfDiffError::InvalidInput("xref stream object is missing endobj".into()))?;
    let object = parse_indirect_object(&bytes[start_offset..object_end], config)?;
    let PdfPrimitive::Dictionary(_) = object.value else {
        return Err(PdfDiffError::InvalidInput(
            "xref stream object value must be a dictionary".into(),
        ));
    };
    if !matches!(
        dictionary_value(&object.value, "Type"),
        Some(PdfPrimitive::Name(name)) if name == "XRef"
    ) {
        return Err(PdfDiffError::InvalidInput(
            "startxref does not point to a classic xref table or /XRef stream".into(),
        ));
    }
    let Some(stream) = object.stream else {
        return Err(PdfDiffError::InvalidInput(
            "xref stream object does not contain a stream".into(),
        ));
    };
    if !stream.decoded {
        return Err(PdfDiffError::InvalidInput(
            "xref stream could not be decoded".into(),
        ));
    }

    let widths = xref_stream_widths(&object.value)?;
    let indices = xref_stream_indices(&object.value)?;
    let entries = parse_xref_stream_entries(&stream.bytes, &widths, &indices, config.limits)?;

    Ok(XrefTable {
        start_offset,
        entries,
        trailer: object.value,
    })
}

fn xref_stream_widths(value: &PdfPrimitive) -> Result<[usize; 3], PdfDiffError> {
    let Some(PdfPrimitive::Array(items)) = dictionary_value(value, "W") else {
        return Err(PdfDiffError::InvalidInput(
            "xref stream is missing /W array".into(),
        ));
    };
    if items.len() != 3 {
        return Err(PdfDiffError::InvalidInput(
            "xref stream /W array must have three entries".into(),
        ));
    }
    let mut widths = [0usize; 3];
    for (index, item) in items.iter().enumerate() {
        let PdfPrimitive::Integer(width) = item else {
            return Err(PdfDiffError::InvalidInput(
                "xref stream /W entries must be integers".into(),
            ));
        };
        widths[index] = usize::try_from(*width).map_err(|_| {
            PdfDiffError::InvalidInput("xref stream /W entry is out of range".into())
        })?;
    }
    if widths == [0, 0, 0] {
        return Err(PdfDiffError::InvalidInput(
            "xref stream /W array cannot be all zero".into(),
        ));
    }
    Ok(widths)
}

fn xref_stream_indices(value: &PdfPrimitive) -> Result<Vec<(usize, usize)>, PdfDiffError> {
    if let Some(PdfPrimitive::Array(items)) = dictionary_value(value, "Index") {
        if items.len() % 2 != 0 {
            return Err(PdfDiffError::InvalidInput(
                "xref stream /Index array must contain object/count pairs".into(),
            ));
        }
        let mut pairs = Vec::new();
        for pair in items.chunks(2) {
            let [PdfPrimitive::Integer(first), PdfPrimitive::Integer(count)] = pair else {
                return Err(PdfDiffError::InvalidInput(
                    "xref stream /Index entries must be integers".into(),
                ));
            };
            pairs.push((
                usize::try_from(*first).map_err(|_| {
                    PdfDiffError::InvalidInput(
                        "xref stream /Index object number is out of range".into(),
                    )
                })?,
                usize::try_from(*count).map_err(|_| {
                    PdfDiffError::InvalidInput("xref stream /Index count is out of range".into())
                })?,
            ));
        }
        return Ok(pairs);
    }

    let Some(PdfPrimitive::Integer(size)) = dictionary_value(value, "Size") else {
        return Err(PdfDiffError::InvalidInput(
            "xref stream requires /Size when /Index is absent".into(),
        ));
    };
    Ok(vec![(
        0,
        usize::try_from(*size)
            .map_err(|_| PdfDiffError::InvalidInput("xref stream /Size is out of range".into()))?,
    )])
}

fn parse_xref_stream_entries(
    bytes: &[u8],
    widths: &[usize; 3],
    indices: &[(usize, usize)],
    limits: ResourceLimits,
) -> Result<Vec<XrefEntry>, PdfDiffError> {
    let entry_width = widths.iter().sum::<usize>();
    if entry_width == 0 {
        return Err(PdfDiffError::InvalidInput(
            "xref stream entry width is zero".into(),
        ));
    }
    let expected_entries = indices
        .iter()
        .try_fold(0usize, |total, (_, count)| total.checked_add(*count))
        .ok_or_else(|| PdfDiffError::InvalidInput("xref stream entry count overflow".into()))?;
    if expected_entries > limits.max_objects {
        return Err(resource_limit_error(
            "RESOURCE_LIMIT_OBJECT_COUNT",
            format!(
                "xref stream has {expected_entries} entries, limit is {}",
                limits.max_objects
            ),
        ));
    }
    let expected_bytes = expected_entries
        .checked_mul(entry_width)
        .ok_or_else(|| PdfDiffError::InvalidInput("xref stream byte count overflow".into()))?;
    if bytes.len() < expected_bytes {
        return Err(PdfDiffError::InvalidInput(
            "xref stream is shorter than /W and /Index require".into(),
        ));
    }

    let mut entries = Vec::new();
    let mut cursor = 0;
    for (first_object, count) in indices {
        for local_index in 0..*count {
            let field_type = read_xref_stream_field(bytes, &mut cursor, widths[0])?.unwrap_or(1);
            let field_2 = read_xref_stream_field(bytes, &mut cursor, widths[1])?.unwrap_or(0);
            let field_3 = read_xref_stream_field(bytes, &mut cursor, widths[2])?.unwrap_or(0);
            let object_number = u32::try_from(first_object + local_index).map_err(|_| {
                PdfDiffError::InvalidInput("xref stream object number is out of range".into())
            })?;
            let entry = match field_type {
                0 => XrefEntry {
                    object_id: ObjectId {
                        number: object_number,
                        generation: u16::try_from(field_3).map_err(|_| {
                            PdfDiffError::InvalidInput(
                                "xref stream generation is out of range".into(),
                            )
                        })?,
                    },
                    byte_offset: 0,
                    in_use: false,
                    kind: XrefEntryKind::Free,
                },
                1 => XrefEntry {
                    object_id: ObjectId {
                        number: object_number,
                        generation: u16::try_from(field_3).map_err(|_| {
                            PdfDiffError::InvalidInput(
                                "xref stream generation is out of range".into(),
                            )
                        })?,
                    },
                    byte_offset: field_2,
                    in_use: true,
                    kind: XrefEntryKind::InUse,
                },
                2 => XrefEntry {
                    object_id: ObjectId {
                        number: object_number,
                        generation: 0,
                    },
                    byte_offset: 0,
                    in_use: true,
                    kind: XrefEntryKind::Compressed {
                        object_stream: ObjectId {
                            number: u32::try_from(field_2).map_err(|_| {
                                PdfDiffError::InvalidInput(
                                    "xref stream object-stream number is out of range".into(),
                                )
                            })?,
                            generation: 0,
                        },
                        object_index: field_3,
                    },
                },
                other => {
                    return Err(PdfDiffError::InvalidInput(format!(
                        "unsupported xref stream entry type {other}"
                    )));
                }
            };
            entries.push(entry);
        }
    }
    Ok(entries)
}

fn read_xref_stream_field(
    bytes: &[u8],
    cursor: &mut usize,
    width: usize,
) -> Result<Option<usize>, PdfDiffError> {
    if width == 0 {
        return Ok(None);
    }
    let end = cursor
        .checked_add(width)
        .ok_or_else(|| PdfDiffError::InvalidInput("xref stream cursor overflow".into()))?;
    let Some(field) = bytes.get(*cursor..end) else {
        return Err(PdfDiffError::InvalidInput(
            "xref stream entry is truncated".into(),
        ));
    };
    *cursor = end;
    let mut value = 0usize;
    for byte in field {
        value = value
            .checked_mul(256)
            .and_then(|current| current.checked_add(usize::from(*byte)))
            .ok_or_else(|| PdfDiffError::InvalidInput("xref stream field overflow".into()))?;
    }
    Ok(Some(value))
}

struct IndirectValueParts<'a> {
    value_bytes: &'a [u8],
    value_offset: usize,
    value_end: usize,
    stream: Option<PdfStream>,
}

fn split_indirect_object_value_and_stream(
    bytes: &[u8],
    value_start: usize,
    config: ParseConfig,
) -> Result<IndirectValueParts<'_>, PdfDiffError> {
    let Some(relative_endobj) = find_keyword(bytes, value_start, b"endobj") else {
        return Err(PdfDiffError::InvalidInput(
            "indirect object is missing endobj".into(),
        ));
    };
    let body_end = relative_endobj;
    let Some(stream_marker) =
        find_keyword(bytes, value_start, b"stream").filter(|marker| *marker < body_end)
    else {
        let (value_bytes, leading_trim) = trim_ascii(&bytes[value_start..body_end]);
        return Ok(IndirectValueParts {
            value_bytes,
            value_offset: value_start + leading_trim,
            value_end: body_end,
            stream: None,
        });
    };
    let (value_bytes, leading_trim) = trim_ascii(&bytes[value_start..stream_marker]);
    let stream_data_start = match bytes.get(stream_marker + b"stream".len()) {
        Some(b'\r') if bytes.get(stream_marker + b"stream".len() + 1) == Some(&b'\n') => {
            stream_marker + b"stream".len() + 2
        }
        Some(b'\n') => stream_marker + b"stream".len() + 1,
        _ => stream_marker + b"stream".len(),
    };
    let Some(endstream_marker) =
        find_keyword(bytes, stream_data_start, b"endstream").filter(|marker| *marker < body_end)
    else {
        return Err(PdfDiffError::InvalidInput(
            "stream object is missing endstream".into(),
        ));
    };
    let stream_bytes = trim_trailing_ascii(&bytes[stream_data_start..endstream_marker]);
    if stream_bytes.len() > config.limits.max_stream_bytes {
        return Err(resource_limit_error(
            "RESOURCE_LIMIT_STREAM_BYTES",
            format!(
                "stream has {} bytes, limit is {}",
                stream_bytes.len(),
                config.limits.max_stream_bytes
            ),
        ));
    }

    Ok(IndirectValueParts {
        value_bytes,
        value_offset: value_start + leading_trim,
        value_end: body_end,
        stream: Some(PdfStream {
            bytes: stream_bytes.to_vec(),
            raw_bytes: stream_bytes.to_vec(),
            byte_range: ByteRange::new(stream_data_start, stream_data_start + stream_bytes.len()),
            declared_length: None,
            filters: Vec::new(),
            decode_params: Vec::new(),
            decoded: true,
        }),
    })
}

fn decode_stream(mut stream: PdfStream, config: ParseConfig) -> Result<PdfStream, PdfDiffError> {
    let mut decoded = stream.raw_bytes.clone();
    for filter in &stream.filters {
        let result = match filter.as_str() {
            "FlateDecode" | "Fl" => {
                flate_decode_limited(&decoded, config.limits.max_decoded_stream_bytes)
            }
            "ASCIIHexDecode" | "AHx" => {
                ascii_hex_decode_limited(&decoded, config.limits.max_decoded_stream_bytes)
            }
            "RunLengthDecode" | "RL" => {
                run_length_decode_limited(&decoded, config.limits.max_decoded_stream_bytes)
            }
            _ => {
                stream.bytes.clone_from(&stream.raw_bytes);
                stream.decoded = false;
                return Ok(stream);
            }
        };
        match result {
            Ok(next) => decoded = next,
            Err(PdfDiffError::ResourceLimitExceeded(message)) => {
                return Err(PdfDiffError::ResourceLimitExceeded(message));
            }
            Err(_) => {
                stream.bytes.clone_from(&stream.raw_bytes);
                stream.decoded = false;
                return Ok(stream);
            }
        }
    }
    stream.bytes = decoded;
    stream.decoded = true;
    Ok(stream)
}

fn ascii_hex_decode_limited(bytes: &[u8], limit: usize) -> Result<Vec<u8>, PdfDiffError> {
    let mut decoded = Vec::new();
    let mut high_nibble = None;
    for byte in bytes {
        if *byte == b'>' {
            break;
        }
        if byte.is_ascii_whitespace() {
            continue;
        }
        let Some(nibble) = hex_nibble(*byte) else {
            return Err(PdfDiffError::InvalidInput(
                "ASCIIHexDecode contained a non-hex byte".into(),
            ));
        };
        if let Some(high) = high_nibble.take() {
            push_decoded_byte_limited(&mut decoded, (high << 4) | nibble, limit)?;
        } else {
            high_nibble = Some(nibble);
        }
    }
    if let Some(high) = high_nibble {
        push_decoded_byte_limited(&mut decoded, high << 4, limit)?;
    }
    Ok(decoded)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn run_length_decode_limited(bytes: &[u8], limit: usize) -> Result<Vec<u8>, PdfDiffError> {
    let mut decoded = Vec::new();
    let mut index = 0;
    while let Some(length) = bytes.get(index).copied() {
        index += 1;
        match length {
            128 => break,
            0..=127 => {
                let count = usize::from(length) + 1;
                let end = index.checked_add(count).ok_or_else(|| {
                    PdfDiffError::InvalidInput("RunLengthDecode segment overflows".into())
                })?;
                let Some(segment) = bytes.get(index..end) else {
                    return Err(PdfDiffError::InvalidInput(
                        "RunLengthDecode literal segment is truncated".into(),
                    ));
                };
                ensure_decoded_capacity(decoded.len(), segment.len(), limit)?;
                decoded.extend_from_slice(segment);
                index = end;
            }
            129..=255 => {
                let count = 257usize - usize::from(length);
                let Some(byte) = bytes.get(index).copied() else {
                    return Err(PdfDiffError::InvalidInput(
                        "RunLengthDecode repeat segment is truncated".into(),
                    ));
                };
                index += 1;
                ensure_decoded_capacity(decoded.len(), count, limit)?;
                decoded.extend(std::iter::repeat_n(byte, count));
            }
        }
    }
    Ok(decoded)
}

fn push_decoded_byte_limited(
    decoded: &mut Vec<u8>,
    byte: u8,
    limit: usize,
) -> Result<(), PdfDiffError> {
    ensure_decoded_capacity(decoded.len(), 1, limit)?;
    decoded.push(byte);
    Ok(())
}

fn ensure_decoded_capacity(
    current_len: usize,
    additional: usize,
    limit: usize,
) -> Result<(), PdfDiffError> {
    if current_len.saturating_add(additional) > limit {
        return Err(resource_limit_error(
            "RESOURCE_LIMIT_DECODED_STREAM_BYTES",
            format!("decoded stream exceeds limit of {limit} bytes"),
        ));
    }
    Ok(())
}

fn flate_decode_limited(bytes: &[u8], limit: usize) -> Result<Vec<u8>, PdfDiffError> {
    let mut decoder = ZlibDecoder::new(bytes);
    let mut decoded = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let read = decoder
            .read(&mut chunk)
            .map_err(|error| PdfDiffError::InvalidInput(format!("FlateDecode failed: {error}")))?;
        if read == 0 {
            break;
        }
        if decoded.len().saturating_add(read) > limit {
            return Err(resource_limit_error(
                "RESOURCE_LIMIT_DECODED_STREAM_BYTES",
                format!("decoded stream exceeds limit of {limit} bytes"),
            ));
        }
        decoded.extend_from_slice(&chunk[..read]);
    }
    Ok(decoded)
}

fn stream_filters_from_primitive(value: &PdfPrimitive) -> Vec<String> {
    match dictionary_value(value, "Filter") {
        None => Vec::new(),
        Some(PdfPrimitive::Name(name)) => vec![name.clone()],
        Some(PdfPrimitive::Array(items)) => items
            .iter()
            .filter_map(|item| match item {
                PdfPrimitive::Name(name) => Some(name.clone()),
                _ => None,
            })
            .collect(),
        Some(_) => Vec::new(),
    }
}

fn decode_params_from_primitive(value: &PdfPrimitive) -> Vec<Option<String>> {
    match dictionary_value(value, "DecodeParms").or_else(|| dictionary_value(value, "DP")) {
        Some(PdfPrimitive::Array(items)) => items.iter().map(decode_param_signature).collect(),
        Some(value) => vec![decode_param_signature(value)],
        None => Vec::new(),
    }
}

fn decode_param_signature(value: &PdfPrimitive) -> Option<String> {
    match value {
        PdfPrimitive::Null => None,
        PdfPrimitive::Dictionary(entries) => Some(format!(
            "<<{}>>",
            entries
                .iter()
                .map(|(key, value)| format!("/{key} {}", primitive_signature(value)))
                .collect::<Vec<_>>()
                .join(" ")
        )),
        other => Some(primitive_signature(other)),
    }
}

fn primitive_signature(value: &PdfPrimitive) -> String {
    match value {
        PdfPrimitive::Null => "null".to_owned(),
        PdfPrimitive::Boolean(value) => value.to_string(),
        PdfPrimitive::Integer(value) => value.to_string(),
        PdfPrimitive::Real(value) => canonical_number(*value as f32),
        PdfPrimitive::Name(value) => format!("/{value}"),
        PdfPrimitive::LiteralString(bytes) => format!("({})", stable_hash(bytes)),
        PdfPrimitive::HexString(bytes) => format!("<{}>", stable_hash(bytes)),
        PdfPrimitive::Reference(id) => format!("{} {} R", id.number, id.generation),
        PdfPrimitive::Array(items) => format!(
            "[{}]",
            items
                .iter()
                .map(primitive_signature)
                .collect::<Vec<_>>()
                .join(" ")
        ),
        PdfPrimitive::Dictionary(entries) => format!(
            "<<{}>>",
            entries
                .iter()
                .map(|(key, value)| format!("/{key} {}", primitive_signature(value)))
                .collect::<Vec<_>>()
                .join(" ")
        ),
    }
}

fn stable_hash(bytes: &[u8]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn canonical_number(value: f32) -> String {
    if value == 0.0 {
        return "0".to_owned();
    }
    let mut text = format!("{value:.4}");
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

fn stream_length_from_primitive(value: &PdfPrimitive) -> Option<usize> {
    match dictionary_value(value, "Length")? {
        PdfPrimitive::Integer(length) => usize::try_from(*length).ok(),
        _ => None,
    }
}

fn dictionary_value<'a>(value: &'a PdfPrimitive, key: &str) -> Option<&'a PdfPrimitive> {
    let PdfPrimitive::Dictionary(entries) = value else {
        return None;
    };
    entries
        .iter()
        .find(|(entry_key, _)| entry_key == key)
        .map(|(_, entry_value)| entry_value)
}

struct PrimitiveParser<'a> {
    bytes: &'a [u8],
    index: usize,
    config: ParseConfig,
}

impl<'a> PrimitiveParser<'a> {
    fn new(bytes: &'a [u8], config: ParseConfig) -> Self {
        Self {
            bytes,
            index: 0,
            config,
        }
    }

    fn parse_value(&mut self, depth: usize) -> Result<PdfPrimitive, PdfDiffError> {
        if depth > self.config.limits.max_indirect_depth {
            return Err(resource_limit_error(
                "RESOURCE_LIMIT_RECURSION_DEPTH",
                format!(
                    "primitive nesting depth exceeds limit of {}",
                    self.config.limits.max_indirect_depth
                ),
            ));
        }

        self.skip_whitespace_and_comments();
        match self.peek() {
            Some(b'/') => self.parse_name().map(PdfPrimitive::Name),
            Some(b'(') => self.parse_literal_string().map(PdfPrimitive::LiteralString),
            Some(b'[') => self.parse_array(depth),
            Some(b'<') if self.peek_next() == Some(b'<') => self.parse_dictionary(depth),
            Some(b'<') => self.parse_hex_string().map(PdfPrimitive::HexString),
            Some(byte) if starts_keyword_byte(byte) => self.parse_keyword(),
            Some(byte) if starts_number_byte(byte) => self.parse_number_or_reference(),
            Some(_) => Err(PdfDiffError::InvalidInput(format!(
                "unexpected byte 0x{:02x} while parsing PDF primitive",
                self.bytes[self.index]
            ))),
            None => Err(PdfDiffError::InvalidInput(
                "expected PDF primitive, found end of input".into(),
            )),
        }
    }

    fn parse_unsigned_object_number(&mut self, label: &str) -> Result<u32, PdfDiffError> {
        let word = self.read_word();
        let NumericPrimitive::Integer(value) = parse_number_token(&word)? else {
            return Err(PdfDiffError::InvalidInput(format!(
                "{label} must be an integer"
            )));
        };
        u32::try_from(value).map_err(|_| {
            PdfDiffError::InvalidInput(format!("{label} is outside the supported range"))
        })
    }

    fn parse_unsigned_generation(&mut self) -> Result<u16, PdfDiffError> {
        let word = self.read_word();
        let NumericPrimitive::Integer(value) = parse_number_token(&word)? else {
            return Err(PdfDiffError::InvalidInput(
                "generation number must be an integer".into(),
            ));
        };
        u16::try_from(value).map_err(|_| {
            PdfDiffError::InvalidInput("generation number is outside the supported range".into())
        })
    }

    fn expect_keyword(&mut self, expected: &[u8]) -> Result<(), PdfDiffError> {
        let word = self.read_word();
        if word == expected {
            return Ok(());
        }
        Err(PdfDiffError::InvalidInput(format!(
            "expected keyword {}",
            String::from_utf8_lossy(expected)
        )))
    }

    fn parse_keyword(&mut self) -> Result<PdfPrimitive, PdfDiffError> {
        let word = self.read_word();
        match word.as_slice() {
            b"true" => Ok(PdfPrimitive::Boolean(true)),
            b"false" => Ok(PdfPrimitive::Boolean(false)),
            b"null" => Ok(PdfPrimitive::Null),
            _ => Err(PdfDiffError::InvalidInput(format!(
                "unsupported primitive keyword {}",
                String::from_utf8_lossy(&word)
            ))),
        }
    }

    fn parse_number_or_reference(&mut self) -> Result<PdfPrimitive, PdfDiffError> {
        let first = self.read_word();
        let first_number = parse_number_token(&first)?;
        let after_first = self.index;

        if let NumericPrimitive::Integer(number) = first_number {
            let restore = self.index;
            self.skip_whitespace_and_comments();
            if let Some(second) = self.try_read_integer_word() {
                self.skip_whitespace_and_comments();
                if self.peek() == Some(b'R') && self.peek_after_word() {
                    self.index += 1;
                    let object_number = u32::try_from(number).map_err(|_| {
                        PdfDiffError::InvalidInput("reference object number is out of range".into())
                    })?;
                    let generation = u16::try_from(second).map_err(|_| {
                        PdfDiffError::InvalidInput(
                            "reference generation number is out of range".into(),
                        )
                    })?;
                    return Ok(PdfPrimitive::Reference(ObjectId {
                        number: object_number,
                        generation,
                    }));
                }
            }
            self.index = restore;
        }

        self.index = after_first;
        match first_number {
            NumericPrimitive::Integer(value) => Ok(PdfPrimitive::Integer(value)),
            NumericPrimitive::Real(value) => Ok(PdfPrimitive::Real(value)),
        }
    }

    fn parse_name(&mut self) -> Result<String, PdfDiffError> {
        self.expect_byte(b'/')?;
        let mut output = Vec::new();
        while let Some(byte) = self.peek() {
            if is_delimiter_or_whitespace(byte) {
                break;
            }
            if byte == b'#' {
                self.index += 1;
                let high = self.next_hex_digit()?;
                let low = self.next_hex_digit()?;
                output.push((high << 4) | low);
            } else {
                output.push(byte);
                self.index += 1;
            }
        }
        if output.is_empty() {
            return Err(PdfDiffError::InvalidInput("PDF name is empty".into()));
        }
        Ok(String::from_utf8_lossy(&output).into_owned())
    }

    fn parse_literal_string(&mut self) -> Result<Vec<u8>, PdfDiffError> {
        self.expect_byte(b'(')?;
        let mut output = Vec::new();
        let mut depth = 1usize;
        while let Some(byte) = self.peek() {
            self.index += 1;
            match byte {
                b'\\' => {
                    let Some(escaped) = self.peek() else {
                        return Err(PdfDiffError::InvalidInput(
                            "unterminated literal string escape".into(),
                        ));
                    };
                    self.index += 1;
                    match escaped {
                        b'n' => output.push(b'\n'),
                        b'r' => output.push(b'\r'),
                        b't' => output.push(b'\t'),
                        b'b' => output.push(0x08),
                        b'f' => output.push(0x0c),
                        b'\n' => {}
                        b'\r' => {
                            if self.peek() == Some(b'\n') {
                                self.index += 1;
                            }
                        }
                        b'0'..=b'7' => output.push(self.finish_octal_escape(escaped)),
                        other => output.push(other),
                    }
                }
                b'(' => {
                    depth += 1;
                    output.push(byte);
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(output);
                    }
                    output.push(byte);
                }
                other => output.push(other),
            }
        }
        Err(PdfDiffError::InvalidInput(
            "unterminated literal string".into(),
        ))
    }

    fn parse_hex_string(&mut self) -> Result<Vec<u8>, PdfDiffError> {
        self.expect_byte(b'<')?;
        let mut nibbles = Vec::new();
        while let Some(byte) = self.peek() {
            self.index += 1;
            if byte == b'>' {
                if nibbles.len() % 2 == 1 {
                    nibbles.push(0);
                }
                return Ok(nibbles
                    .chunks(2)
                    .map(|pair| (pair[0] << 4) | pair[1])
                    .collect());
            }
            if byte.is_ascii_whitespace() {
                continue;
            }
            nibbles.push(hex_value(byte).ok_or_else(|| {
                PdfDiffError::InvalidInput(format!("invalid hex string digit 0x{byte:02x}"))
            })?);
        }
        Err(PdfDiffError::InvalidInput("unterminated hex string".into()))
    }

    fn parse_array(&mut self, depth: usize) -> Result<PdfPrimitive, PdfDiffError> {
        self.expect_byte(b'[')?;
        let mut values = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            match self.peek() {
                Some(b']') => {
                    self.index += 1;
                    return Ok(PdfPrimitive::Array(values));
                }
                Some(_) => values.push(self.parse_value(depth + 1)?),
                None => {
                    return Err(PdfDiffError::InvalidInput("unterminated array".into()));
                }
            }
        }
    }

    fn parse_dictionary(&mut self, depth: usize) -> Result<PdfPrimitive, PdfDiffError> {
        self.expect_byte(b'<')?;
        self.expect_byte(b'<')?;
        let mut entries = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            match (self.peek(), self.peek_next()) {
                (Some(b'>'), Some(b'>')) => {
                    self.index += 2;
                    return Ok(PdfPrimitive::Dictionary(entries));
                }
                (Some(b'/'), _) => {
                    let key = self.parse_name()?;
                    let value = self.parse_value(depth + 1)?;
                    entries.push((key, value));
                }
                (Some(_), _) => {
                    return Err(PdfDiffError::InvalidInput(
                        "dictionary key must be a PDF name".into(),
                    ));
                }
                (None, _) => {
                    return Err(PdfDiffError::InvalidInput("unterminated dictionary".into()));
                }
            }
        }
    }

    fn skip_whitespace_and_comments(&mut self) -> usize {
        loop {
            while self.peek().is_some_and(|byte| byte.is_ascii_whitespace()) {
                self.index += 1;
            }
            if self.peek() == Some(b'%') {
                while let Some(byte) = self.peek() {
                    self.index += 1;
                    if matches!(byte, b'\r' | b'\n') {
                        break;
                    }
                }
                continue;
            }
            return self.index;
        }
    }

    fn read_word(&mut self) -> Vec<u8> {
        let start = self.index;
        while let Some(byte) = self.peek() {
            if is_delimiter_or_whitespace(byte) {
                break;
            }
            self.index += 1;
        }
        self.bytes[start..self.index].to_vec()
    }

    fn try_read_integer_word(&mut self) -> Option<i64> {
        let start = self.index;
        let word = self.read_word();
        if word.is_empty() {
            self.index = start;
            return None;
        }
        match parse_number_token(&word).ok()? {
            NumericPrimitive::Integer(value) => Some(value),
            NumericPrimitive::Real(_) => {
                self.index = start;
                None
            }
        }
    }

    fn peek_after_word(&self) -> bool {
        self.bytes
            .get(self.index + 1)
            .is_none_or(|byte| is_delimiter_or_whitespace(*byte))
    }

    fn finish_octal_escape(&mut self, first: u8) -> u8 {
        let mut value = first - b'0';
        for _ in 0..2 {
            let Some(byte @ b'0'..=b'7') = self.peek() else {
                break;
            };
            self.index += 1;
            value = value.saturating_mul(8).saturating_add(byte - b'0');
        }
        value
    }

    fn expect_byte(&mut self, expected: u8) -> Result<(), PdfDiffError> {
        if self.peek() == Some(expected) {
            self.index += 1;
            return Ok(());
        }
        Err(PdfDiffError::InvalidInput(format!(
            "expected byte 0x{expected:02x}"
        )))
    }

    fn next_hex_digit(&mut self) -> Result<u8, PdfDiffError> {
        let Some(byte) = self.peek() else {
            return Err(PdfDiffError::InvalidInput(
                "name hex escape is truncated".into(),
            ));
        };
        self.index += 1;
        hex_value(byte)
            .ok_or_else(|| PdfDiffError::InvalidInput("name hex escape is invalid".into()))
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.index).copied()
    }

    fn peek_next(&self) -> Option<u8> {
        self.bytes.get(self.index + 1).copied()
    }

    fn is_eof(&self) -> bool {
        self.index >= self.bytes.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum NumericPrimitive {
    Integer(i64),
    Real(f64),
}

fn parse_number_token(bytes: &[u8]) -> Result<NumericPrimitive, PdfDiffError> {
    let token = std::str::from_utf8(bytes)
        .map_err(|_| PdfDiffError::InvalidInput("number token is not UTF-8".into()))?;
    if token.contains('.') {
        return token
            .parse::<f64>()
            .map(NumericPrimitive::Real)
            .map_err(|_| PdfDiffError::InvalidInput(format!("invalid real number {token}")));
    }
    token
        .parse::<i64>()
        .map(NumericPrimitive::Integer)
        .map_err(|_| PdfDiffError::InvalidInput(format!("invalid integer {token}")))
}

fn starts_keyword_byte(byte: u8) -> bool {
    matches!(byte, b't' | b'f' | b'n')
}

fn starts_number_byte(byte: u8) -> bool {
    byte.is_ascii_digit() || matches!(byte, b'+' | b'-' | b'.')
}

fn is_delimiter_or_whitespace(byte: u8) -> bool {
    byte.is_ascii_whitespace()
        || matches!(byte, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'/' | b'%')
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn find_keyword(bytes: &[u8], start: usize, keyword: &[u8]) -> Option<usize> {
    bytes
        .get(start..)?
        .windows(keyword.len())
        .position(|window| window == keyword)
        .map(|index| start + index)
}

fn trim_ascii(bytes: &[u8]) -> (&[u8], usize) {
    let mut start = 0;
    let mut end = bytes.len();
    while start < end && bytes[start].is_ascii_whitespace() {
        start += 1;
    }
    while end > start && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    (&bytes[start..end], start)
}

fn trim_trailing_ascii(bytes: &[u8]) -> &[u8] {
    let mut end = bytes.len();
    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    &bytes[..end]
}

fn locate_startxref(bytes: &[u8]) -> Result<usize, PdfDiffError> {
    let marker = find_last_keyword(bytes, b"startxref")
        .ok_or_else(|| PdfDiffError::InvalidInput("PDF is missing startxref marker".into()))?;
    let mut parser = XrefParser::new(bytes, marker + b"startxref".len());
    parser.skip_ascii_whitespace();
    parser.read_usize("startxref offset")
}

fn find_last_keyword(bytes: &[u8], keyword: &[u8]) -> Option<usize> {
    bytes
        .windows(keyword.len())
        .rposition(|window| window == keyword)
}

struct XrefParser<'a> {
    bytes: &'a [u8],
    index: usize,
}

impl<'a> XrefParser<'a> {
    fn new(bytes: &'a [u8], index: usize) -> Self {
        Self { bytes, index }
    }

    fn expect_keyword(&mut self, expected: &[u8]) -> Result<(), PdfDiffError> {
        if self.starts_with(expected) {
            self.index += expected.len();
            return Ok(());
        }
        Err(PdfDiffError::InvalidInput(format!(
            "expected keyword {}",
            String::from_utf8_lossy(expected)
        )))
    }

    fn starts_with(&self, expected: &[u8]) -> bool {
        self.bytes
            .get(self.index..)
            .is_some_and(|remaining| remaining.starts_with(expected))
    }

    fn skip_ascii_whitespace(&mut self) -> usize {
        while self
            .bytes
            .get(self.index)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            self.index += 1;
        }
        self.index
    }

    fn skip_inline_whitespace(&mut self) {
        while matches!(self.bytes.get(self.index), Some(b' ' | b'\t')) {
            self.index += 1;
        }
    }

    fn skip_line_break(&mut self) {
        self.skip_inline_whitespace();
        match self.bytes.get(self.index) {
            Some(b'\r') if self.bytes.get(self.index + 1) == Some(&b'\n') => {
                self.index += 2;
            }
            Some(b'\r' | b'\n') => {
                self.index += 1;
            }
            _ => {}
        }
    }

    fn read_usize(&mut self, label: &str) -> Result<usize, PdfDiffError> {
        let start = self.index;
        while self
            .bytes
            .get(self.index)
            .is_some_and(|byte| byte.is_ascii_digit())
        {
            self.index += 1;
        }
        if start == self.index {
            return Err(PdfDiffError::InvalidInput(format!("expected {label}")));
        }
        std::str::from_utf8(&self.bytes[start..self.index])
            .map_err(|_| PdfDiffError::InvalidInput(format!("{label} is not UTF-8")))?
            .parse()
            .map_err(|_| PdfDiffError::InvalidInput(format!("{label} is out of range")))
    }

    fn read_fixed_width_usize(&mut self, width: usize, label: &str) -> Result<usize, PdfDiffError> {
        let value = self.fixed_width_bytes(width, label)?;
        std::str::from_utf8(value)
            .map_err(|_| PdfDiffError::InvalidInput(format!("{label} is not UTF-8")))?
            .parse()
            .map_err(|_| PdfDiffError::InvalidInput(format!("{label} is out of range")))
    }

    fn read_fixed_width_u16(&mut self, width: usize, label: &str) -> Result<u16, PdfDiffError> {
        let value = self.fixed_width_bytes(width, label)?;
        std::str::from_utf8(value)
            .map_err(|_| PdfDiffError::InvalidInput(format!("{label} is not UTF-8")))?
            .parse()
            .map_err(|_| PdfDiffError::InvalidInput(format!("{label} is out of range")))
    }

    fn fixed_width_bytes(&mut self, width: usize, label: &str) -> Result<&'a [u8], PdfDiffError> {
        let Some(value) = self.bytes.get(self.index..self.index + width) else {
            return Err(PdfDiffError::InvalidInput(format!("truncated {label}")));
        };
        if !value.iter().all(u8::is_ascii_digit) {
            return Err(PdfDiffError::InvalidInput(format!("{label} is invalid")));
        }
        self.index += width;
        Ok(value)
    }

    fn next_byte(&mut self) -> Option<u8> {
        let byte = self.bytes.get(self.index).copied()?;
        self.index += 1;
        Some(byte)
    }
}

fn parse_header(bytes: &[u8]) -> Result<PdfDocument, PdfDiffError> {
    let header = bytes
        .get(..8)
        .ok_or_else(|| PdfDiffError::InvalidInput("file is too short for a PDF header".into()))?;

    if !header.starts_with(b"%PDF-") {
        return Err(PdfDiffError::UnsupportedPdf("missing %PDF- header".into()));
    }

    let major = header[5];
    let minor = header[7];
    if !major.is_ascii_digit() || header[6] != b'.' || !minor.is_ascii_digit() {
        return Err(PdfDiffError::InvalidInput(
            "malformed PDF version header".into(),
        ));
    }

    Ok(PdfDocument {
        version: PdfVersion {
            major: major - b'0',
            minor: minor - b'0',
        },
        objects: Vec::new(),
        pages: Vec::new(),
        diagnostics: Vec::new(),
        incremental_update: None,
    })
}

fn parse_indirect_objects(
    bytes: &[u8],
    config: &ParseConfig,
) -> Result<Vec<PdfObject>, PdfDiffError> {
    let mut objects = Vec::new();
    let mut cursor = 0;

    while let Some(marker_start) = find_keyword(bytes, cursor, b" obj") {
        let line_start = bytes[..marker_start]
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |index| index + 1);
        let (header_bytes, _) = trim_ascii(&bytes[line_start..marker_start]);
        let Ok(header) = std::str::from_utf8(header_bytes) else {
            cursor = marker_start + b" obj".len();
            continue;
        };
        let Some((number, generation)) = parse_object_header(header) else {
            cursor = marker_start + b" obj".len();
            continue;
        };
        let body_start = marker_start + b" obj".len();
        let Some(end_marker) = find_keyword(bytes, body_start, b"endobj") else {
            return Err(PdfDiffError::InvalidInput(format!(
                "object {number} {generation} is missing endobj"
            )));
        };
        let object_end = end_marker + b"endobj".len();
        let (body_bytes, _) = trim_ascii(&bytes[body_start..end_marker]);
        let body = String::from_utf8_lossy(body_bytes).into_owned();
        let stream = parse_stream(bytes, body_start, end_marker, config)?;
        objects.push(PdfObject {
            id: ObjectId { number, generation },
            body,
            stream,
            byte_range: ByteRange::new(line_start, object_end),
        });
        cursor = object_end;
    }

    objects.sort_by_key(|object| (object.id.number, object.id.generation));
    Ok(objects)
}

fn pdf_objects_from_object_store(objects: Vec<ParsedIndirectObject>) -> Vec<PdfObject> {
    objects
        .into_iter()
        .map(|object| PdfObject {
            id: object.id,
            body: primitive_to_pdf_syntax(&object.value),
            stream: object.stream,
            byte_range: object.byte_range,
        })
        .collect()
}

fn primitive_to_pdf_syntax(value: &PdfPrimitive) -> String {
    match value {
        PdfPrimitive::Null => "null".to_owned(),
        PdfPrimitive::Boolean(value) => value.to_string(),
        PdfPrimitive::Integer(value) => value.to_string(),
        PdfPrimitive::Real(value) => value.to_string(),
        PdfPrimitive::Name(name) => format!("/{name}"),
        PdfPrimitive::LiteralString(bytes) => format!("({})", String::from_utf8_lossy(bytes)),
        PdfPrimitive::HexString(bytes) => {
            let mut out = String::from("<");
            for byte in bytes {
                out.push_str(&format!("{byte:02x}"));
            }
            out.push('>');
            out
        }
        PdfPrimitive::Array(items) => format!(
            "[{}]",
            items
                .iter()
                .map(primitive_to_pdf_syntax)
                .collect::<Vec<_>>()
                .join(" ")
        ),
        PdfPrimitive::Dictionary(entries) => format!(
            "<< {} >>",
            entries
                .iter()
                .map(|(key, value)| format!("/{key} {}", primitive_to_pdf_syntax(value)))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        PdfPrimitive::Reference(id) => format!("{} {} R", id.number, id.generation),
    }
}

fn parse_object_header(header: &str) -> Option<(u32, u16)> {
    let mut parts = header.split_ascii_whitespace();
    let number = parts.next()?.parse().ok()?;
    let generation = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((number, generation))
}

fn parse_stream(
    bytes: &[u8],
    body_start: usize,
    body_end: usize,
    config: &ParseConfig,
) -> Result<Option<PdfStream>, PdfDiffError> {
    let Some(stream_marker) = find_keyword(bytes, body_start, b"stream") else {
        return Ok(None);
    };
    if stream_marker >= body_end {
        return Ok(None);
    }

    let stream_data_start = match bytes.get(stream_marker + b"stream".len()) {
        Some(b'\r') if bytes.get(stream_marker + b"stream".len() + 1) == Some(&b'\n') => {
            stream_marker + b"stream".len() + 2
        }
        Some(b'\n') => stream_marker + b"stream".len() + 1,
        _ => stream_marker + b"stream".len(),
    };
    let Some(endstream_marker) = find_keyword(bytes, stream_data_start, b"endstream") else {
        return Err(PdfDiffError::InvalidInput(
            "stream is missing endstream marker".into(),
        ));
    };
    if endstream_marker > body_end {
        return Err(PdfDiffError::InvalidInput(
            "stream is missing endstream marker".into(),
        ));
    }

    let stream_header = String::from_utf8_lossy(&bytes[body_start..stream_marker]);
    let declared_length = stream_length_from_body(&stream_header);
    let mut stream_data_end = declared_length
        .and_then(|length| stream_data_start.checked_add(length))
        .filter(|declared_end| {
            *declared_end <= endstream_marker
                && bytes[*declared_end..endstream_marker]
                    .iter()
                    .all(u8::is_ascii_whitespace)
        })
        .unwrap_or(endstream_marker);
    while stream_data_end > stream_data_start
        && matches!(bytes.get(stream_data_end - 1), Some(b'\n' | b'\r'))
    {
        stream_data_end -= 1;
    }
    let stream_len = stream_data_end.saturating_sub(stream_data_start);
    if stream_len > config.limits.max_stream_bytes {
        return Err(resource_limit_error(
            "RESOURCE_LIMIT_STREAM_BYTES",
            format!(
                "stream has {stream_len} bytes, limit is {}",
                config.limits.max_stream_bytes
            ),
        ));
    }
    decode_stream(
        PdfStream {
            bytes: bytes[stream_data_start..stream_data_end].to_vec(),
            raw_bytes: bytes[stream_data_start..stream_data_end].to_vec(),
            byte_range: ByteRange::new(stream_data_start, stream_data_end),
            declared_length,
            filters: stream_filters_from_body(&stream_header),
            decode_params: decode_params_from_body(&stream_header),
            decoded: true,
        },
        *config,
    )
    .map(Some)
}

fn stream_filters_from_body(body: &str) -> Vec<String> {
    parse_primitive(body.as_bytes(), ParseConfig::default())
        .map(|parsed| stream_filters_from_primitive(&parsed.value))
        .unwrap_or_default()
}

fn decode_params_from_body(body: &str) -> Vec<Option<String>> {
    parse_primitive(body.as_bytes(), ParseConfig::default())
        .map(|parsed| decode_params_from_primitive(&parsed.value))
        .unwrap_or_default()
}

fn stream_length_from_body(body: &str) -> Option<usize> {
    let bytes = body.as_bytes();
    let mut index = body.find("/Length")? + "/Length".len();
    skip_ascii_whitespace_bytes(bytes, &mut index);
    let start = index;
    while matches!(bytes.get(index), Some(byte) if byte.is_ascii_digit()) {
        index += 1;
    }
    if start == index {
        return None;
    }
    std::str::from_utf8(&bytes[start..index]).ok()?.parse().ok()
}

fn skip_ascii_whitespace_bytes(bytes: &[u8], index: &mut usize) {
    while matches!(bytes.get(*index), Some(byte) if byte.is_ascii_whitespace()) {
        *index += 1;
    }
}

fn resolve_pages(
    objects: &[PdfObject],
    limits: ResourceLimits,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<PdfPage>, PdfDiffError> {
    let values = parsed_object_values(objects);
    if let Some(pages_root_id) = catalog_pages_root_id(&values) {
        let mut pages = Vec::new();
        let mut seen = BTreeSet::new();
        append_pages_from_tree(
            pages_root_id,
            &values,
            InheritedPageAttributes::default(),
            limits,
            diagnostics,
            &mut seen,
            &mut pages,
        )?;
        if !pages.is_empty() {
            return Ok(pages);
        }
        diagnostics.push(Diagnostic::warning(
            "PAGE_TREE_EMPTY",
            "catalog /Pages tree resolved without page entries; falling back to scanned page objects",
        ));
    }

    scanned_pages_from_objects(objects, &values, limits, diagnostics)
}

#[derive(Debug, Clone, Default)]
struct InheritedPageAttributes {
    media_box: Option<spdfdiff_types::Rect>,
    crop_box: Option<spdfdiff_types::Rect>,
    rotation: Option<i32>,
    resources_object_id: Option<ObjectId>,
    has_resources: bool,
}

fn parsed_object_values(objects: &[PdfObject]) -> BTreeMap<ObjectId, PdfPrimitive> {
    objects
        .iter()
        .filter_map(|object| {
            parse_primitive(object.body.as_bytes(), ParseConfig::default())
                .ok()
                .map(|parsed| (object.id, parsed.value))
        })
        .collect()
}

fn catalog_pages_root_id(values: &BTreeMap<ObjectId, PdfPrimitive>) -> Option<ObjectId> {
    values.values().find_map(|value| {
        if !matches!(
            dictionary_value(value, "Type"),
            Some(PdfPrimitive::Name(name)) if name == "Catalog"
        ) {
            return None;
        }
        match dictionary_value(value, "Pages") {
            Some(PdfPrimitive::Reference(id)) => Some(*id),
            _ => None,
        }
    })
}

fn append_pages_from_tree(
    object_id: ObjectId,
    values: &BTreeMap<ObjectId, PdfPrimitive>,
    inherited: InheritedPageAttributes,
    limits: ResourceLimits,
    diagnostics: &mut Vec<Diagnostic>,
    seen: &mut BTreeSet<ObjectId>,
    pages: &mut Vec<PdfPage>,
) -> Result<(), PdfDiffError> {
    if pages.len() > limits.max_pages {
        return Err(page_count_limit_error(pages.len(), limits));
    }
    if !seen.insert(object_id) {
        diagnostics.push(
            Diagnostic::warning(
                "PAGE_TREE_CYCLE",
                format!("page tree cycle detected at object {}", object_id.number),
            )
            .with_object(object_id),
        );
        return Ok(());
    }

    let Some(value) = values.get(&object_id) else {
        diagnostics.push(
            Diagnostic::warning(
                "PAGE_TREE_OBJECT_MISSING",
                format!("page tree references missing object {}", object_id.number),
            )
            .with_object(object_id),
        );
        seen.remove(&object_id);
        return Ok(());
    };
    let inherited = inherited.merge(value);
    match dictionary_value(value, "Type") {
        Some(PdfPrimitive::Name(name)) if name == "Pages" => {
            let Some(PdfPrimitive::Array(kids)) = dictionary_value(value, "Kids") else {
                diagnostics.push(
                    Diagnostic::warning("PAGE_TREE_KIDS_MISSING", "pages node has no /Kids array")
                        .with_object(object_id),
                );
                seen.remove(&object_id);
                return Ok(());
            };
            for kid in kids {
                match kid {
                    PdfPrimitive::Reference(kid_id) => append_pages_from_tree(
                        *kid_id,
                        values,
                        inherited.clone(),
                        limits,
                        diagnostics,
                        seen,
                        pages,
                    )?,
                    _ => diagnostics.push(
                        Diagnostic::warning(
                            "PAGE_TREE_KID_INVALID",
                            "pages node contains a non-reference /Kids entry",
                        )
                        .with_object(object_id),
                    ),
                }
            }
        }
        Some(PdfPrimitive::Name(name)) if name == "Page" => {
            if let Some(page) = page_from_value(object_id, value, &inherited, diagnostics) {
                pages.push(page.with_page_index(pages.len()));
            }
        }
        _ => diagnostics.push(
            Diagnostic::warning(
                "PAGE_TREE_NODE_INVALID",
                "page tree reference does not point to a /Page or /Pages node",
            )
            .with_object(object_id),
        ),
    }
    seen.remove(&object_id);
    if pages.len() > limits.max_pages {
        return Err(page_count_limit_error(pages.len(), limits));
    }
    Ok(())
}

impl InheritedPageAttributes {
    fn merge(&self, value: &PdfPrimitive) -> Self {
        Self {
            media_box: rect_from_array(dictionary_value(value, "MediaBox")).or(self.media_box),
            crop_box: rect_from_array(dictionary_value(value, "CropBox")).or(self.crop_box),
            rotation: integer_from_value(dictionary_value(value, "Rotate")).or(self.rotation),
            resources_object_id: reference_from_value(dictionary_value(value, "Resources"))
                .or(self.resources_object_id),
            has_resources: dictionary_value(value, "Resources").is_some() || self.has_resources,
        }
    }
}

impl PdfPage {
    fn with_page_index(mut self, page_index: usize) -> Self {
        self.page_index = page_index;
        self
    }
}

fn page_from_value(
    object_id: ObjectId,
    value: &PdfPrimitive,
    inherited: &InheritedPageAttributes,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<PdfPage> {
    let content_object_ids = content_references_from_value(dictionary_value(value, "Contents"));
    let Some(content_object_id) = content_object_ids.first().copied() else {
        diagnostics.push(
            Diagnostic::warning(
                "MISSING_CONTENT_STREAM",
                "page does not reference /Contents",
            )
            .with_object(object_id),
        );
        return None;
    };
    Some(PdfPage {
        page_index: 0,
        object_id,
        content_object_id,
        content_object_ids,
        media_box: inherited.media_box,
        crop_box: inherited.crop_box.or(inherited.media_box),
        rotation: inherited.rotation.unwrap_or(0),
        resources_object_id: inherited.resources_object_id,
        has_resources: inherited.has_resources,
    })
}

fn scanned_pages_from_objects(
    objects: &[PdfObject],
    values: &BTreeMap<ObjectId, PdfPrimitive>,
    limits: ResourceLimits,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<PdfPage>, PdfDiffError> {
    let mut pages = Vec::new();
    for object in objects {
        let Some(value) = values.get(&object.id) else {
            continue;
        };
        if !matches!(
            dictionary_value(value, "Type"),
            Some(PdfPrimitive::Name(name)) if name == "Page"
        ) {
            continue;
        };
        if let Some(page) = page_from_value(
            object.id,
            value,
            &InheritedPageAttributes::default().merge(value),
            diagnostics,
        ) {
            pages.push(page.with_page_index(pages.len()));
        }
    }
    if pages.len() > limits.max_pages {
        return Err(page_count_limit_error(pages.len(), limits));
    }
    Ok(pages)
}

fn page_count_limit_error(page_count: usize, limits: ResourceLimits) -> PdfDiffError {
    resource_limit_error(
        "RESOURCE_LIMIT_PAGE_COUNT",
        format!("file has {page_count} pages, limit is {}", limits.max_pages),
    )
}

fn content_references_from_value(value: Option<&PdfPrimitive>) -> Vec<ObjectId> {
    match value {
        Some(PdfPrimitive::Reference(id)) => vec![*id],
        Some(PdfPrimitive::Array(items)) => items
            .iter()
            .filter_map(|item| match item {
                PdfPrimitive::Reference(id) => Some(*id),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn rect_from_array(value: Option<&PdfPrimitive>) -> Option<spdfdiff_types::Rect> {
    let Some(PdfPrimitive::Array(items)) = value else {
        return None;
    };
    let [x0, y0, x1, y1] = items.as_slice() else {
        return None;
    };
    Some(spdfdiff_types::Rect {
        x0: number_from_value(x0)?,
        y0: number_from_value(y0)?,
        x1: number_from_value(x1)?,
        y1: number_from_value(y1)?,
    })
}

fn number_from_value(value: &PdfPrimitive) -> Option<f32> {
    match value {
        PdfPrimitive::Integer(value) => Some(*value as f32),
        PdfPrimitive::Real(value) => Some(*value as f32),
        _ => None,
    }
}

fn integer_from_value(value: Option<&PdfPrimitive>) -> Option<i32> {
    let Some(PdfPrimitive::Integer(value)) = value else {
        return None;
    };
    i32::try_from(*value).ok()
}

fn reference_from_value(value: Option<&PdfPrimitive>) -> Option<ObjectId> {
    match value {
        Some(PdfPrimitive::Reference(id)) => Some(*id),
        _ => None,
    }
}

fn scan_unsupported_features(objects: &[PdfObject]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for object in objects {
        if let Some(stream) = &object.stream {
            if let Some(declared_length) = stream.declared_length {
                let actual_length = stream.raw_bytes.len();
                let missing_bytes = declared_length.saturating_sub(actual_length);
                if actual_length > declared_length || missing_bytes > 2 {
                    diagnostics.push(
                        Diagnostic::warning(
                            "STREAM_LENGTH_MISMATCH",
                            format!(
                                "stream declared {declared_length} bytes but contains {} bytes",
                                actual_length
                            ),
                        )
                        .with_object(object.id),
                    );
                }
            }
            let unsupported_filters = stream
                .filters
                .iter()
                .filter(|filter| !is_supported_stream_filter(filter))
                .collect::<Vec<_>>();
            if !unsupported_filters.is_empty() {
                diagnostics.push(
                    Diagnostic::warning(
                        "UNSUPPORTED_STREAM_FILTER",
                        format!(
                            "unsupported stream filter chain {}; raw bytes were preserved",
                            unsupported_filters
                                .iter()
                                .map(|filter| format!("/{filter}"))
                                .collect::<Vec<_>>()
                                .join(" ")
                        ),
                    )
                    .with_object(object.id),
                );
            } else if !stream.decoded && !stream.filters.is_empty() {
                diagnostics.push(
                    Diagnostic::warning(
                        "STREAM_DECODE_FAILED",
                        format!(
                            "stream filter chain {} could not be decoded; raw bytes were preserved",
                            stream
                                .filters
                                .iter()
                                .map(|filter| format!("/{filter}"))
                                .collect::<Vec<_>>()
                                .join(" ")
                        ),
                    )
                    .with_object(object.id),
                );
            }
            if stream.decode_params.len() > stream.filters.len() {
                diagnostics.push(
                    Diagnostic::warning(
                        "STREAM_DECODE_PARAMS_MISMATCH",
                        format!(
                            "stream has {} DecodeParms entries for {} filters",
                            stream.decode_params.len(),
                            stream.filters.len()
                        ),
                    )
                    .with_object(object.id),
                );
            }
        }
    }
    diagnostics
}

fn is_supported_stream_filter(filter: &str) -> bool {
    matches!(
        filter,
        "FlateDecode" | "Fl" | "ASCIIHexDecode" | "AHx" | "RunLengthDecode" | "RL"
    )
}

trait DiagnosticExt {
    fn with_object(self, object_id: ObjectId) -> Self;
}

impl DiagnosticExt for Diagnostic {
    fn with_object(mut self, object_id: ObjectId) -> Self {
        self.object = Some(object_id);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::ZlibEncoder};
    use spdfdiff_types::DiagnosticSeverity;
    use std::io::Write;

    #[test]
    fn parses_pdf_header() {
        let document = PdfDocument::parse(b"%PDF-1.7\n").expect("header should parse");
        assert_eq!(document.version, PdfVersion { major: 1, minor: 7 });
    }

    #[test]
    fn rejects_non_pdf_header() {
        let error = PdfDocument::parse(b"not a pdf").expect_err("header should be rejected");
        assert!(matches!(error, PdfDiffError::UnsupportedPdf(_)));
    }

    #[test]
    fn enforces_file_size_limit() {
        let config = ParseConfig {
            limits: ResourceLimits {
                max_file_bytes: 4,
                ..ResourceLimits::default()
            },
        };
        let error =
            PdfDocument::parse_with_config(b"%PDF-1.7\n", config).expect_err("limit should fail");
        assert!(matches!(error, PdfDiffError::ResourceLimitExceeded(_)));
    }

    #[test]
    fn object_count_limit_error_has_stable_code() {
        let fixture = classic_xref_pdf();
        let config = ParseConfig {
            limits: ResourceLimits {
                max_objects: 1,
                ..ResourceLimits::default()
            },
        };

        let error = parse_xref_table(&fixture.bytes, config).expect_err("limit should fail");

        assert!(matches!(
            error,
            PdfDiffError::ResourceLimitExceeded(message)
                if message.contains("RESOURCE_LIMIT_OBJECT_COUNT")
        ));
    }

    #[test]
    fn parses_primitive_scalars() {
        assert_eq!(
            parse_primitive(b" true ", ParseConfig::default())
                .expect("boolean should parse")
                .value,
            PdfPrimitive::Boolean(true)
        );
        assert_eq!(
            parse_primitive(b"null", ParseConfig::default())
                .expect("null should parse")
                .value,
            PdfPrimitive::Null
        );
        assert_eq!(
            parse_primitive(b"-42", ParseConfig::default())
                .expect("integer should parse")
                .value,
            PdfPrimitive::Integer(-42)
        );
        assert!(matches!(
            parse_primitive(b"3.25", ParseConfig::default())
                .expect("real should parse")
                .value,
            PdfPrimitive::Real(value) if value == 3.25
        ));
    }

    #[test]
    fn skips_comments_and_preserves_primitive_byte_range() {
        let parsed = parse_primitive(b"  % comment\n/Name", ParseConfig::default())
            .expect("name should parse");

        assert_eq!(parsed.value, PdfPrimitive::Name("Name".into()));
        assert_eq!(parsed.byte_range, ByteRange::new(12, 17));
    }

    #[test]
    fn parses_names_with_hex_escapes() {
        let parsed =
            parse_primitive(b"/A#20Name", ParseConfig::default()).expect("name should parse");

        assert_eq!(parsed.value, PdfPrimitive::Name("A Name".into()));
    }

    #[test]
    fn parses_literal_and_hex_strings() {
        assert_eq!(
            parse_primitive(br"(Hello \(PDF\)\n)", ParseConfig::default())
                .expect("literal string should parse")
                .value,
            PdfPrimitive::LiteralString(b"Hello (PDF)\n".to_vec())
        );
        assert_eq!(
            parse_primitive(b"<48656c6c6f2>", ParseConfig::default())
                .expect("hex string should parse")
                .value,
            PdfPrimitive::HexString(vec![b'H', b'e', b'l', b'l', b'o', 0x20])
        );
    }

    #[test]
    fn parses_arrays_dictionaries_and_references() {
        let parsed = parse_primitive(
            b"<< /Type /Page /Count 1 /Kids [3 0 R] >>",
            ParseConfig::default(),
        )
        .expect("dictionary should parse");

        let PdfPrimitive::Dictionary(entries) = parsed.value else {
            panic!("expected dictionary");
        };
        assert_eq!(
            entries[0],
            ("Type".into(), PdfPrimitive::Name("Page".into()))
        );
        assert_eq!(entries[1], ("Count".into(), PdfPrimitive::Integer(1)));
        assert_eq!(
            entries[2],
            (
                "Kids".into(),
                PdfPrimitive::Array(vec![PdfPrimitive::Reference(ObjectId {
                    number: 3,
                    generation: 0
                })])
            )
        );
    }

    #[test]
    fn rejects_malformed_primitives_without_panic() {
        assert!(matches!(
            parse_primitive(b"(unterminated", ParseConfig::default()),
            Err(PdfDiffError::InvalidInput(_))
        ));
        assert!(matches!(
            parse_primitive(b"<< /Name >>", ParseConfig::default()),
            Err(PdfDiffError::InvalidInput(_))
        ));
        assert!(matches!(
            parse_primitive(b"<zz>", ParseConfig::default()),
            Err(PdfDiffError::InvalidInput(_))
        ));
    }

    #[test]
    fn enforces_primitive_nesting_limit() {
        let config = ParseConfig {
            limits: ResourceLimits {
                max_indirect_depth: 1,
                ..ResourceLimits::default()
            },
        };

        assert!(matches!(
            parse_primitive(b"[[1]]", config),
            Err(PdfDiffError::ResourceLimitExceeded(message))
                if message.contains("RESOURCE_LIMIT_RECURSION_DEPTH")
        ));
    }

    #[test]
    fn parses_indirect_object_with_dictionary() {
        let parsed = parse_indirect_object(
            b"7 0 obj\n<< /Type /Page /Contents 8 0 R >>\nendobj\n",
            ParseConfig::default(),
        )
        .expect("indirect object should parse");

        assert_eq!(
            parsed.id,
            ObjectId {
                number: 7,
                generation: 0
            }
        );
        assert_eq!(parsed.byte_range, ByteRange::new(0, 48));
        assert_eq!(parsed.value_byte_range, ByteRange::new(8, 41));
        let PdfPrimitive::Dictionary(entries) = parsed.value else {
            panic!("expected dictionary object");
        };
        assert_eq!(
            entries[1],
            (
                "Contents".into(),
                PdfPrimitive::Reference(ObjectId {
                    number: 8,
                    generation: 0
                })
            )
        );
        assert_eq!(parsed.stream, None);
    }

    #[test]
    fn parses_indirect_stream_object() {
        let parsed = parse_indirect_object(
            b"8 0 obj\n<< /Length 5 >>\nstream\nHello\nendstream\nendobj",
            ParseConfig::default(),
        )
        .expect("stream object should parse");

        assert_eq!(
            parsed.id,
            ObjectId {
                number: 8,
                generation: 0
            }
        );
        assert_eq!(
            parsed.value,
            PdfPrimitive::Dictionary(vec![("Length".into(), PdfPrimitive::Integer(5))])
        );
        let stream = parsed.stream.expect("stream should be present");
        assert_eq!(stream.bytes, b"Hello");
        assert_eq!(stream.raw_bytes, b"Hello");
        assert_eq!(stream.declared_length, Some(5));
        assert!(stream.filters.is_empty());
        assert!(stream.decoded);
        assert_eq!(stream.byte_range, ByteRange::new(31, 36));
    }

    #[test]
    fn decodes_flate_stream_object() {
        let compressed = flate_bytes(b"BT (Hello) Tj ET");
        let object = stream_object_with_filter(&compressed, "/FlateDecode");
        let parsed = parse_indirect_object(&object, ParseConfig::default())
            .expect("flate stream object should parse");
        let stream = parsed.stream.expect("stream should be present");

        assert_eq!(stream.bytes, b"BT (Hello) Tj ET");
        assert_eq!(stream.raw_bytes, compressed);
        assert_eq!(stream.filters, vec!["FlateDecode"]);
        assert!(stream.decoded);
    }

    #[test]
    fn preserves_raw_bytes_when_flate_decode_fails() {
        let object = stream_object_with_filter(b"not deflated", "/FlateDecode");
        let parsed = parse_indirect_object(&object, ParseConfig::default())
            .expect("failed decode should keep partial stream object");
        let stream = parsed.stream.expect("stream should be present");

        assert_eq!(stream.bytes, b"not deflated");
        assert_eq!(stream.raw_bytes, b"not deflated");
        assert_eq!(stream.filters, vec!["FlateDecode"]);
        assert!(!stream.decoded);
    }

    #[test]
    fn decodes_ascii_hex_stream_object() {
        let object =
            stream_object_with_filter(b"4254202848656c6c6f2920546a204554>", "/ASCIIHexDecode");
        let parsed = parse_indirect_object(&object, ParseConfig::default())
            .expect("ASCIIHex stream object should parse");
        let stream = parsed.stream.expect("stream should be present");

        assert_eq!(stream.bytes, b"BT (Hello) Tj ET");
        assert_eq!(stream.filters, vec!["ASCIIHexDecode"]);
        assert!(stream.decoded);
    }

    #[test]
    fn decodes_run_length_stream_object() {
        let object = stream_object_with_filter(b"\x02ABC\xFD!\x80", "/RunLengthDecode");
        let parsed = parse_indirect_object(&object, ParseConfig::default())
            .expect("RunLength stream object should parse");
        let stream = parsed.stream.expect("stream should be present");

        assert_eq!(stream.bytes, b"ABC!!!!");
        assert_eq!(stream.filters, vec!["RunLengthDecode"]);
        assert!(stream.decoded);
    }

    #[test]
    fn decodes_filter_chain_and_preserves_decode_params() {
        let compressed = flate_bytes(b"BT (Hello chain) Tj ET");
        let ascii_hex = ascii_hex_bytes(&compressed);
        let object = stream_object_with_filter(
            &ascii_hex,
            "[/ASCIIHexDecode /FlateDecode] /DecodeParms [null << /Predictor 1 >>]",
        );
        let parsed = parse_indirect_object(&object, ParseConfig::default())
            .expect("filter-chain stream object should parse");
        let stream = parsed.stream.expect("stream should be present");

        assert_eq!(stream.bytes, b"BT (Hello chain) Tj ET");
        assert_eq!(stream.filters, vec!["ASCIIHexDecode", "FlateDecode"]);
        assert_eq!(stream.decode_params.len(), 2);
        assert_eq!(stream.decode_params[0], None);
        assert!(
            stream.decode_params[1]
                .as_deref()
                .is_some_and(|value| value.contains("/Predictor 1"))
        );
        assert!(stream.decoded);
    }

    #[test]
    fn rejects_encrypted_pdf_with_stable_code() {
        let error =
            PdfDocument::parse(b"%PDF-1.7\n1 0 obj\n<< /Type /Catalog /Encrypt 2 0 R >>\nendobj\n")
                .expect_err("encrypted PDF should not be parsed");

        assert!(matches!(
            error,
            PdfDiffError::UnsupportedPdf(message) if message.contains("UNSUPPORTED_ENCRYPTION")
        ));
    }

    #[test]
    fn parses_compact_stream_dictionary_metadata_without_xref() {
        let compressed = flate_bytes(b"BT (compact) Tj ET");
        let mut pdf = format!(
            "%PDF-1.7\n1 0 obj\n<</Filter /FlateDecode/Length {}>>\nstream\n",
            compressed.len()
        )
        .into_bytes();
        pdf.extend_from_slice(&compressed);
        pdf.extend_from_slice(b"\nendstream\nendobj\n");

        let document = PdfDocument::parse(&pdf).expect("compact stream dictionary should parse");
        let stream = document.objects[0]
            .stream
            .as_ref()
            .expect("stream should be present");

        assert_eq!(stream.filters, vec!["FlateDecode"]);
        assert_eq!(stream.declared_length, Some(compressed.len()));
        assert_eq!(stream.bytes, b"BT (compact) Tj ET");
        assert!(stream.decoded);
        assert!(
            document
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != "UNSUPPORTED_STREAM_FILTER")
        );
    }

    #[test]
    fn rejects_unterminated_indirect_object_gracefully() {
        assert!(matches!(
            parse_indirect_object(b"1 0 obj\n<< /Type /Page >>", ParseConfig::default()),
            Err(PdfDiffError::InvalidInput(_))
        ));
    }

    #[test]
    fn rejects_unterminated_stream_object_gracefully() {
        assert!(matches!(
            parse_indirect_object(
                b"1 0 obj\n<< /Length 5 >>\nstream\nHello\nendobj",
                ParseConfig::default()
            ),
            Err(PdfDiffError::InvalidInput(_))
        ));
    }

    #[test]
    fn parses_classic_xref_table_and_trailer() {
        let fixture = classic_xref_pdf();
        let xref = parse_xref_table(&fixture.bytes, ParseConfig::default())
            .expect("classic xref should parse");

        assert_eq!(xref.start_offset, fixture.xref_offset);
        assert_eq!(xref.entries.len(), 3);
        assert_eq!(xref.entries[1].byte_offset, fixture.object_offsets[0]);
        assert!(xref.entries[1].in_use);
        assert_eq!(
            dictionary_entry(&xref.trailer, "Root"),
            Some(&PdfPrimitive::Reference(ObjectId {
                number: 1,
                generation: 0
            }))
        );
    }

    #[test]
    fn builds_object_store_with_lookup() {
        let fixture = classic_xref_pdf();
        let store = parse_object_store(&fixture.bytes, ParseConfig::default())
            .expect("object store should parse");

        assert_eq!(store.objects.len(), 2);
        let catalog = store
            .get(ObjectId {
                number: 1,
                generation: 0,
            })
            .expect("catalog should be available");
        assert_eq!(catalog.byte_range.start, fixture.object_offsets[0]);
        assert!(matches!(catalog.value, PdfPrimitive::Dictionary(_)));

        let stream = store
            .get(ObjectId {
                number: 2,
                generation: 0,
            })
            .and_then(|object| object.stream.as_ref())
            .expect("stream object should be available");
        assert_eq!(stream.bytes, b"Hello");
    }

    #[test]
    fn detects_recursive_reference_chain_safely() {
        let fixture = recursive_reference_pdf();
        let store = parse_object_store(&fixture, ParseConfig::default())
            .expect("recursive object store should parse");

        let error = store
            .resolve_reference_chain(
                ObjectId {
                    number: 1,
                    generation: 0,
                },
                ResourceLimits::default(),
            )
            .expect_err("cycle should be rejected");

        assert!(matches!(
            error,
            PdfDiffError::ResourceLimitExceeded(message)
                if message.contains("RESOURCE_LIMIT_REFERENCE_CYCLE")
        ));
    }

    #[test]
    fn enforces_reference_chain_depth_limit() {
        let fixture = reference_chain_pdf();
        let store = parse_object_store(&fixture, ParseConfig::default())
            .expect("reference chain store should parse");
        let limits = ResourceLimits {
            max_indirect_depth: 1,
            ..ResourceLimits::default()
        };

        let error = store
            .resolve_reference_chain(
                ObjectId {
                    number: 1,
                    generation: 0,
                },
                limits,
            )
            .expect_err("depth should be rejected");

        assert!(matches!(
            error,
            PdfDiffError::ResourceLimitExceeded(message)
                if message.contains("RESOURCE_LIMIT_REFERENCE_DEPTH")
        ));
    }

    #[test]
    fn parses_controlled_xref_stream() {
        let fixture = xref_stream_pdf(false);
        let xref = parse_xref_table(&fixture.bytes, ParseConfig::default())
            .expect("xref stream should parse");

        assert_eq!(xref.start_offset, fixture.xref_offset);
        assert_eq!(xref.entries.len(), 4);
        assert_eq!(xref.entries[1].kind, XrefEntryKind::InUse);
        assert_eq!(xref.entries[1].byte_offset, fixture.object_offsets[0]);
        assert_eq!(
            dictionary_entry(&xref.trailer, "Type"),
            Some(&PdfPrimitive::Name("XRef".into()))
        );

        let store = parse_object_store(&fixture.bytes, ParseConfig::default())
            .expect("xref stream object store should parse");
        assert!(
            store
                .get(ObjectId {
                    number: 1,
                    generation: 0
                })
                .is_some()
        );
    }

    #[test]
    fn parses_compressed_xref_stream_entry() {
        let fixture = xref_stream_pdf(true);
        let xref = parse_xref_table(&fixture.bytes, ParseConfig::default())
            .expect("xref stream should parse");

        assert!(xref.entries.iter().any(|entry| entry.kind
            == XrefEntryKind::Compressed {
                object_stream: ObjectId {
                    number: 2,
                    generation: 0
                },
                object_index: 7
            }));
    }

    #[test]
    fn resolves_object_stored_inside_object_stream() {
        let fixture = object_stream_pdf(false);
        let store = parse_object_store(&fixture.bytes, ParseConfig::default())
            .expect("object stream store should parse");

        let embedded = store
            .get(ObjectId {
                number: 5,
                generation: 0,
            })
            .expect("embedded object should resolve");

        assert_eq!(
            embedded.value,
            PdfPrimitive::Dictionary(vec![("Type".into(), PdfPrimitive::Name("Page".into()))])
        );
        assert_eq!(
            embedded.embedded_source,
            Some(EmbeddedObjectSource {
                object_stream_id: ObjectId {
                    number: 2,
                    generation: 0
                },
                object_index: 0
            })
        );
    }

    #[test]
    fn malformed_object_stream_fails_softly() {
        let fixture = object_stream_pdf(true);
        let error = parse_object_store(&fixture.bytes, ParseConfig::default())
            .expect_err("malformed object stream should fail");

        assert!(matches!(
            error,
            PdfDiffError::InvalidInput(message) if message.contains("object stream")
        ));
    }

    #[test]
    fn malformed_xref_stream_reports_exact_error() {
        let fixture = malformed_xref_stream_pdf();
        let error =
            parse_xref_table(&fixture, ParseConfig::default()).expect_err("xref should fail");

        assert!(matches!(
            error,
            PdfDiffError::InvalidInput(message) if message.contains("/W")
        ));
    }

    #[test]
    fn xref_stream_object_is_not_reported_as_unsupported() {
        let pdf = b"%PDF-1.7
1 0 obj
<< /Type /XRef /Length 0 >>
stream

endstream
endobj
";
        let document = PdfDocument::parse(pdf).expect("xref stream fixture should parse partially");

        assert_eq!(document.diagnostics.len(), 0);
    }

    #[test]
    fn reports_incremental_update_markers_and_selects_latest_startxref() {
        let mut bytes = classic_xref_pdf().bytes;
        let previous_xref_offset = locate_startxref(&bytes).expect("first xref should exist");
        let second_xref_offset = bytes.len();
        bytes.extend_from_slice(
            format!(
                "xref\n0 1\n0000000000 65535 f \ntrailer\n<< /Size 1 /Prev {previous_xref_offset} >>\nstartxref\n{second_xref_offset}\n%%EOF\n"
            )
            .as_bytes(),
        );

        let document = PdfDocument::parse(&bytes)
            .expect("incremental fixture should recover by scanning objects");
        let incremental_update = document
            .incremental_update
            .as_ref()
            .expect("incremental metadata should be exposed");

        assert!(document.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "INCREMENTAL_UPDATE_DETECTED"
                && diagnostic.message.contains("latest revision")
        }));
        assert!(
            document
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "PRIOR_REVISION_PRESENT")
        );
        assert_eq!(incremental_update.revision_count, 2);
        assert_eq!(
            incremental_update.selected_startxref_offset,
            Some(second_xref_offset)
        );
        assert_eq!(
            incremental_update.prior_startxref_offsets,
            vec![previous_xref_offset]
        );
        assert_eq!(
            incremental_update.trailer_prev_offsets,
            vec![previous_xref_offset]
        );
    }

    #[test]
    fn reports_recovery_when_xref_is_damaged() {
        let mut bytes = classic_xref_pdf().bytes;
        let marker = find_last_keyword(&bytes, b"startxref").expect("startxref should exist");
        bytes[marker + b"startxref".len() + 1] = b'x';

        let document = PdfDocument::parse(&bytes).expect("object scan should recover fixture");

        assert!(
            document
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "XREF_RECOVERY_USED")
        );
        assert!(!document.objects.is_empty());
    }

    #[test]
    fn resolves_first_page_content_stream() {
        let document = PdfDocument::parse(minimal_pdf()).expect("fixture should parse");
        let content = document
            .first_page_content()
            .expect("page content should resolve");

        assert_eq!(content.page_index, 0);
        assert_eq!(
            content.stream_object_id,
            ObjectId {
                number: 4,
                generation: 0
            }
        );
        assert_eq!(content.bytes, b"BT /F1 12 Tf 72 720 Td (Hello) Tj ET");
    }

    #[test]
    fn resolves_first_page_content_stream_array_in_order() {
        let document =
            PdfDocument::parse(multi_content_stream_pdf()).expect("fixture should parse");
        let contents = document
            .first_page_contents()
            .expect("page content streams should resolve");

        assert_eq!(
            document.pages[0].content_object_ids,
            vec![
                ObjectId {
                    number: 4,
                    generation: 0
                },
                ObjectId {
                    number: 5,
                    generation: 0
                }
            ]
        );
        assert_eq!(contents.len(), 2);
        assert_eq!(contents[0].bytes, b"BT /F1 12 Tf 72 720 Td (Hello) Tj");
        assert_eq!(contents[1].bytes, b"( world) Tj ET");
    }

    #[test]
    fn resolves_page_content_streams_across_all_pages() {
        let document = PdfDocument::parse(multi_page_pdf()).expect("fixture should parse");
        let contents = document.page_contents();

        assert_eq!(contents.len(), 2);
        assert_eq!(contents[0].page_index, 0);
        assert_eq!(
            contents[0].bytes,
            b"BT /F1 12 Tf 72 720 Td (First page) Tj ET"
        );
        assert_eq!(contents[1].page_index, 1);
        assert_eq!(
            contents[1].bytes,
            b"BT /F1 12 Tf 72 720 Td (Second page) Tj ET"
        );
    }

    #[test]
    fn resolves_page_tree_order_and_inherited_page_attributes() {
        let document = PdfDocument::parse(nested_page_tree_pdf()).expect("fixture should parse");

        assert_eq!(document.pages.len(), 2);
        assert_eq!(
            document.pages[0].object_id,
            ObjectId {
                number: 5,
                generation: 0
            }
        );
        assert_eq!(
            document.pages[1].object_id,
            ObjectId {
                number: 3,
                generation: 0
            }
        );
        assert_eq!(
            document.pages[0].media_box,
            Some(spdfdiff_types::Rect {
                x0: 0.0,
                y0: 0.0,
                x1: 612.0,
                y1: 792.0,
            })
        );
        assert_eq!(
            document.pages[0].crop_box,
            Some(spdfdiff_types::Rect {
                x0: 10.0,
                y0: 20.0,
                x1: 300.0,
                y1: 400.0,
            })
        );
        assert_eq!(document.pages[0].rotation, 90);
        assert_eq!(document.pages[1].rotation, 180);
        assert_eq!(
            document.pages[0].resources_object_id,
            Some(ObjectId {
                number: 8,
                generation: 0
            })
        );
        assert!(document.pages[0].has_resources);
        assert_eq!(document.page_contents()[0].stream_object_id.number, 6);
        assert_eq!(document.page_contents()[1].stream_object_id.number, 4);
    }

    #[test]
    fn emits_diagnostic_for_page_without_contents() {
        let pdf = b"%PDF-1.7
1 0 obj
<< /Type /Page >>
endobj
";
        let document = PdfDocument::parse(pdf).expect("fixture should parse partially");
        assert_eq!(document.pages, Vec::new());
        assert_eq!(document.diagnostics.len(), 1);
        assert_eq!(document.diagnostics[0].code, "MISSING_CONTENT_STREAM");
        assert_eq!(
            document.diagnostics[0].severity,
            DiagnosticSeverity::Warning
        );
    }

    #[test]
    fn emits_diagnostic_for_unsupported_stream_filter() {
        let pdf = b"%PDF-1.7
1 0 obj
<< /Type /Page /Contents 2 0 R >>
endobj
2 0 obj
<< /Length 5 /Filter /DCTDecode >>
stream
abcde
endstream
endobj
";
        let document = PdfDocument::parse(pdf).expect("fixture should parse partially");

        assert!(document.diagnostics.iter().any(|diagnostic| diagnostic.code
            == "UNSUPPORTED_STREAM_FILTER"
            && diagnostic.object
                == Some(ObjectId {
                    number: 2,
                    generation: 0
                })));
    }

    #[test]
    fn emits_diagnostic_when_flate_decode_fails() {
        let pdf = b"%PDF-1.7
1 0 obj
<< /Type /Page /Contents 2 0 R >>
endobj
2 0 obj
<< /Length 12 /Filter /FlateDecode >>
stream
not deflated
endstream
endobj
";
        let document = PdfDocument::parse(pdf).expect("fixture should parse partially");

        assert!(document.diagnostics.iter().any(|diagnostic| diagnostic.code
            == "STREAM_DECODE_FAILED"
            && diagnostic.object
                == Some(ObjectId {
                    number: 2,
                    generation: 0
                })));
    }

    #[test]
    fn parses_binary_stream_bytes_without_text_offset_panic() {
        let mut pdf = b"%PDF-1.7
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>
endobj
4 0 obj
<< /Length 5 >>
stream
"
        .to_vec();
        pdf.extend_from_slice(&[0xff, 0xfe, b'A', b'\r', b'\n']);
        pdf.extend_from_slice(
            b"
endstream
endobj
",
        );

        let document =
            PdfDocument::parse(&pdf).expect("binary stream fixture should parse without panic");
        let stream = document
            .objects
            .iter()
            .find(|object| {
                object.id
                    == ObjectId {
                        number: 4,
                        generation: 0,
                    }
            })
            .and_then(|object| object.stream.as_ref())
            .expect("content stream should be parsed");

        assert_eq!(stream.bytes, vec![0xff, 0xfe, b'A']);
        assert!(stream.byte_range.end <= pdf.len());
    }

    #[test]
    fn enforces_decoded_stream_size_limit() {
        let compressed = flate_bytes(b"decoded text");
        let object = stream_object_with_filter(&compressed, "/FlateDecode");
        let config = ParseConfig {
            limits: ResourceLimits {
                max_decoded_stream_bytes: 4,
                ..ResourceLimits::default()
            },
        };

        assert!(matches!(
            parse_indirect_object(&object, config),
            Err(PdfDiffError::ResourceLimitExceeded(message))
                if message.contains("RESOURCE_LIMIT_DECODED_STREAM_BYTES")
        ));
    }

    #[test]
    fn parses_simple_tagged_structure_tree() {
        let document = PdfDocument::parse(tagged_pdf()).expect("tagged fixture should parse");
        let structure = document.tagged_structure(ParseConfig::default());

        assert_eq!(
            structure.root_object_id,
            Some(ObjectId {
                number: 6,
                generation: 0
            })
        );
        assert_eq!(structure.roots.len(), 2);
        assert_eq!(structure.roots[0].structure_type, "H1");
        assert_eq!(structure.roots[0].mcids, vec![0]);
        assert_eq!(structure.roots[1].structure_type, "P");
        assert_eq!(structure.roots[1].mcids, vec![1]);
        assert_eq!(
            structure.parent_tree,
            vec![TaggedParentTreeEntry {
                struct_parent: 0,
                element_object_ids: vec![
                    ObjectId {
                        number: 7,
                        generation: 0
                    },
                    ObjectId {
                        number: 8,
                        generation: 0
                    }
                ],
            }]
        );
        assert!(structure.diagnostics.is_empty());
    }

    #[test]
    fn parses_tagged_role_map_and_applies_mapped_structure_type() {
        let document = PdfDocument::parse(tagged_role_map_pdf())
            .expect("tagged role-map fixture should parse");
        let structure = document.tagged_structure(ParseConfig::default());

        assert_eq!(
            structure.role_map,
            vec![TaggedRoleMapEntry {
                source: "ChapterTitle".to_owned(),
                target: "H1".to_owned(),
            }]
        );
        assert_eq!(structure.roots.len(), 1);
        assert_eq!(structure.roots[0].structure_type, "ChapterTitle");
        assert_eq!(
            structure.roots[0].mapped_structure_type.as_deref(),
            Some("H1")
        );
        assert_eq!(structure.roots[0].mcids, vec![0]);
        assert!(structure.diagnostics.is_empty());
    }

    #[test]
    fn reports_missing_tagged_structure_as_empty_summary() {
        let document = PdfDocument::parse(minimal_pdf()).expect("fixture should parse");
        let structure = document.tagged_structure(ParseConfig::default());

        assert_eq!(structure.root_object_id, None);
        assert!(structure.roots.is_empty());
        assert!(structure.parent_tree.is_empty());
        assert!(structure.diagnostics.is_empty());
    }

    fn minimal_pdf() -> &'static [u8] {
        b"%PDF-1.7
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>
endobj
4 0 obj
<< /Length 38 >>
stream
BT /F1 12 Tf 72 720 Td (Hello) Tj ET
endstream
endobj
"
    }

    fn tagged_pdf() -> &'static [u8] {
        b"%PDF-1.7
1 0 obj
<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /Contents 4 0 R /StructParents 0 >>
endobj
4 0 obj
<< /Length 66 >>
stream
BT /H1 << /MCID 0 >> BDC (Title) Tj EMC /P << /MCID 1 >> BDC (Body) Tj EMC ET
endstream
endobj
6 0 obj
<< /Type /StructTreeRoot /K [7 0 R 8 0 R] /ParentTree << /Nums [0 [7 0 R 8 0 R]] >> >>
endobj
7 0 obj
<< /Type /StructElem /S /H1 /P 6 0 R /K 0 >>
endobj
8 0 obj
<< /Type /StructElem /S /P /P 6 0 R /K 1 >>
endobj
"
    }

    fn tagged_role_map_pdf() -> &'static [u8] {
        b"%PDF-1.7
1 0 obj
<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /Contents 4 0 R /StructParents 0 >>
endobj
4 0 obj
<< /Length 49 >>
stream
BT /ChapterTitle << /MCID 0 >> BDC (Title) Tj EMC ET
endstream
endobj
6 0 obj
<< /Type /StructTreeRoot /K [7 0 R] /RoleMap << /ChapterTitle /H1 >> >>
endobj
7 0 obj
<< /Type /StructElem /S /ChapterTitle /P 6 0 R /K 0 >>
endobj
"
    }

    fn multi_content_stream_pdf() -> &'static [u8] {
        b"%PDF-1.7
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /Contents [4 0 R 5 0 R] >>
endobj
4 0 obj
<< /Length 35 >>
stream
BT /F1 12 Tf 72 720 Td (Hello) Tj
endstream
endobj
5 0 obj
<< /Length 13 >>
stream
( world) Tj ET
endstream
endobj
"
    }

    fn multi_page_pdf() -> &'static [u8] {
        b"%PDF-1.7
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>
endobj
4 0 obj
<< /Length 43 >>
stream
BT /F1 12 Tf 72 720 Td (First page) Tj ET
endstream
endobj
5 0 obj
<< /Type /Page /Parent 2 0 R /Contents 6 0 R >>
endobj
6 0 obj
<< /Length 44 >>
stream
BT /F1 12 Tf 72 720 Td (Second page) Tj ET
endstream
endobj
"
    }

    fn nested_page_tree_pdf() -> &'static [u8] {
        b"%PDF-1.7
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [7 0 R] /Count 2 /MediaBox [0 0 612 792] /Resources 8 0 R /Rotate 90 >>
endobj
3 0 obj
<< /Type /Page /Parent 7 0 R /Contents 4 0 R /Rotate 180 >>
endobj
4 0 obj
<< /Length 44 >>
stream
BT /F1 12 Tf 72 720 Td (Second page) Tj ET
endstream
endobj
5 0 obj
<< /Type /Page /Parent 7 0 R /Contents 6 0 R >>
endobj
6 0 obj
<< /Length 43 >>
stream
BT /F1 12 Tf 72 720 Td (First page) Tj ET
endstream
endobj
7 0 obj
<< /Type /Pages /Parent 2 0 R /Kids [5 0 R 3 0 R] /Count 2 /CropBox [10 20 300 400] >>
endobj
8 0 obj
<< /Font << /F1 9 0 R >> >>
endobj
9 0 obj
<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>
endobj
"
    }

    struct ClassicXrefFixture {
        bytes: Vec<u8>,
        object_offsets: Vec<usize>,
        xref_offset: usize,
    }

    fn classic_xref_pdf() -> ClassicXrefFixture {
        classic_xref_pdf_with_objects(&[
            b"1 0 obj\n<< /Type /Catalog >>\nendobj\n".as_slice(),
            b"2 0 obj\n<< /Length 5 >>\nstream\nHello\nendstream\nendobj\n".as_slice(),
        ])
    }

    fn recursive_reference_pdf() -> Vec<u8> {
        classic_xref_pdf_with_objects(&[
            b"1 0 obj\n2 0 R\nendobj\n".as_slice(),
            b"2 0 obj\n1 0 R\nendobj\n".as_slice(),
        ])
        .bytes
    }

    fn reference_chain_pdf() -> Vec<u8> {
        classic_xref_pdf_with_objects(&[
            b"1 0 obj\n2 0 R\nendobj\n".as_slice(),
            b"2 0 obj\n3 0 R\nendobj\n".as_slice(),
            b"3 0 obj\n<< /Type /Catalog >>\nendobj\n".as_slice(),
        ])
        .bytes
    }

    fn classic_xref_pdf_with_objects(objects: &[&[u8]]) -> ClassicXrefFixture {
        let mut bytes = b"%PDF-1.7\n".to_vec();
        let mut object_offsets = Vec::new();

        for object in objects {
            object_offsets.push(bytes.len());
            bytes.extend_from_slice(object);
        }

        let xref_offset = bytes.len();
        bytes.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
        bytes.extend_from_slice(b"0000000000 65535 f \n");
        for offset in &object_offsets {
            bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        bytes.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
                objects.len() + 1,
                xref_offset
            )
            .as_bytes(),
        );

        ClassicXrefFixture {
            bytes,
            object_offsets,
            xref_offset,
        }
    }

    fn xref_stream_pdf(include_compressed_entry: bool) -> ClassicXrefFixture {
        let mut bytes = b"%PDF-1.7\n".to_vec();
        let mut object_offsets = Vec::new();

        object_offsets.push(bytes.len());
        bytes.extend_from_slice(b"1 0 obj\n<< /Type /Catalog >>\nendobj\n");

        object_offsets.push(bytes.len());
        bytes.extend_from_slice(b"2 0 obj\n<< /Length 5 >>\nstream\nHello\nendstream\nendobj\n");

        let mut xref_data = Vec::new();
        push_xref_stream_entry(&mut xref_data, 0, 0, 65_535);
        push_xref_stream_entry(&mut xref_data, 1, object_offsets[0], 0);
        push_xref_stream_entry(&mut xref_data, 1, object_offsets[1], 0);
        if include_compressed_entry {
            push_xref_stream_entry(&mut xref_data, 2, 2, 7);
        } else {
            push_xref_stream_entry(&mut xref_data, 0, 0, 0);
        }
        let compressed = flate_bytes(&xref_data);
        let xref_offset = bytes.len();
        bytes.extend_from_slice(
            format!(
                "4 0 obj\n<< /Type /XRef /Size 4 /Root 1 0 R /W [1 4 2] /Index [0 4] /Length {} /Filter /FlateDecode >>\nstream\n",
                compressed.len()
            )
            .as_bytes(),
        );
        bytes.extend_from_slice(&compressed);
        bytes.extend_from_slice(
            format!("\nendstream\nendobj\nstartxref\n{xref_offset}\n%%EOF\n").as_bytes(),
        );

        ClassicXrefFixture {
            bytes,
            object_offsets,
            xref_offset,
        }
    }

    fn object_stream_pdf(malformed: bool) -> ClassicXrefFixture {
        let mut bytes = b"%PDF-1.7\n".to_vec();
        let mut object_offsets = Vec::new();

        object_offsets.push(bytes.len());
        bytes.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Page 5 0 R >>\nendobj\n");

        let embedded_body = b"<< /Type /Page >>";
        let object_stream_header = if malformed { "5 0" } else { "5 0 " };
        let object_count = if malformed { 2 } else { 1 };
        let first = object_stream_header.len();
        object_offsets.push(bytes.len());
        bytes.extend_from_slice(
            format!(
                "2 0 obj\n<< /Type /ObjStm /N {object_count} /First {first} /Length {} >>\nstream\n",
                first + embedded_body.len()
            )
            .as_bytes(),
        );
        bytes.extend_from_slice(object_stream_header.as_bytes());
        bytes.extend_from_slice(embedded_body);
        bytes.extend_from_slice(b"\nendstream\nendobj\n");

        let xref_offset = bytes.len();
        let mut xref_data = Vec::new();
        push_xref_stream_entry(&mut xref_data, 0, 0, 65_535);
        push_xref_stream_entry(&mut xref_data, 1, object_offsets[0], 0);
        push_xref_stream_entry(&mut xref_data, 1, object_offsets[1], 0);
        push_xref_stream_entry(&mut xref_data, 0, 0, 0);
        push_xref_stream_entry(&mut xref_data, 1, xref_offset, 0);
        push_xref_stream_entry(&mut xref_data, 2, 2, 0);
        let compressed = flate_bytes(&xref_data);
        bytes.extend_from_slice(
            format!(
                "4 0 obj\n<< /Type /XRef /Size 6 /Root 1 0 R /W [1 4 2] /Index [0 6] /Length {} /Filter /FlateDecode >>\nstream\n",
                compressed.len()
            )
            .as_bytes(),
        );
        bytes.extend_from_slice(&compressed);
        bytes.extend_from_slice(
            format!("\nendstream\nendobj\nstartxref\n{xref_offset}\n%%EOF\n").as_bytes(),
        );

        ClassicXrefFixture {
            bytes,
            object_offsets,
            xref_offset,
        }
    }

    fn malformed_xref_stream_pdf() -> Vec<u8> {
        let mut bytes = b"%PDF-1.7\n".to_vec();
        let xref_offset = bytes.len();
        bytes.extend_from_slice(
            format!(
                "1 0 obj\n<< /Type /XRef /Size 1 /Length 0 >>\nstream\n\nendstream\nendobj\nstartxref\n{xref_offset}\n%%EOF\n"
            )
            .as_bytes(),
        );
        bytes
    }

    fn push_xref_stream_entry(entries: &mut Vec<u8>, kind: u8, field_2: usize, field_3: usize) {
        entries.push(kind);
        entries.extend_from_slice(&(field_2 as u32).to_be_bytes());
        entries.extend_from_slice(&(field_3 as u16).to_be_bytes());
    }

    fn flate_bytes(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(bytes).expect("fixture should compress");
        encoder.finish().expect("fixture should finish")
    }

    fn ascii_hex_bytes(bytes: &[u8]) -> Vec<u8> {
        let mut out = String::new();
        for byte in bytes {
            out.push_str(&format!("{byte:02X}"));
        }
        out.push('>');
        out.into_bytes()
    }

    fn stream_object_with_filter(stream_bytes: &[u8], filter: &str) -> Vec<u8> {
        let mut object = format!(
            "9 0 obj\n<< /Length {} /Filter {filter} >>\nstream\n",
            stream_bytes.len()
        )
        .into_bytes();
        object.extend_from_slice(stream_bytes);
        object.extend_from_slice(b"\nendstream\nendobj");
        object
    }

    fn dictionary_entry<'a>(value: &'a PdfPrimitive, key: &str) -> Option<&'a PdfPrimitive> {
        let PdfPrimitive::Dictionary(entries) = value else {
            return None;
        };
        entries
            .iter()
            .find(|(entry_key, _)| entry_key == key)
            .map(|(_, entry_value)| entry_value)
    }

    #[cfg(feature = "fuzzing")]
    mod fuzzing {
        use super::*;

        #[test]
        fn parser_fuzz_target_handles_malformed_inputs_without_panic() {
            let cases: &[&[u8]] = &[
                b"",
                b"%PDF-1.7\nxref",
                b"%PDF-1.7\n1 0 obj\n<< /Length 999999999 >>\nstream\nx",
                b"%PDF-1.7\n1 0 obj\n[ /Name (unterminated",
                b"%PDF-1.7\nstartxref\n999999\n%%EOF",
            ];

            for case in cases {
                let _ = PdfDocument::parse(case);
                let _ = parse_primitive(case, ParseConfig::default());
            }
        }
    }
}
