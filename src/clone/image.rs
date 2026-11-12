use super::{Error, IMAGE_CAPACITY, Profile};

// DF884 table 45B864; ten adjacent callees:
// 19F4/391C, D6A58/D6F78, D7550/D9740, DB980/DD6DC, DF9F0/DFA38.
// Schema: accepted BA01 selector-schema.tsv, 188 ordered operations.
const FOOTER: &[u8; 16] = b"IcomCloneFormat3";

/// Convert the internal M=4997A-byte image through adjacent raw MapRev layouts.
/// The immutable `seed` is the caller's actual G generation, NOT a built-in
/// factory image. It is required only on routes using seed windows. All
/// intermediates are M bytes, zeroed before the ordered copies. Down-conversion
/// is lossy, not an inverse. Unknown profiles cannot silently select layout 6.
///
/// All eight DF884 callers use this same codec, but their pre/post metadata,
/// live G generation, file status and document adoption remain caller-owned.
/// A returned image is not proof of complete radio coverage or safe flashing.
pub fn convert_image(
    source: &[u8],
    source_profile: Profile,
    destination_profile: Profile,
    seed: Option<&[u8]>,
) -> Result<Vec<u8>, Error> {
    if source.len() != IMAGE_CAPACITY {
        return Err(Error::Invalid(
            "conversion requires internal image length".into(),
        ));
    }
    let mut image = source.to_vec();
    let mut selector = source_profile.selector();
    while selector != destination_profile.selector() {
        let next = if selector < destination_profile.selector() {
            selector + 1
        } else {
            selector - 1
        };
        let needs_seed = matches!((selector, next), (1, 2) | (2, 1) | (3, 4) | (4, 5));
        let g = if needs_seed {
            seed.filter(|g| g.len() == IMAGE_CAPACITY)
                .ok_or(Error::Invalid("conversion requires current G image".into()))?
        } else {
            &[]
        };
        image = adjacent(&image, selector, next, g);
        selector = next;
    }
    Ok(image)
}

// (destination, source, byte length). Zero gaps are deliberate source behavior,
// not default/factory data and not evidence that those bytes were acquired.
fn copies(d: &mut [u8], s: &[u8], windows: &[(usize, usize, usize)]) {
    for &(dst, src, n) in windows {
        d[dst..dst + n].copy_from_slice(&s[src..src + n]);
    }
}

fn adjacent(s: &[u8], from: u8, to: u8, g: &[u8]) -> Vec<u8> {
    let mut d = vec![0; IMAGE_CAPACITY];
    match (from, to) {
        (1, 2) => {
            copies(
                &mut d,
                s,
                &[
                    (0, 0, 0xa60f),
                    (0xaa0f, 0xa60f, 0x2ab60),
                    (0x35580, 0x35180, 0x2190),
                    (0x37740, 0x37340, 0x90a),
                    (0x38080, 0x37c80, 0x4f),
                    (0x380d1, 0x37ccf, 0x21),
                    (0x380f4, 0x37cf0, 0x3d6),
                    (0x384d2, 0x380ce, 8),
                    (0x384da, 0x380d6, 0xa),
                    (0x384e5, 0x380e1, 0x15),
                    (0x3871a, 0x3828e, 0x8b),
                    (0x387a6, 0x38319, 2),
                    (0x38b1b, 0x3871b, 0x5fb),
                    (0x39116, 0x38d16, 2),
                    (0x39140, 0x38d40, 0x165),
                    (0x392c0, 0x38ec0, 0x1d34),
                    (0x3b000, 0x3ac00, 0x97),
                    (0x3b09b, 0x3ac97, 2),
                    (0x3b0c0, 0x3acc0, 0x30),
                ],
            );
            copies(
                &mut d,
                g,
                &[
                    (0xa60f, 0xa60f, 0x400),
                    (0x3804a, 0x3808a, 1),
                    (0x380cf, 0x3930f, 2),
                    (0x380f2, 0x39332, 2),
                    (0x387a5, 0x39a02, 1),
                    (0x387a8, 0x39a05, 0x61),
                    (0x3b097, 0x3c897, 4),
                ],
            );
            d[0x3556f] = 2;
            for i in 0..8 {
                d[0x384ca + i] = match s[0x380c6 + i] {
                    0x14 | 0x16 => 0x14,
                    0x15 | 0x17 => 0x15,
                    b => b,
                };
            }
            for i in 0..17 {
                copies(&mut d, s, &[(0x384fa + 0x20 * i, 0x380f6 + 0x18 * i, 0x18)]);
                copies(&mut d, g, &[(0x38512 + 0x20 * i, 0x39752 + 0x20 * i, 8)]);
            }
            clear_seven(&mut d, 0x38b2e, 0x38e8e);
            d[0x3b0f0..0x3b100].copy_from_slice(FOOTER);
        }
        (2, 1) => {
            copies(
                &mut d,
                s,
                &[
                    (0, 0, 0xa60f),
                    (0xa60f, 0xaa0f, 0x2ab60),
                    (0x35180, 0x35580, 0x2190),
                    (0x37340, 0x37740, 0x90a),
                    (0x37c80, 0x38080, 0x4f),
                    (0x37ccf, 0x380d1, 0x21),
                    (0x37cf0, 0x380f4, 0x25),
                    (0x37d16, 0x3811a, 0x35a),
                    (0x38071, 0x38475, 0x55),
                    (0x380ce, 0x384d2, 8),
                    (0x380d6, 0x384da, 0xa),
                    (0x380e1, 0x384e5, 0x15),
                    (0x3828e, 0x3871a, 0x8b),
                    (0x38319, 0x387a6, 2),
                    (0x3871b, 0x38b1b, 0x5fd),
                    (0x38d40, 0x39140, 0x165),
                    (0x38ec0, 0x392c0, 0x1d34),
                    (0x3ac00, 0x3b000, 0x97),
                    (0x3ac97, 0x3b09b, 2),
                    (0x3acc0, 0x3b0c0, 0x30),
                ],
            );
            d[0x3516f] = 1;
            for i in 0..17 {
                if d[0x37388 + i] > 2 {
                    d[0x37388 + i] = g[0x377c8 + i];
                }
                copies(&mut d, s, &[(0x380f6 + 0x18 * i, 0x384fa + 0x20 * i, 0x18)]);
            }
            d[0x37d15] = if s[0x38119] == 1 && s[0x387a8] == 1 {
                0
            } else {
                s[0x38119]
            };
            d[0x38070] = if s[0x38474] == 2 {
                g[0x396b4]
            } else {
                s[0x38474]
            };
            for i in 0..8 {
                d[0x380c6 + i] = if matches!(s[0x384ca + i], 0x16 | 0x17 | 0x1e) {
                    g[0x3970a + i]
                } else {
                    s[0x384ca + i]
                };
            }
            clear_seven(&mut d, 0x3872e, 0x38a8e);
            d[0x3acf0..0x3ad00].copy_from_slice(FOOTER);
        }
        (2, 3) | (3, 2) => {
            copies(&mut d, s, &[(0, 0, 0x35580), (0x35580, 0x35580, 0x2b00)]);
            let windows = [
                (0x392c0, 0x38080, 0x10c0),
                (0x3a780, 0x39140, 0xe8),
                (0x3c89b, 0x39228, 0x18),
                (0x3a868, 0x39240, 0x63),
                (0x3c8b3, 0x392a3, 1),
                (0x3a8cb, 0x392a4, 1),
                (0x3ab80, 0x392c0, 0x17fc),
                (0x3c8b4, 0x3aabc, 0xb5),
                (0x3c37c, 0x3ab71, 0x483),
                (0x3c800, 0x3b000, 0x9b),
                (0x3c969, 0x3b09b, 2),
                (0x3c980, 0x3b0c0, 0x30),
            ];
            if to == 3 {
                copies(&mut d, s, &windows);
            } else {
                // Reverse-copy order matters for overlapping windows. This list
                // is in source schema order, NOT inferred as an inverse map.
                copies(
                    &mut d,
                    s,
                    &[
                        (0x38080, 0x392c0, 0x10c0),
                        (0x39140, 0x3a780, 0xe8),
                        (0x39240, 0x3a868, 0x63),
                        (0x392a4, 0x3a8cb, 1),
                        (0x392c0, 0x3ab80, 0x17fc),
                        (0x3ab71, 0x3c37c, 0x483),
                        (0x3b000, 0x3c800, 0x9b),
                        (0x39228, 0x3c89b, 0x18),
                        (0x392a3, 0x3c8b3, 1),
                        (0x3aabc, 0x3c8b4, 0xb5),
                        (0x3b09b, 0x3c969, 2),
                        (0x3b0c0, 0x3c980, 0x30),
                    ],
                );
            }
            d[0x3556f] = to;
            d[if to == 3 { 0x39724 } else { 0x384e4 }] = 0;
            let footer = if to == 3 { 0x3c9b0 } else { 0x3b0f0 };
            d[footer..footer + 16].copy_from_slice(FOOTER);
        }
        (3, 4) | (4, 3) => {
            if to == 4 {
                copies(&mut d, s, &[(0, 0, 0x32c19)]);
                for i in 0..50 {
                    copies(&mut d, s, &[(0x309f5 + 0x30 * i, 0x309f5 + 0x2f * i, 0x2f)]);
                    d[0x30a24 + 0x30 * i] = if s[0x30a19 + 0x2f * i..0x30a1d + 0x2f * i] == [0; 4] {
                        1
                    } else {
                        g[0x30a24 + 0x30 * i]
                    };
                }
                copies(
                    &mut d,
                    s,
                    &[
                        (0x31355, 0x31323, 0x424c),
                        (0x355c0, 0x35580, 0x2b00),
                        (0x392c0, 0x392c0, 0x464),
                        (0x39725, 0x39725, 0x29b),
                        (0x399c1, 0x399c0, 0x89),
                        (0x39d5b, 0x39d5b, 0xa),
                        (0x39d66, 0x39d65, 0x5f1),
                        (0x3a357, 0x3a356, 2),
                    ],
                );
                d[0x355a1] = 4;
                d[0x399c0] = g[0x399c0];
                d[0x3a073..0x3a077].fill(0);
                d[0x3a312..0x3a315].fill(0);
            } else {
                copies(&mut d, s, &[(0, 0, 0x32be7)]);
                for i in 0..50 {
                    copies(&mut d, s, &[(0x309f5 + 0x2f * i, 0x309f5 + 0x30 * i, 0x2f)]);
                }
                copies(
                    &mut d,
                    s,
                    &[
                        (0x31323, 0x31355, 0x424c),
                        (0x35580, 0x355c0, 0x2b00),
                        (0x392c0, 0x392c0, 0x464),
                        (0x39725, 0x39725, 0x29b),
                        (0x399c0, 0x399c1, 0x89),
                        (0x39d5b, 0x39d5b, 0xa),
                        (0x39d65, 0x39d66, 0x5f1),
                        (0x3a356, 0x3a357, 2),
                    ],
                );
                d[0x3556f] = 3;
                d[0x3a072..0x3a076].fill(0);
                d[0x3a311..0x3a314].fill(0);
            }
            copies(
                &mut d,
                s,
                &[(0x3a780, 0x3a780, 0x180), (0x3ab80, 0x3ab80, 0x1e30)],
            );
            d[0x39724] = 0;
            d[0x3c9b0..0x3c9c0].copy_from_slice(FOOTER);
        }
        (4, 5) | (5, 4) => {
            copies(&mut d, s, &[(0, 0, 0x355c0), (0x355c0, 0x355c0, 0x2b00)]);
            if to == 5 {
                copies(&mut d, s, &[(0x392c0, 0x392c0, 0x464)]);
                copies(&mut d, g, &[(0x399c1, 0x399c1, 0x1c)]);
                copies(
                    &mut d,
                    s,
                    &[(0x399dd, 0x399c1, 0x89), (0x3d400, 0x3c980, 0x30)],
                );
            } else {
                copies(&mut d, s, &[(0x392c0, 0x392c0, 0x2ea)]);
                if s[0x395a8] != 0x20 {
                    d[0x395a2..0x395aa].fill(0x20);
                }
                copies(
                    &mut d,
                    s,
                    &[
                        (0x395aa, 0x395aa, 0x17a),
                        (0x399c1, 0x399dd, 0x89),
                        (0x3c980, 0x3d400, 0x30),
                    ],
                );
            }
            copies(
                &mut d,
                s,
                &[
                    (0x39725, 0x39725, 0x29c),
                    (0x39d5b, 0x39d5b, 0x5fe),
                    (0x3a780, 0x3a780, 0x180),
                    (0x3ab80, 0x3ab80, 0x1e00),
                ],
            );
            d[0x355a1] = to;
            d[0x39724] = 0;
            let footer = if to == 5 { 0x3d430 } else { 0x3c9b0 };
            d[footer..footer + 16].copy_from_slice(FOOTER);
        }
        (5, 6) | (6, 5) => {
            d.copy_from_slice(s);
            if to == 5 {
                d[0x399bb] = d[0x399bb].min(1);
                d[0x399c0] = d[0x399c0].min(2);
            }
            d[0x3a318..0x3a31b].fill(0);
            d[0x39724] = 0;
            d[0x355a1] = to;
        }
        _ => unreachable!("adjacent validated selectors"),
    }
    d
}

fn clear_seven(d: &mut [u8], a: usize, b: usize) {
    for i in 186..=192 {
        d[a + 4 * i..a + 4 * i + 4].fill(0);
        d[b + 3 * i..b + 3 * i + 3].fill(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_36_routes_match_independent_ordered_schema_oracle() {
        // Python independently interprets the accepted 188-row TSV. Fixed
        // synthetic source/seed, not production-code output or factory bytes.
        let expected = [
            [
                "9beac810ec15dad91d4f8666e20133dc",
                "5235bdb13de9a66be331eb7d2f702bef",
                "ad458671ec9cc69ba4433c3467cee188",
                "67462558e7901f9579dc256a658ad240",
                "84c5b5741b2b4ef02641d3bee785d875",
                "3e68f031d715caac4c69037ade4af8de",
            ],
            [
                "58b8c44332a31909b2d7d6bddee58884",
                "9beac810ec15dad91d4f8666e20133dc",
                "199c7e172204fe9ce29fdbe7acd6c866",
                "425064ec6a8a748a5ec9af1fa449d14a",
                "f1c56fb1780e5999922d95d84e35f2ad",
                "9e74026a782641b79a1395e1cdedeb1e",
            ],
            [
                "be5a226f6cc09cfb0c4388c35f781545",
                "1ad70dda9b39ac58721a3108e6df3c38",
                "9beac810ec15dad91d4f8666e20133dc",
                "4f14707f138a34816f7396966313fd3b",
                "10308b576a1315f84f631071959e3aa9",
                "a245c5adbc1865a5dfae3a4b60e975a8",
            ],
            [
                "559bd1ec4e3551616777a8198ea9d51a",
                "5a6b785880108db22d091835ec288bb4",
                "a4fd35d624530d04e868524318085238",
                "9beac810ec15dad91d4f8666e20133dc",
                "a1b7119200ae63cdaaa22561b2267b3b",
                "9699840d0b752a8a9ff90299038a1fae",
            ],
            [
                "5c2b2d0ac3249bf2e9c8bf4acf65e876",
                "19ad2eb9b5d3b977edfba77e4a2f469e",
                "22b363650e7d076bcef1cc19e931f974",
                "7ce797974b945987df8d82e182ee9113",
                "9beac810ec15dad91d4f8666e20133dc",
                "5dc3c8e36fdf00374cc0758fbacf64b5",
            ],
            [
                "6a295ab06559bc2eed89da9d25f0005b",
                "57a62da976350181155df6713b25f204",
                "23af267c2721e44430cdefdfd22b8731",
                "5d8b8c48328e57290cb23cb925abedc8",
                "2364a00fc5cdbc830337cecf02a20cf2",
                "9beac810ec15dad91d4f8666e20133dc",
            ],
        ];
        let mut s: Vec<_> = (0..IMAGE_CAPACITY)
            .map(|i| (i * 73 + i / 256) as u8)
            .collect();
        let g: Vec<_> = (0..IMAGE_CAPACITY)
            .map(|i| (i * 31 + 7 + i / 4096) as u8)
            .collect();
        s[0x380c6..0x380ce].copy_from_slice(&[0x14, 0x15, 0x16, 0x17, 0x1e, 0, 255, 85]);
        s[0x384ca..0x384d2].copy_from_slice(&[0x16, 0x17, 0x1e, 0x14, 0x15, 0, 255, 85]);
        s[0x38119] = 1;
        s[0x387a8] = 1;
        s[0x38474] = 2;
        for i in (0..50).step_by(2) {
            s[0x30a19 + 0x2f * i..0x30a1d + 0x2f * i].fill(0);
        }
        assert_eq!(
            format!("{:x}", md5::compute(&s)),
            "9beac810ec15dad91d4f8666e20133dc"
        );
        assert_eq!(
            format!("{:x}", md5::compute(&g)),
            "abc2274c31685d2bbe53a18f30ed07ca"
        );
        for (a, row) in expected.iter().enumerate() {
            for (b, hash) in row.iter().enumerate() {
                let d = convert_image(
                    &s,
                    Profile::try_from(a as u8 + 1).unwrap(),
                    Profile::try_from(b as u8 + 1).unwrap(),
                    Some(&g),
                )
                .unwrap();
                assert_eq!(
                    format!("{:x}", md5::compute(d)),
                    *hash,
                    "{} -> {}",
                    a + 1,
                    b + 1
                );
            }
        }
    }
    #[test]
    fn boundaries_seed_loss_and_seven_inclusive_slots() {
        let s = vec![0xa5; IMAGE_CAPACITY];
        let g = vec![0x3c; IMAGE_CAPACITY];
        let p = |n| Profile::try_from(n).unwrap();
        assert!(convert_image(&s, p(1), p(6), None).is_err());
        assert!(convert_image(&s[..IMAGE_CAPACITY - 1], p(6), p(6), None).is_err());
        let d = convert_image(&s, p(1), p(2), Some(&g)).unwrap();
        assert_eq!(&d[0xa60f..0xaa0f], &[0x3c; 0x400]);
        assert_eq!(&d[0x38b2e + 4 * 192..0x38b2e + 4 * 193], &[0; 4]);
        assert_eq!(d[0x38b2e + 4 * 185], 0xa5);
        let d = convert_image(&s, p(6), p(5), None).unwrap();
        assert_eq!((d[0x399bb], d[0x399c0], d[0x355a1]), (1, 2, 5));
        assert_eq!(&d[0x3a318..0x3a31b], &[0; 3]);
        assert_eq!(convert_image(&s, p(6), p(6), None).unwrap(), s);
    }
}
