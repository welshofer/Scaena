//! # scaena-pdf
//!
//! The PDF painter in the browser (PLAN 2.54): `scaena export --format pdf`'s
//! ([`scaena_export::pdf`]), as a WASM module of its own. The editor's module lays the deck's
//! pages out as the CLI does and hands them over as bytes (`Player.pdfLaidOut`): each page's
//! frame, and the fonts and images the frames name. This module draws them with krilla. The
//! editor's module leaves krilla out: only an export draws a PDF (SPEC §15).

use scaena_export::pdf::{PdfSettings, Prepared, prepared};
use wasm_bindgen::prelude::*;

/// The PDF of `laid`, a deck's pages as `Player.pdfLaidOut` lays them out: the bytes
/// `scaena export --format pdf` writes for the same deck.
#[wasm_bindgen]
pub fn pdf(laid: &[u8]) -> Result<Vec<u8>, JsError> {
    let laid = Prepared::from_bytes(laid).map_err(|e| JsError::new(&e.to_string()))?;
    prepared(&laid, &PdfSettings::default()).map_err(|e| JsError::new(&e.to_string()))
}

#[cfg(test)]
mod tests {
    use scaena_export::pdf::{PdfSettings, Prepared, prepared};
    use scaena_ops::export::{Progress, pdf_document, pdf_laid_out};

    /// Laid out, handed over as bytes, and drawn here: the PDF `export --format pdf` writes.
    #[test]
    fn it_draws_the_pdf_the_cli_writes() {
        for bundle in ["../../docs/examples/revenue.deck.json", "../../tests/fixtures/torture.scaena"] {
            let b = scaena_store::Bundle::open(std::path::Path::new(bundle)).unwrap();
            let (laid, pages) = pdf_laid_out(&b, None, &Progress::default()).unwrap();
            let bytes = laid.to_bytes().unwrap();
            let ours = prepared(&Prepared::from_bytes(&bytes).unwrap(), &PdfSettings::default()).unwrap();
            let (cli, cli_pages) = pdf_document(&b, None, &PdfSettings::default()).unwrap();
            assert_eq!(pages, cli_pages, "{bundle}");
            assert!(ours == cli, "{bundle}: the PDF drawn from the laid-out bytes differs from the CLI's");
        }
    }

    #[test]
    fn other_bytes_are_an_error_that_says_why() {
        let b = scaena_store::Bundle::open(std::path::Path::new("../../docs/examples/revenue.deck.json")).unwrap();
        let bytes = pdf_laid_out(&b, None, &Progress::default()).unwrap().0.to_bytes().unwrap();
        let said = |bytes: &[u8]| Prepared::from_bytes(bytes).unwrap_err().to_string();
        assert!(said(b"%PDF-1.7").contains("does not begin `SPDF`"));
        assert!(said(&bytes[..6]).contains("ends early"));
        assert!(said(&bytes[..bytes.len() - 1]).contains("ends early"));
        let mut longer = bytes.clone();
        longer.push(0);
        assert!(said(&longer).contains("past its last file (1 B)"));
    }
}
