use std::io::{Cursor, Seek, Write};

use binrw::BinWrite;
use flate2::{Compress, Compression, FlushCompress, Status};
use thiserror::Error;

use crate::binrw_ext::u24;
use crate::tact::keys::TactKeys;

use super::espec::{self, Block, ESpec, Encrypted, Zip};
use super::repr;
use super::{compute_md5, salsa_crypt};

pub fn encode_blte(keys: &TactKeys, espec: &ESpec, input: &[u8]) -> Result<Vec<u8>, EncodeError> {
    let mut buf = vec![];
    encode_blte_into(keys, espec, input, &mut Cursor::new(&mut buf))?;
    Ok(buf)
}

pub fn encode_blte_into(
    keys: &TactKeys,
    espec: &ESpec,
    input: &[u8],
    w: &mut (impl Write + Seek),
) -> Result<(), EncodeError> {
    let mut header = repr::BLTEHeader {
        header_size: 0,
        flags: Some(0xf),
        chunk_count: None,
        chunks: vec![],
    };

    let mut buf = vec![];
    process_top(keys, espec, input, &mut buf, &mut header)?;

    header.header_size = if header.chunks.is_empty() {
        0
    } else {
        4 + 1 + 3 + 4 + 24 * header.chunks.len() as u32
    };
    header.chunk_count = Some(u24::new(header.chunks.len() as u32));

    // dbg!(&header);

    header.write(w)?;
    w.write_all(&buf)?;

    Ok(())
}

fn process_top(
    keys: &TactKeys,
    espec: &ESpec,
    input: &[u8],
    buf: &mut Vec<u8>,
    header: &mut repr::BLTEHeader,
) -> Result<(), EncodeError> {
    match espec {
        ESpec::Raw | ESpec::Zip(_) | ESpec::Encrypted(_) => {
            process_inner(keys, espec, input, buf, 0)?;
        }
        ESpec::Blocks(v) => {
            let input = v.blocks.iter().try_fold(input, |input, block| {
                process_block(keys, block, input, buf, header)
            })?;

            let input = process_block(keys, &v.final_, input, buf, header)?;

            if !input.is_empty() {
                return Err(EncodeError::LeftoverData(input.len()));
            }
        }
    }
    Ok(())
}

fn process_block<'a>(
    keys: &TactKeys,
    block: &Block,
    input: &'a [u8],
    buf: &mut Vec<u8>,
    header: &mut repr::BLTEHeader,
) -> Result<&'a [u8], EncodeError> {
    let mut process_chunk = |input: &[u8]| -> Result<(), EncodeError> {
        let start_pos = buf.len();
        process_inner(keys, &block.inner, input, buf, header.chunks.len())?;
        let end_pos = buf.len();

        let checksum = compute_md5(&buf[start_pos..end_pos]);

        header.chunks.push(repr::ChunkInfo {
            compressed_size: (end_pos - start_pos) as u32,
            decompressed_size: input.len() as u32,
            checksum,
        });
        Ok(())
    };

    let mut rest = input;
    match block.size {
        espec::BlockSize::Chunked { size, count } => {
            let mut num_chunks = 0;
            loop {
                if size > rest.len() as u64 {
                    return Err(EncodeError::ChunkUnderflow(size, rest.len()));
                }

                let input;
                (input, rest) = rest.split_at(size as usize);

                process_chunk(input)?;

                num_chunks += 1;
                if num_chunks >= count {
                    break;
                }
            }
        }
        espec::BlockSize::ChunkedGreedy { size } => loop {
            if rest.is_empty() {
                break;
            }

            let input;
            (input, rest) = rest.split_at((size as usize).min(rest.len()));

            process_chunk(input)?;
        },
        espec::BlockSize::Greedy => {
            process_chunk(input)?;
            rest = &input[input.len()..];
        }
    }

    Ok(rest)
}

fn process_inner(
    keys: &TactKeys,
    espec: &ESpec,
    input: &[u8],
    buf: &mut Vec<u8>,
    block_index: usize,
) -> Result<(), EncodeError> {
    match espec {
        ESpec::Raw => {
            buf.push(b'N');
            buf.extend_from_slice(input);
        }
        ESpec::Zip(v) => process_zip(v, input, buf),
        ESpec::Encrypted(v) => process_encrypt(keys, v, input, buf, block_index)?,
        ESpec::Blocks(_) => {
            panic!("Nested BLTE blocks not supported")
        }
    }
    Ok(())
}

fn process_zip(v: &Zip, input: &[u8], buf: &mut Vec<u8>) {
    // dbg!(v);
    buf.push(b'Z');

    let level = Compression::new(v.level as u32);
    let window_bits = match v.bits {
        espec::ZipBits::Bits(bits) => bits,
        espec::ZipBits::MPQ => match input.len() {
            // v if v <= 0x100 => 8,
            v if v <= 0x200 => 9,
            v if v <= 0x400 => 10,
            v if v <= 0x800 => 11,
            v if v <= 0x1000 => 12,
            v if v <= 0x2000 => 13,
            v if v <= 0x4000 => 14,
            _ => 15,
        },
    };

    let mut compress = Compress::new_with_window_bits(level, true, window_bits);

    // FIXME: Upstream a constructor to create ZlibEncoder with provided Compress
    // FIXME: Better allocation strategy
    buf.reserve(input.len());
    loop {
        let total_in = compress.total_in();
        let status = compress
            .compress_vec(
                &input[total_in as usize as usize..],
                buf,
                FlushCompress::Finish,
            )
            .expect("compress operation failed unexpectedly");

        match status {
            Status::Ok => {
                buf.reserve(128);
            }
            Status::StreamEnd => break,
            Status::BufError => panic!("compress unexpectedly returned BufError"),
        }
    }

    // let pre = buf.len();

    // assert_ne!(status, Status::BufError);
    // while status != Status::StreamEnd {
    //     status = compress
    //         .compress(&[], buf, FlushCompress::Finish)
    //         .expect("compress operation failed unexpectedly");
    // }

    // dbg_zlib_wrapper(&buf[pre..pre + 2]);
}

fn process_encrypt(
    keys: &TactKeys,
    v: &Encrypted,
    input: &[u8],
    buf: &mut Vec<u8>,
    block_index: usize,
) -> Result<(), EncodeError> {
    let key = keys
        .get_key(&v.key)
        .copied()
        .ok_or(EncodeError::MissingEncryptionKey(v.key))?;

    buf.push(b'E');
    let encrypt_header = repr::EncryptHeader {
        key_name_length: 8,
        key_name: v.key,
        iv_length: 4,
        iv: v.iv,
        type_: b'S',
    };

    let mut header_buf = [0u8; 15];
    encrypt_header.write(&mut Cursor::new(header_buf.as_mut_slice()))?;
    buf.extend_from_slice(&header_buf);

    let mut full_iv = [0; 8];
    full_iv[0..4].copy_from_slice(&v.iv);

    let index = block_index.to_le_bytes();
    for i in 0..4 {
        full_iv[i] ^= index[i];
    }

    let mut inner_buf = vec![];
    process_inner(keys, &v.inner, input, &mut inner_buf, block_index)?;

    salsa_crypt(key, full_iv, &mut inner_buf);

    buf.extend_from_slice(&inner_buf);

    Ok(())
}

#[derive(Error, Debug)]
pub enum EncodeError {
    #[error("missing encryption key: {0:02X?}")]
    MissingEncryptionKey([u8; 8]),
    #[error("missing data to fill chunk: expected {0} bytes, found {1} bytes")]
    ChunkUnderflow(u64, usize),
    #[error("leftover data after main block: {0} bytes")]
    LeftoverData(usize),
    #[error("error writing to supplied writer: {0}")]
    IoError(#[from] std::io::Error),
    #[error("error writing structure to underlying writer: {0}")]
    BinError(#[from] binrw::Error),
}
