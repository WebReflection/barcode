//! UPC-A: structurally EAN-13 with a leading 0. Round-trips through the
//! shared decoder machinery; fixtures encoder verified bit-exact vs zint.

use barcode_core::{BitImage, Symbology, SymbologyDecoder};
use barcode_ean13::{Ean13Decoder, UpcaDecoder};
use barcode_fixtures::encode_upca;

fn decode_with(digits12: &str, narrow: u32, quiet: u32) -> Vec<barcode_core::DecodeResult> {
    let c = encode_upca(digits12, narrow, 8, quiet);
    let img = BitImage::new(c.width, c.height, c.bits());
    let mut d = UpcaDecoder::new(2);
    d.decode(&img)
}

#[test]
fn roundtrip_and_symbology() {
    for digits in ["036000291452", "526018159085", "199351819093"] {
        let res = decode_with(digits, 2, 6);
        assert_eq!(res.len(), 1, "{digits}");
        assert_eq!(res[0].symbology, Symbology::UPCA);
        assert_eq!(res[0].text, digits);
    }
}

#[test]
fn ean13_decoder_still_sees_13_digits() {
    // the same symbol reported through the EAN-13 decoder keeps its 0 lead
    let c = encode_upca("036000291452", 2, 8, 6);
    let img = BitImage::new(c.width, c.height, c.bits());
    let mut d = Ean13Decoder::new(2);
    let res = d.decode(&img);
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].symbology, Symbology::EAN13);
    assert_eq!(res[0].text, "0036000291452");
}

#[test]
fn nonzero_lead_is_ean13_only() {
    // 4006381333931 (EAN-13, lead 4) must not surface as UPC-A
    let c = barcode_fixtures::encode_ean13("4006381333931", 2, 8, 6);
    let img = BitImage::new(c.width, c.height, c.bits());
    let mut upca = UpcaDecoder::new(2);
    assert!(upca.decode(&img).is_empty());
    let mut ean = Ean13Decoder::new(2);
    let res = ean.decode(&img);
    assert_eq!(res[0].text, "4006381333931");
}

#[test]
fn narrow_and_quiet_variations() {
    for (narrow, quiet) in [(1u32, 4u32), (3u32, 2u32), (2u32, 10u32)] {
        let res = decode_with("123456789012", narrow, quiet);
        assert_eq!(res.len(), 1, "narrow={narrow} quiet={quiet}");
        assert_eq!(res[0].text, "123456789012");
    }
}
