use std::{collections::HashMap, io::Cursor};

use binrw::BinRead;
use byteorder::{ByteOrder, BE};

use super::EncodingKey;

#[derive(Debug)]
pub struct Index {
    pub entries: HashMap<EncodingKey, Entry>,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub size: u64,
    pub offset: u64,
}

pub fn parse_index(content: &[u8]) -> Result<Index, anyhow::Error> {
    let res = repr::Index::read(&mut Cursor::new(content))?;
    assert_eq!(res.footer.key_size_in_bytes, 16);

    let mut entries = HashMap::new();
    for block in res.blocks {
        'block: for entry in block.entries.0 {
            let key = EncodingKey::from_slice(&entry.ekey);
            if key == EncodingKey::ZERO {
                // We've reached zero padding, block is done
                break 'block;
            }

            let size = BE::read_uint(&entry.size, res.footer.size_bytes as usize);
            let offset = BE::read_uint(&entry.offset, res.footer.offset_bytes as usize);

            assert!(!entries.contains_key(&key));

            let entry = Entry { size, offset };
            entries.insert(key, entry);
        }
    }

    assert_eq!(res.footer.num_elements as usize, entries.len());

    Ok(Index { entries })
}

mod repr {
    use std::io::SeekFrom;

    use binrw::BinRead;

    use crate::binrw_ext::Block;

    #[derive(BinRead)]
    #[br(little)]
    pub struct Index {
        // TODO: Match agent behaviour, trying with multiple toc_hash_sizes
        #[br(seek_before = SeekFrom::End(-28))]
        pub footer: Footer,

        #[br(seek_before = SeekFrom::Start(0), args {
            count: num_blocks(&footer),
            inner: (footer.block_size_kb, footer.key_size_in_bytes, footer.size_bytes, footer.offset_bytes),
        })]
        pub blocks: Vec<IndexBlock>,

        pub toc: TableOfContents,
    }

    #[derive(BinRead, Debug)]
    #[br(little)]
    pub struct Footer {
        pub toc_hash: [u8; 8],
        pub version: u8,
        pub unk0: u8,
        pub unk1: u8,
        pub block_size_kb: u8,
        pub offset_bytes: u8,
        pub size_bytes: u8,
        pub key_size_in_bytes: u8,
        pub checksum_size: u8,
        pub num_elements: u32,
        pub footer_checksum: [u8; 8],
    }

    #[derive(BinRead)]
    #[br(import(block_size_kb: u8, key_size_in_bytes: u8, size_bytes: u8, offset_bytes: u8))]
    pub struct IndexBlock {
        #[br(args {
            count: block_size_kb as usize * 1024,
            inner: (key_size_in_bytes, size_bytes, offset_bytes),
        })]
        pub entries: Block<IndexEntry>,
    }

    #[derive(BinRead)]
    #[br(import(key_size_in_bytes: u8, size_bytes: u8, offset_bytes: u8))]
    pub struct IndexEntry {
        #[br(count = key_size_in_bytes)]
        pub ekey: Vec<u8>,

        #[br(count = size_bytes)]
        pub size: Vec<u8>,

        #[br(count = offset_bytes)]
        pub offset: Vec<u8>,
    }

    impl std::fmt::Debug for IndexEntry {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("IndexEntry")
                .field("ekey", &format_args!("{:x?}", self.ekey))
                .field("size", &format_args!("{:x?}", self.size))
                .field("offset", &format_args!("{:x?}", self.offset))
                .finish()
        }
    }

    #[derive(BinRead)]
    #[br(import(num_blocks: usize, key_size_in_bytes: u8, checksum_size: u8))]
    pub struct TableOfContents {
        #[br(args { count: num_blocks, inner: (key_size_in_bytes,) })]
        pub entries: Vec<TOCEntry>,
        #[br(args { count: num_blocks, inner: (checksum_size,) })]
        pub blocks_hash: Vec<TOCBlockHash>,
    }

    #[derive(BinRead)]
    #[br(import(key_size_in_bytes: u8))]
    pub struct TOCEntry {
        #[br(count = key_size_in_bytes)]
        pub last_ekey: Vec<u8>,
    }

    #[derive(BinRead)]
    #[br(import(checksum_size: u8))]
    pub struct TOCBlockHash {
        #[br(count = checksum_size)]
        pub lower_part_of_md5_of_block: Vec<u8>,
    }

    const fn num_blocks(footer: &Footer) -> usize {
        let block_size = (footer.block_size_kb as usize) * 1024;
        let elements_per_block = block_size
            / (footer.key_size_in_bytes + footer.size_bytes + footer.offset_bytes) as usize;
        div_ceil(footer.num_elements as usize, elements_per_block)
    }

    const fn div_ceil(lhs: usize, rhs: usize) -> usize {
        // TODO: use usize::div_ceil once stable
        let d = lhs / rhs;
        let r = lhs % rhs;
        if r > 0 && rhs > 0 {
            d + 1
        } else {
            d
        }
    }
}
