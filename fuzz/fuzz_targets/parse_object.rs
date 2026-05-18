#![no_main]

use libfuzzer_sys::fuzz_target;
use pdf_core::{parse_indirect_object, parse_primitive};
use spdfdiff_types::{ParseConfig, ResourceLimits};

fuzz_target!(|data: &[u8]| {
    let config = ParseConfig {
        limits: ResourceLimits {
            max_file_bytes: 128 * 1024,
            max_objects: 128,
            max_indirect_depth: 16,
            max_stream_bytes: 32 * 1024,
            max_decoded_stream_bytes: 64 * 1024,
            max_content_ops_per_page: 4_096,
            max_pages: 64,
        },
    };

    let _ = parse_primitive(data, config);
    let _ = parse_indirect_object(data, config);
});
