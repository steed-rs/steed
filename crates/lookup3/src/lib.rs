/// mix -- mix 3 32-bit values reversibly.
///
/// This is reversible, so any information in (a,b,c) before mix() is
/// still in (a,b,c) after mix().
///
/// If four pairs of (a,b,c) inputs are run through mix(), or through
/// mix() in reverse, there are at least 32 bits of the output that
/// are sometimes the same for one pair and different for another pair.
/// This was tested for:
/// * pairs that differed by one bit, by two bits, in any combination
///   of top bits of (a,b,c), or in any combination of bottom bits of
///   (a,b,c).
/// * "differ" is defined as +, -, ^, or ~^.  For + and -, I transformed
///   the output delta to a Gray code (a^(a>>1)) so a string of 1's (as
///   is commonly produced by subtraction) look like a single 1-bit
///   difference.
/// * the base values were pseudorandom, all zero but one bit set, or
///   all zero plus a counter that starts at zero.
///
/// Some k values for my "a-=c; a^=rot(c,k); c+=b;" arrangement that
/// satisfy this are
///     4  6  8 16 19  4
///     9 15  3 18 27 15
///    14  9  3  7 17  3
/// Well, "9 15 3 18 27 15" didn't quite get 32 bits diffing
/// for "differ" defined as + with a one-bit base and a two-bit delta.  I
/// used http://burtleburtle.net/bob/hash/avalanche.html to choose
/// the operations, constants, and arrangements of the variables.
///
/// This does not achieve avalanche.  There are input bits of (a,b,c)
/// that fail to affect some output bits of (a,b,c), especially of a.  The
/// most thoroughly mixed value is c, but it doesn't really even achieve
/// avalanche in c.
///
/// This allows some parallelism.  Read-after-writes are good at doubling
/// the number of bits affected, so the goal of mixing pulls in the opposite
/// direction as the goal of parallelism.  I did what I could.  Rotates
/// seem to cost as much as shifts on every machine I could lay my hands
/// on, and rotates are much kinder to the top and bottom bits, so I used
/// rotates.
#[inline(always)]
fn mix(a: &mut u32, b: &mut u32, c: &mut u32) {
    *a = a.wrapping_sub(*c);
    *a ^= u32::rotate_left(*c, 4);
    *c = c.wrapping_add(*b);
    *b = b.wrapping_sub(*a);
    *b ^= u32::rotate_left(*a, 6);
    *a = a.wrapping_add(*c);
    *c = c.wrapping_sub(*b);
    *c ^= u32::rotate_left(*b, 8);
    *b = b.wrapping_add(*a);
    *a = a.wrapping_sub(*c);
    *a ^= u32::rotate_left(*c, 16);
    *c = c.wrapping_add(*b);
    *b = b.wrapping_sub(*a);
    *b ^= u32::rotate_left(*a, 19);
    *a = a.wrapping_add(*c);
    *c = c.wrapping_sub(*b);
    *c ^= u32::rotate_left(*b, 4);
    *b = b.wrapping_add(*a);
}

/// final -- final mixing of 3 32-bit values (a,b,c) into c
///
/// Pairs of (a,b,c) values differing in only a few bits will usually
/// produce values of c that look totally different.  This was tested for
/// * pairs that differed by one bit, by two bits, in any combination
///   of top bits of (a,b,c), or in any combination of bottom bits of
///   (a,b,c).
/// * "differ" is defined as +, -, ^, or ~^.  For + and -, I transformed
///   the output delta to a Gray code (a^(a>>1)) so a string of 1's (as
///   is commonly produced by subtraction) look like a single 1-bit
///   difference.
/// * the base values were pseudorandom, all zero but one bit set, or
///   all zero plus a counter that starts at zero.
///
/// These constants passed:
///  14 11 25 16 4 14 24
///  12 14 25 16 4 14 24
/// and these came close:
///   4  8 15 26 3 22 24
///  10  8 15 26 3 22 24
///  11  8 15 26 3 22 24
#[inline(always)]
fn r#final(a: &mut u32, b: &mut u32, c: &mut u32) {
    *c ^= *b;
    *c = c.wrapping_sub(u32::rotate_left(*b, 14));
    *a ^= *c;
    *a = a.wrapping_sub(u32::rotate_left(*c, 11));
    *b ^= *a;
    *b = b.wrapping_sub(u32::rotate_left(*a, 25));
    *c ^= *b;
    *c = c.wrapping_sub(u32::rotate_left(*b, 16));
    *a ^= *c;
    *a = a.wrapping_sub(u32::rotate_left(*c, 4));
    *b ^= *a;
    *b = b.wrapping_sub(u32::rotate_left(*a, 14));
    *c ^= *b;
    *c = c.wrapping_sub(u32::rotate_left(*b, 24));
}

/// This works on all machines.  To be useful, it requires
/// -- that the key be an array of uint32_t's, and
/// -- that the length be the number of uint32_t's in the key
///
/// The function hashword() is identical to hashlittle() on little-endian
/// machines, and identical to hashbig() on big-endian machines,
/// except that the length has to be measured in uint32_ts rather than in
/// bytes.  hashlittle() is more complicated than hashword() only because
/// hashlittle() has to dance around fitting the key bytes into registers.
#[inline(always)]
pub fn hashword(k: &[u32], initval: u32) -> u32 {
    let (c, _) = hashword2(k, initval, 0);
    c
}

/// hashword2() -- same as hashword(), but take two seeds and return two
/// 32-bit values.  pc and pb must both be nonnull, and \*pc and \*pb must
/// both be initialized with seeds.  If you pass in (\*pb)==0, the output
/// (\*pc) will be the same as the return value from hashword().
pub fn hashword2(mut k: &[u32], pc: u32, pb: u32) -> (u32, u32) {
    let mut a = 0xdeadbeef_u32
        .wrapping_add((k.len() as u32) << 2)
        .wrapping_add(pc);
    let mut b = a;
    let mut c = a.wrapping_add(pb);

    while k.len() > 3 {
        a = a.wrapping_add(k[0]);
        b = b.wrapping_add(k[1]);
        c = c.wrapping_add(k[2]);
        k = &k[3..];
    }

    let remaining = k.len();
    if remaining >= 3 {
        c = c.wrapping_add(k[2]);
    }
    if remaining >= 2 {
        b = b.wrapping_add(k[1]);
    }
    if remaining >= 1 {
        a = a.wrapping_add(k[0]);
        r#final(&mut a, &mut b, &mut c);
    }

    (c, b)
}

/// hashlittle() -- hash a variable-length key into a 32-bit value
///   k       : the key (the unaligned variable-length array of bytes)
///   length  : the length of the key, counting by bytes
///   initval : can be any 4-byte value
/// Returns a 32-bit value.  Every bit of the key affects every bit of
/// the return value.  Two keys differing by one or two bits will have
/// totally different hash values.
///
/// The best hash table sizes are powers of 2.  There is no need to do
/// mod a prime (mod is sooo slow!).  If you need less than 32 bits,
/// use a bitmask.  For example, if you need only 10 bits, do
///   h = (h & hashmask(10));
/// In which case, the hash table should have hashsize(10) elements.
///
/// If you are hashing n strings (uint8_t **)k, do it like this:
///   for (i=0, h=0; i<n; ++i) h = hashlittle( k[i], len[i], h);
///
/// By Bob Jenkins, 2006.  bob_jenkins@burtleburtle.net.  You may use this
/// code any way you wish, private, educational, or commercial.  It's free.
///
/// Use for hash table lookup, or anything where one collision in 2^^32 is
/// acceptable.  Do NOT use for cryptographic purposes.
pub fn hashlittle(k: &[u8], initval: u32) -> u32 {
    let (c, _) = hashlittle2(k, initval, 0);
    c
}

/// hashlittle2: return 2 32-bit hash values
///
/// This is identical to hashlittle(), except it returns two 32-bit hash
/// values instead of just one.  This is good enough for hash table
/// lookup with 2^^64 buckets, or if you want a second hash if you're not
/// happy with the first, or if you want a probably-unique 64-bit ID for
/// the key. \*pc is better mixed than \*pb, so use \*pc first.  If you want
/// a 64-bit value do something like `*pc + (((uint64_t)*pb)<<32)`.
pub fn hashlittle2(mut k: &[u8], pc: u32, pb: u32) -> (u32, u32) {
    // FIXME: We only implement the byte-by-byte version, and rely on the
    // compiler to optimize.
    let mut a = 0xdeadbeef_u32.wrapping_add(k.len() as u32).wrapping_add(pc);
    let mut b = a;
    let mut c = a.wrapping_add(pb);

    while k.len() > 12 {
        a = a.wrapping_add(k[0] as u32);
        a = a.wrapping_add((k[1] as u32) << 8);
        a = a.wrapping_add((k[2] as u32) << 16);
        a = a.wrapping_add((k[3] as u32) << 24);
        b = b.wrapping_add(k[4] as u32);
        b = b.wrapping_add((k[5] as u32) << 8);
        b = b.wrapping_add((k[6] as u32) << 16);
        b = b.wrapping_add((k[7] as u32) << 24);
        c = c.wrapping_add(k[8] as u32);
        c = c.wrapping_add((k[9] as u32) << 8);
        c = c.wrapping_add((k[10] as u32) << 16);
        c = c.wrapping_add((k[11] as u32) << 24);
        mix(&mut a, &mut b, &mut c);
        k = &k[12..];
    }

    let remaining = k.len();
    if remaining >= 12 {
        c = c.wrapping_add((k[11] as u32) << 24);
    }
    if remaining >= 11 {
        c = c.wrapping_add((k[10] as u32) << 16);
    }
    if remaining >= 10 {
        c = c.wrapping_add((k[9] as u32) << 8);
    }
    if remaining >= 9 {
        c = c.wrapping_add(k[8] as u32);
    }
    if remaining >= 8 {
        b = b.wrapping_add((k[7] as u32) << 24);
    }
    if remaining >= 7 {
        b = b.wrapping_add((k[6] as u32) << 16);
    }
    if remaining >= 6 {
        b = b.wrapping_add((k[5] as u32) << 8);
    }
    if remaining >= 5 {
        b = b.wrapping_add(k[4] as u32);
    }
    if remaining >= 4 {
        a = a.wrapping_add((k[3] as u32) << 24);
    }
    if remaining >= 3 {
        a = a.wrapping_add((k[2] as u32) << 16);
    }
    if remaining >= 2 {
        a = a.wrapping_add((k[1] as u32) << 8);
    }
    if remaining >= 1 {
        a = a.wrapping_add(k[0] as u32);
        r#final(&mut a, &mut b, &mut c);
    }

    (c, b)
}

/// hashbig():
/// This is the same as hashword() on big-endian machines.  It is different
/// from hashlittle() on all machines.  hashbig() takes advantage of
/// big-endian byte ordering.
pub fn hashbig(k: &[u8], initval: u32) -> u32 {
    let (c, _) = hashbig2(k, initval, 0);
    c
}

pub fn hashbig2(mut k: &[u8], pc: u32, pb: u32) -> (u32, u32) {
    // FIXME: We only implement the byte-by-byte version, and rely on the
    // compiler to optimize.
    let mut a = 0xdeadbeef_u32.wrapping_add(k.len() as u32).wrapping_add(pc);
    let mut b = a;
    let mut c = a.wrapping_add(pb);

    while k.len() > 12 {
        a = a.wrapping_add((k[0] as u32) << 24);
        a = a.wrapping_add((k[1] as u32) << 16);
        a = a.wrapping_add((k[2] as u32) << 8);
        a = a.wrapping_add(k[3] as u32);
        b = b.wrapping_add((k[4] as u32) << 24);
        b = b.wrapping_add((k[5] as u32) << 16);
        b = b.wrapping_add((k[6] as u32) << 8);
        b = b.wrapping_add(k[7] as u32);
        c = c.wrapping_add((k[8] as u32) << 24);
        c = c.wrapping_add((k[9] as u32) << 16);
        c = c.wrapping_add((k[10] as u32) << 8);
        c = c.wrapping_add(k[11] as u32);
        mix(&mut a, &mut b, &mut c);
        k = &k[12..];
    }

    let remaining = k.len();
    if remaining >= 12 {
        c = c.wrapping_add(k[11] as u32);
    }
    if remaining >= 11 {
        c = c.wrapping_add((k[10] as u32) << 8);
    }
    if remaining >= 10 {
        c = c.wrapping_add((k[9] as u32) << 16);
    }
    if remaining >= 9 {
        c = c.wrapping_add((k[8] as u32) << 24);
    }
    if remaining >= 8 {
        b = b.wrapping_add(k[7] as u32);
    }
    if remaining >= 7 {
        b = b.wrapping_add((k[6] as u32) << 8);
    }
    if remaining >= 6 {
        b = b.wrapping_add((k[5] as u32) << 16);
    }
    if remaining >= 5 {
        b = b.wrapping_add((k[4] as u32) << 24);
    }
    if remaining >= 4 {
        a = a.wrapping_add(k[3] as u32);
    }
    if remaining >= 3 {
        a = a.wrapping_add((k[2] as u32) << 8);
    }
    if remaining >= 2 {
        a = a.wrapping_add((k[1] as u32) << 16);
    }
    if remaining >= 1 {
        a = a.wrapping_add((k[0] as u32) << 24);
        r#final(&mut a, &mut b, &mut c);
    }

    (c, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[rustfmt::skip]
    fn test_known_values() {
        let values: &[(u32, u32, &[u8], u32, u32)] = &[
            (         0,          0, b"",                               0xdeadbeef, 0xdeadbeef),
            (0xdeadbeef,          0, b"",                               0xbd5b7dde, 0xdeadbeef),
            (         0,          0, b"Four score and seven years ago", 0x17770551, 0xce7226e6),
            (         1,          0, b"Four score and seven years ago", 0xe3607cae, 0xbd371de4),
            (         0,          1, b"Four score and seven years ago", 0xcd628161, 0x6cbea4b3),
        ];

        for &(pb, pc, k, e0, e1) in values {
            let (r0, r1) = hashlittle2(k, pc, pb);
            println!("hash is {:8x} {:8x}", r0, r1);
            assert_eq!(e0, r0, "primary output of hashlittle2 should match expected");
            assert_eq!(e1, r1, "secondary output of hashlittle2 should match expected");
        }
    }
}
