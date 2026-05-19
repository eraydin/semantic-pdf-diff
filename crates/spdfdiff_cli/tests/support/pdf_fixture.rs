#[derive(Debug, Clone, PartialEq)]
pub struct MinimalPdf {
    pages: Vec<MinimalPdfPage>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MinimalPdfPage {
    text: String,
    x: f32,
    y: f32,
}

impl MinimalPdf {
    #[must_use]
    pub fn single_page(text: impl Into<String>) -> Self {
        Self::new(vec![MinimalPdfPage::new(text, 72.0, 720.0)])
    }

    #[must_use]
    pub fn single_page_at(text: impl Into<String>, x: f32, y: f32) -> Self {
        Self::new(vec![MinimalPdfPage::new(text, x, y)])
    }

    #[must_use]
    pub fn new(pages: Vec<MinimalPdfPage>) -> Self {
        assert!(
            !pages.is_empty(),
            "minimal PDF fixtures need at least one page"
        );
        Self { pages }
    }

    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let page_count = self.pages.len();
        let font_object_id = 3 + page_count * 2;
        let kids = (0..page_count)
            .map(|index| format!("{} 0 R", 3 + index * 2))
            .collect::<Vec<_>>()
            .join(" ");

        let mut objects = Vec::new();
        objects.push("<< /Type /Catalog /Pages 2 0 R >>\n".to_owned());
        objects.push(format!(
            "<< /Type /Pages /Kids [{kids}] /Count {page_count} >>\n"
        ));
        for (index, page) in self.pages.iter().enumerate() {
            let page_object_id = 3 + index * 2;
            let content_object_id = page_object_id + 1;
            objects.push(format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 {font_object_id} 0 R >> >> /Contents {content_object_id} 0 R >>\n"
            ));
            let content = format!(
                "BT /F1 12 Tf {:.2} {:.2} Td ({}) Tj ET\n",
                page.x,
                page.y,
                escape_literal_string(&page.text)
            );
            objects.push(format!(
                "<< /Length {} >>\nstream\n{}endstream\n",
                content.len(),
                content
            ));
        }
        objects.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\n".to_owned());

        let mut pdf = b"%PDF-1.7\n".to_vec();
        let mut offsets = Vec::new();
        for (index, body) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
            pdf.extend_from_slice(body.as_bytes());
            pdf.extend_from_slice(b"endobj\n");
        }

        let xref_offset = pdf.len();
        pdf.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
        pdf.extend_from_slice(b"0000000000 65535 f \n");
        for offset in offsets {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
                objects.len() + 1
            )
            .as_bytes(),
        );
        pdf
    }
}

impl MinimalPdfPage {
    #[must_use]
    pub fn new(text: impl Into<String>, x: f32, y: f32) -> Self {
        Self {
            text: text.into(),
            x,
            y,
        }
    }
}

fn escape_literal_string(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}
