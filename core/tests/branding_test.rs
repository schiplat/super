//! Branding storage: magic-byte sniffing, persistence pair (logo + meta),
//! extension swap on re-upload, and clearing.

use super_core::branding::{clear_logo, read_logo, sniff_image, store_logo};
use tempfile::TempDir;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\nfakepngbytes";
const JPEG: &[u8] = b"\xff\xd8\xff\xe0fakejpegbytes";

#[test]
fn sniffing_covers_supported_types_and_rejects_junk() {
    assert_eq!(sniff_image(PNG).unwrap(), ("png", "image/png"));
    assert_eq!(sniff_image(JPEG).unwrap(), ("jpg", "image/jpeg"));
    assert_eq!(sniff_image(b"GIF89a...").unwrap(), ("gif", "image/gif"));
    assert_eq!(
        sniff_image(b"RIFF\x00\x00\x00\x00WEBPVP8 ").unwrap(),
        ("webp", "image/webp")
    );
    assert_eq!(
        sniff_image(b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>").unwrap(),
        ("svg", "image/svg+xml")
    );
    assert_eq!(
        sniff_image(b"<?xml version=\"1.0\"?><svg xmlns=\"x\"></svg>").unwrap(),
        ("svg", "image/svg+xml")
    );
    assert!(sniff_image(b"").is_err());
    assert!(sniff_image(b"not an image at all").is_err());
    assert!(sniff_image(b"<notasvg></notasvg>").is_err());
}

#[tokio::test]
async fn logo_round_trip_swap_and_clear() {
    let dir = TempDir::new().unwrap().path().join("data/branding");

    // Nothing stored: read is None.
    assert!(read_logo(&dir).is_none());

    // Store a PNG, then re-store an SVG: the extension swaps and only one
    // logo remains.
    let (_, png_type) = store_logo(&dir, PNG, "tester").unwrap();
    assert_eq!(png_type, "image/png");
    let (path, svg_type) = store_logo(&dir, b"<svg></svg>", "tester").unwrap();
    assert_eq!(svg_type, "image/svg+xml");
    assert!(path.ends_with("logo.svg"));
    assert!(!dir.join("logo.png").exists());
    let (path2, _) = read_logo(&dir).unwrap();
    assert!(path2.ends_with("logo.svg"));

    // Clear removes both the logo and the meta.
    clear_logo(&dir);
    assert!(read_logo(&dir).is_none());
}

#[tokio::test]
async fn oversize_upload_is_rejected() {
    let dir = TempDir::new().unwrap().path().join("data/branding");
    let big = vec![0x89u8; super_core::branding::MAX_LOGO_BYTES + 1]; // PNG magic, oversize
    let error = store_logo(&dir, &big, "tester").unwrap_err();
    assert!(error.contains("too large"), "{error}");
}
