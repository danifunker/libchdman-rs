use libchdman_rs::{
    codec_exists, codec_name, parse_codec_spec, CHD_CODEC_CD_FLAC, CHD_CODEC_CD_LZMA,
    CHD_CODEC_CD_ZLIB, CHD_CODEC_FLAC, CHD_CODEC_LZMA, CHD_CODEC_NONE, CHD_CODEC_ZLIB,
    CHD_CODEC_ZSTD,
};

#[test]
fn known_codecs_exist() {
    for tag in [
        CHD_CODEC_ZLIB,
        CHD_CODEC_ZSTD,
        CHD_CODEC_LZMA,
        CHD_CODEC_FLAC,
        CHD_CODEC_CD_ZLIB,
        CHD_CODEC_CD_LZMA,
        CHD_CODEC_CD_FLAC,
    ] {
        assert!(codec_exists(tag), "{:#x}", tag);
        assert!(codec_name(tag).is_some(), "{:#x}", tag);
    }
}

#[test]
fn unknown_codec_rejected() {
    let bogus = libchdman_rs::make_tag(b'q', b'q', b'q', b'q');
    assert!(!codec_exists(bogus));
    assert!(codec_name(bogus).is_none());
}

#[test]
fn parse_none() {
    assert_eq!(parse_codec_spec("none").unwrap(), [CHD_CODEC_NONE; 4]);
}

#[test]
fn parse_single() {
    assert_eq!(
        parse_codec_spec("cdlz").unwrap(),
        [CHD_CODEC_CD_LZMA, 0, 0, 0]
    );
}

#[test]
fn parse_three() {
    assert_eq!(
        parse_codec_spec("cdlz,cdzl,cdfl").unwrap(),
        [CHD_CODEC_CD_LZMA, CHD_CODEC_CD_ZLIB, CHD_CODEC_CD_FLAC, 0]
    );
}

#[test]
fn parse_four() {
    assert_eq!(
        parse_codec_spec("zlib,zstd,lzma,flac").unwrap(),
        [
            CHD_CODEC_ZLIB,
            CHD_CODEC_ZSTD,
            CHD_CODEC_LZMA,
            CHD_CODEC_FLAC
        ]
    );
}

#[test]
fn parse_rejects_five() {
    assert!(parse_codec_spec("zlib,zstd,lzma,flac,cdlz").is_err());
}

#[test]
fn parse_rejects_empty() {
    assert!(parse_codec_spec("").is_err());
}

#[test]
fn parse_rejects_unknown_mnemonic() {
    assert!(parse_codec_spec("xxxx").is_err());
    assert!(parse_codec_spec("zlib,xxxx").is_err());
}

#[test]
fn parse_rejects_wrong_length() {
    assert!(parse_codec_spec("zli").is_err());
    assert!(parse_codec_spec("zlibb").is_err());
    assert!(parse_codec_spec("zlib,").is_err()); // trailing comma → empty 5th token
}
