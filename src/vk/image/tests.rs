use super::mip_level_count;

/// A full chain halves the longer side down to one texel.
#[test]
fn a_mip_chain_ends_at_one_texel() {
    assert_eq!(mip_level_count(1, 1), 1);
    assert_eq!(mip_level_count(2, 1), 2);
    assert_eq!(mip_level_count(256, 256), 9);
    assert_eq!(mip_level_count(257, 3), 9);
    assert_eq!(mip_level_count(2550, 3300), 12);
    assert_eq!(mip_level_count(0, 0), 1);
}
