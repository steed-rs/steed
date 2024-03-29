use std::{borrow::Cow, fmt::Write, print, println, unreachable};

fn upper_backslash(c: u8) -> u8 {
    if c == b'/' {
        b'\\'
    } else {
        c.to_ascii_uppercase()
    }
}

const S_HASHTABLE: [u32; 16] = [
    0x486E26EE, 0xDCAA16B3, 0xE1918EEF, 0x202DAFDB, 0x341C7DC7, 0x1C365303, 0x40EF2D37, 0x65FD5E49,
    0xD6057177, 0x904ECE93, 0x1C38024F, 0x98FD323B, 0xE3061AE7, 0xA39B0FA1, 0x9797F25F, 0xE4444563,
];

pub fn sstrhash(val: &[u8], no_caseconv: bool, mut seed: u32) -> u32 {
    if seed == 0 {
        seed = 0x7FED7FED;
    }

    let mut shift = 0xEEEEEEEE;
    for mut c in val.iter().copied() {
        if !no_caseconv {
            c = upper_backslash(c);
        }

        seed = (S_HASHTABLE[c as usize >> 4] - S_HASHTABLE[c as usize & 0xF]) ^ (shift + seed);
        shift = c as u32 + seed + 33 * shift + 3;
    }

    if seed != 0 {
        seed
    } else {
        1
    }
}

pub fn parse_hex_bytes<const N: usize>(s: &str) -> Option<[u8; N]> {
    if s.len() != N * 2 {
        return None;
    }

    let mut res = [0u8; N];
    for i in 0..N {
        res[i] = u8::from_str_radix(&s[i * 2..][..2], 16).ok()?;
    }
    Some(res)
}

pub fn format_hex_bytes_be<const N: usize>(val: &[u8; N]) -> String {
    let mut res = String::with_capacity(N * 2);
    for byte in val.iter().rev() {
        res.write_fmt(format_args!("{:02x}", byte)).unwrap();
    }
    res
}

pub fn format_hex_bytes_le<const N: usize>(val: &[u8; N]) -> String {
    let mut res = String::with_capacity(N * 2);
    for byte in val.iter() {
        res.write_fmt(format_args!("{:02x}", byte)).unwrap();
    }
    res
}

pub fn asciiz(val: &[u8]) -> Cow<str> {
    let first_zero = val.iter().position(|&b| b == 0).unwrap_or(val.len());
    let val = &val[..first_zero];
    String::from_utf8_lossy(val)
}

pub fn hexdump(val: &[u8], start: usize, end: usize) {
    const CHUNK_SIZE: usize = 16;

    let mut skipping_zeroes = false;
    let mut zeroes_start = 0;
    for (chunk_idx, chunk) in val[start..end].chunks(CHUNK_SIZE).enumerate() {
        let chunk_start = start + CHUNK_SIZE * chunk_idx;

        if chunk.iter().all(|b| *b == 0) {
            if !skipping_zeroes {
                skipping_zeroes = true;
                zeroes_start = chunk_start;
            }
            continue;
        }

        if skipping_zeroes {
            println!(
                "{:08x} => {:08x} was zero ({} bytes)",
                zeroes_start,
                chunk_start,
                chunk_start - zeroes_start
            );
            skipping_zeroes = false;
        }

        print!("{:08x}: ", chunk_start);

        for word in chunk.chunks(2) {
            match word {
                [a, b] => print!("{:02x}{:02x} ", a, b),
                [a] => print!("{:02x}   ", a),
                _ => unreachable!(),
            }
        }

        let bytes_left = CHUNK_SIZE - chunk.len();
        for _ in 0..(bytes_left + bytes_left / 2) {
            print!(" ");
        }

        for byte in chunk {
            if byte.is_ascii_graphic() {
                print!("{}", *byte as char);
            } else {
                print!(".");
            }
        }

        println!();
    }
}
