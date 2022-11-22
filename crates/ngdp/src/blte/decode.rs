use std::io::Cursor;
use std::io::Read;

use binrw::BinRead;
use flate2::bufread::ZlibDecoder;
use libdeflate_sys::{libdeflate_free_decompressor, libdeflate_zlib_decompress};

use crate::tact::keys::TactKeys;

// TODO: Rewrite as a std::io::Read impl?
pub fn decode_blte(tact_keys: &TactKeys, content: &[u8]) -> Result<Vec<u8>, anyhow::Error> {
    let mut r = Cursor::new(content);
    let res = repr::BLTEHeader::read(&mut r)?;

    // Initialzed before the if to allow for borrowing it, but defer initialization
    let mut dummy_chunk = [repr::ChunkInfo {
        compressed_size: 0,
        decompressed_size: 0,
        checksum: [0; 16],
    }];

    let chunk_infos = if !res.chunks.is_empty() {
        res.chunks.as_slice()
    } else {
        let rest = &content[r.position() as usize..];
        assert_eq!(content.len() - 8, rest.len());
        dummy_chunk[0].compressed_size = rest.len() as u32;
        dummy_chunk[0].checksum = compute_md5(rest);
        dummy_chunk.as_slice()
    };

    let expected_size = chunk_infos
        .iter()
        .map(|c| c.decompressed_size as usize)
        .sum();
    let mut res = Vec::with_capacity(expected_size);

    for (index, chunk_info) in chunk_infos.iter().enumerate() {
        let mut data = vec![0; chunk_info.compressed_size as usize];
        r.read_exact(&mut data)?;
        let hash = compute_md5(&data);
        assert_eq!(
            hash, chunk_info.checksum,
            "blte chunk did not match checksum"
        );
        handle_data_block(&data, tact_keys, index as u32, chunk_info, &mut res)?;
    }

    Ok(res)
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
    index: u32,
    chunk_info: &repr::ChunkInfo,
    out: &mut Vec<u8>,
) -> Result<(), anyhow::Error> {
    let (encoding_mode, data) = data
        .split_first()
        .ok_or_else(|| anyhow::anyhow!("blte: Expected at least one byte for block"))?;
    match encoding_mode {
        b'N' => out.extend_from_slice(data),
        b'Z' => handle_deflate_block(data, chunk_info, out),
        b'F' => todo!("recursive blte block"),
        b'E' => handle_encrypted_block(data, tact_keys, index, chunk_info, out)?,
        encoding_mode => {
            panic!("Unknown encoding mode: {}", encoding_mode.escape_ascii())
        }
    }
    Ok(())
}

pub fn dbg_zlib_wrapper(data: &[u8]) {
    let cm = data[0] & 0xf;
    let cinfo = data[0] >> 4;

    let fcheck = data[1] & 0x1f;
    let dict = (data[1] >> 5) & 1;
    let flevel = data[1] >> 6;

    eprintln!(
        "zlib: {:02x?} - cm: {}, cinfo: {} (window {}), fcheck: {}, dict: {}, flevel: {} ({})",
        &data[..2],
        cm,
        cinfo,
        1 << (cinfo + 8),
        fcheck,
        dict,
        flevel,
        match flevel {
            0 => "fastest",
            1 => "fast",
            2 => "default",
            3 => "slowest",
            _ => "unknown",
        }
    );
}

fn handle_deflate_block(data: &[u8], chunk_info: &repr::ChunkInfo, out: &mut Vec<u8>) {
    // dbg_zlib_wrapper(&data[..2]);

    let decompressed_size = chunk_info.decompressed_size as usize;
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
) -> Result<usize, anyhow::Error> {
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
                Err(anyhow::anyhow!(
                    "decompressed unexpected number of bytes in blte Z chunk"
                ))
            } else {
                Ok(out_nbytes)
            }
        }
        libdeflate_sys::libdeflate_result_LIBDEFLATE_BAD_DATA => {
            Err(anyhow::anyhow!("bad data in blte Z chunk"))
        }
        libdeflate_sys::libdeflate_result_LIBDEFLATE_INSUFFICIENT_SPACE => Err(anyhow::anyhow!(
            "insufficient space in output buffer for blte Z chunk"
        )),
        _ => {
            panic!("libdeflate_deflate_decompress returned an unknown error type: this is an internal bug that **must** be fixed");
        }
    }
}

fn handle_encrypted_block(
    data: &[u8],
    tact_keys: &TactKeys,
    index: u32,
    chunk_info: &repr::ChunkInfo,
    out: &mut Vec<u8>,
) -> Result<(), anyhow::Error> {
    let mut r = Cursor::new(data);
    let header = repr::EncryptHeader::read(&mut r)?;
    let data = &data[r.position() as usize..];

    assert_eq!(8, header.key_name_length);
    assert_eq!(4, header.iv_length);

    if let Some(&key) = tact_keys.get_key(&header.key_name) {
        let mut buf = data.to_vec();
        match header.type_ {
            b'S' => {
                let mut full_iv = [0; 8];
                full_iv[0..4].copy_from_slice(&header.iv);

                let index = index.to_le_bytes();
                for i in 0..4 {
                    full_iv[i] ^= index[i];
                }

                salsa_crypt(key, full_iv, &mut buf);
            }
            _ => anyhow::bail!("Unhandled encryption mode: {}", header.type_.escape_ascii()),
        }

        match buf[0] {
            b'N' | b'Z' | b'F' | b'E' => {
                let chunk_info = repr::ChunkInfo {
                    compressed_size: buf.len() as u32,
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
                out.extend((0..chunk_info.decompressed_size).map(|_| 0u8));
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
        out.extend((0..chunk_info.decompressed_size).map(|_| 0u8));
    }
    Ok(())
}

pub(super) fn salsa_crypt(key: [u8; 16], iv: [u8; 8], buf: &mut [u8]) {
    // println!("key: {:02x?}", key);
    // println!("iv: {:02x?}", iv);
    let key = rust_salsa20::Key::Key16(key);
    let mut cipher = rust_salsa20::Salsa20::new(key, iv, 0);
    // println!("enc: {:02x?}", buf);
    cipher.encrypt(buf);
    // println!("dec: {:02x?}", buf);
}

pub(super) mod repr {
    use binrw::{BinRead, BinWrite};

    use crate::binrw_ext::u24;

    #[derive(BinRead, BinWrite, Debug)]
    #[brw(big, magic = b"BLTE")]
    pub struct BLTEHeader {
        pub header_size: u32,

        #[br(if(header_size > 0))]
        #[bw(if(*header_size > 0))]
        pub flags: Option<u8>,

        #[br(if(header_size > 0))]
        #[bw(if(*header_size > 0))]
        pub chunk_count: Option<u24>,

        #[br(count = chunk_count.unwrap_or(u24::ZERO).get())]
        pub chunks: Vec<ChunkInfo>,
    }

    #[derive(BinRead, BinWrite, Clone)]
    #[brw(big)]
    pub struct ChunkInfo {
        pub compressed_size: u32,
        pub decompressed_size: u32,
        pub checksum: [u8; 16],
    }

    impl std::fmt::Debug for ChunkInfo {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("ChunkInfo")
                .field("compressed_size", &self.compressed_size)
                .field("decompressed_size", &self.decompressed_size)
                .field("checksum", &format_args!("{:02X?}", &self.checksum))
                .finish()
        }
    }

    #[derive(BinRead, BinWrite)]
    #[brw(big)]
    pub struct EncryptHeader {
        pub key_name_length: u8,
        pub key_name: [u8; 8],
        pub iv_length: u8,
        pub iv: [u8; 4],
        pub type_: u8,
    }
}
