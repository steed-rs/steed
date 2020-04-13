use crate::tact::keys::TactKeys;
use binstream::{u24_be, u32_be, ByteReader};
use flate2::bufread::ZlibDecoder;
use libdeflate_sys::{libdeflate_free_decompressor, libdeflate_zlib_decompress};
use std::fmt::Debug;
use std::io::Read;
use zerocopy::{FromBytes, LayoutVerified};

#[derive(FromBytes)]
struct Header {
    magic: u32_be,
    header_size: u32_be,
}

#[derive(FromBytes)]
struct SubHeader {
    _flags: u8,
    chunk_count: u24_be,
}

#[derive(Clone, Debug, FromBytes)]
struct ChunkInfo {
    compressed_size: u32_be,
    decompressed_size: u32_be,
    checksum: [u8; 16],
}

// TODO: Rewrite as a std::io::Read impl?
pub fn decode_blte(tact_keys: &TactKeys, content: &[u8]) -> Option<Vec<u8>> {
    let (header, rest) = LayoutVerified::<_, Header>::new_from_prefix(content)?;

    let magic = header.magic.get().to_be_bytes();
    if &magic != b"BLTE" {
        panic!("invalid magic for BLTE: {}", magic.escape_ascii());
    }

    // Initialzed before the if to allow for borrowing it, but defer initialization
    let mut dummy_chunk = [ChunkInfo {
        compressed_size: u32_be::ZERO,
        decompressed_size: u32_be::ZERO,
        checksum: [0; 16],
    }];

    let (chunk_infos, rest) = if header.header_size.get() > 0 {
        let (sub_header, rest) = LayoutVerified::<_, SubHeader>::new_from_prefix(rest)?;
        let (chunk_infos, rest) = LayoutVerified::<_, [ChunkInfo]>::new_slice_from_prefix(
            rest,
            sub_header.chunk_count.get() as usize,
        )?;
        (chunk_infos.into_slice(), rest)
    } else {
        assert_eq!(content.len() - 8, rest.len());
        dummy_chunk[0].compressed_size = u32_be::new(rest.len() as u32);
        dummy_chunk[0].checksum = compute_md5(rest);
        (dummy_chunk.as_slice(), rest)
    };

    let expected_size = chunk_infos
        .iter()
        .map(|c| c.decompressed_size.get() as usize)
        .sum();
    let mut res = Vec::with_capacity(expected_size);

    let r = &mut ByteReader::new(rest);
    for (index, chunk_info) in chunk_infos.iter().enumerate() {
        let data = r.take(chunk_info.compressed_size.get() as usize)?;
        let hash = compute_md5(data);
        assert_eq!(
            hash, chunk_info.checksum,
            "blte chunk did not match checksum"
        );
        handle_data_block(data, tact_keys, index, chunk_info, &mut res)?;
    }

    Some(res)
}

#[inline(always)]
pub fn compute_md5(data: &[u8]) -> [u8; 16] {
    use md5::{Digest, Md5};
    let mut hasher = Md5::new();
    hasher.update(data);
    let res = hasher.finalize();
    res.into()
}

fn handle_data_block(
    data: &[u8],
    tact_keys: &TactKeys,
    index: usize,
    chunk_info: &ChunkInfo,
    out: &mut Vec<u8>,
) -> Option<()> {
    let (encoding_mode, data) = data.split_first()?;
    match encoding_mode {
        b'N' => out.extend_from_slice(data),
        b'Z' => handle_deflate_block(data, chunk_info, out),
        b'F' => todo!("recursive blte block"),
        b'E' => handle_encrypted_block(data, tact_keys, index, chunk_info, out)?,
        encoding_mode => {
            panic!("Unknown encoding mode: {}", encoding_mode.escape_ascii())
        }
    }
    Some(())
}

fn handle_deflate_block(data: &[u8], chunk_info: &ChunkInfo, out: &mut Vec<u8>) {
    let decompressed_size = chunk_info.decompressed_size.get() as usize;
    if decompressed_size > 0 {
        // If we know the output size, use libdeflate
        zlib_decompress(data, out, decompressed_size).expect("error deflating blte Z chunk");
    } else {
        // Otherwise use flate2 which uses an internal buffer
        let mut decoder = ZlibDecoder::new(data);

        let _num_decompressed = decoder
            .read_to_end(out)
            .expect("error deflating blte Z chunk");
    }
}

fn zlib_decompress(
    in_buf: &[u8],
    out_buf: &mut Vec<u8>,
    decompressed_size: usize,
) -> Option<usize> {
    out_buf.reserve(decompressed_size);
    let out = out_buf.spare_capacity_mut();

    let mut out_nbytes = 0;
    let in_ptr = in_buf.as_ptr() as *const std::ffi::c_void;
    let out_ptr = out.as_mut_ptr() as *mut std::ffi::c_void;

    let ret = unsafe {
        let d = libdeflate_sys::libdeflate_alloc_decompressor();
        let ret = libdeflate_zlib_decompress(
            d,
            in_ptr,
            in_buf.len(),
            out_ptr,
            out.len(),
            &mut out_nbytes,
        );
        libdeflate_free_decompressor(d);
        ret
    };

    match ret {
        libdeflate_sys::libdeflate_result_LIBDEFLATE_SUCCESS => {
            // SAFETY: libdeflate has written exactly out_nbytes into the spare capacity of out_buf
            unsafe {
                out_buf.set_len(out_buf.len() + out_nbytes);
            }

            if out_nbytes != decompressed_size {
                eprintln!("decompressed unexpected number of bytes in blte Z chunk");
                None
            } else {
                Some(out_nbytes)
            }
        }
        libdeflate_sys::libdeflate_result_LIBDEFLATE_BAD_DATA => {
            eprintln!("bad data in blte Z chunk");
            None
        }
        libdeflate_sys::libdeflate_result_LIBDEFLATE_INSUFFICIENT_SPACE => {
            eprintln!("insufficient space in output buffer for blte Z chunk");
            None
        }
        _ => {
            panic!("libdeflate_deflate_decompress returned an unknown error type: this is an internal bug that **must** be fixed");
        }
    }
}

#[derive(FromBytes)]
struct EncryptHeader {
    key_name_length: u8,
    key_name: [u8; 8],
    iv_length: u8,
    iv: [u8; 4],
    type_: u8,
}

fn handle_encrypted_block(
    data: &[u8],
    tact_keys: &TactKeys,
    index: usize,
    chunk_info: &ChunkInfo,
    out: &mut Vec<u8>,
) -> Option<()> {
    let (header, data) = LayoutVerified::<_, EncryptHeader>::new_from_prefix(data)?;
    assert_eq!(8, header.key_name_length);
    assert_eq!(4, header.iv_length);

    if let Some(&key) = tact_keys.get_key(&header.key_name) {
        let mut buf = data.to_vec();
        match header.type_ {
            b'S' => {
                let mut full_iv = [0; 8];
                full_iv[0..4].copy_from_slice(&header.iv);

                #[allow(clippy::needless_range_loop)]
                for i in 0..4 {
                    full_iv[i] ^= ((index >> (i * 8)) & 0xff) as u8;
                }

                salsa_decrypt(key, full_iv, &mut buf);
            }
            _ => panic!("Unhandled encryption mode: {}", header.type_.escape_ascii()),
        }

        match buf[0] {
            b'N' | b'Z' | b'F' | b'E' => {
                let chunk_info = ChunkInfo {
                    compressed_size: u32_be::new(buf.len() as u32),
                    ..chunk_info.clone()
                };
                handle_data_block(&buf, tact_keys, index, &chunk_info, out)?;
            }
            _ => {
                // println!(
                //     "index: {}, key_name: {:02X?}, iv: {:02X?}, type: {}",
                //     index,
                //     header.key_name,
                //     header.iv,
                //     header.type_.escape_ascii()
                // );
                // eprintln!("decrypted block seemingly corrupt, filling with dummy data");
                out.extend((0..chunk_info.decompressed_size.get()).map(|_| 0u8));
            }
        }
    } else {
        // println!(
        //     "index: {}, key_name: {:02X?}, iv: {:02X?}, type: {}",
        //     index,
        //     header.key_name,
        //     header.iv,
        //     header.type_.escape_ascii()
        // );
        // println!(
        //     "Encryption key name {:02X?} not found, filling with dummy data",
        //     header.key_name
        // );
        out.extend((0..chunk_info.decompressed_size.get()).map(|_| 0u8));
    }
    Some(())
}

fn salsa_decrypt(key: [u8; 16], iv: [u8; 8], buf: &mut [u8]) {
    // println!("key: {:02x?}", key);
    // println!("iv: {:02x?}", iv);
    let key = rust_salsa20::Key::Key16(key);
    let mut cipher = rust_salsa20::Salsa20::new(key, iv, 0);
    // println!("enc: {:02x?}", buf);
    cipher.encrypt(buf);
    // println!("dec: {:02x?}", buf);
}

#[derive(Debug)]
pub struct Blte {
    pub data: Vec<u8>,
}
