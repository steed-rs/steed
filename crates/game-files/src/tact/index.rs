use binstream::{u32_le, ByteParse, ByteReader, BE};
use std::collections::HashMap;

use crate::casc::idx::Key;

#[derive(Debug)]
pub struct Index {
    pub entries: HashMap<Key, Entry>,
}

pub fn parse_index(content: &[u8]) -> Option<Index> {
    let footer = parse_footer(&content[content.len() - 28..])?;

    let block_size = 1024 * footer.block_size_kb as usize;
    let record_size = footer.key_size_in_bytes as usize
        + footer.size_bytes as usize
        + footer.offset_bytes as usize;

    let mut entries = HashMap::new();
    for record in content
        .chunks_exact(block_size)
        .flat_map(|b| b.chunks_exact(record_size))
        .take(footer.num_elements.get() as usize)
    {
        let r = &mut ByteReader::new(record);

        let key = Key::parse(r)?;
        let size = r.uint::<BE>(footer.size_bytes as usize)?;
        let offset = r.uint::<BE>(footer.offset_bytes as usize)?;

        assert_ne!(key, Key::ZERO);
        assert!(!entries.contains_key(&key));

        let entry = Entry { size, offset };
        entries.insert(key, entry);
    }
    assert_eq!(footer.num_elements.get() as usize, entries.len());

    Some(Index { entries })
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub size: u64,
    pub offset: u64,
}

#[derive(ByteParse, Debug)]
pub struct Footer {
    pub _toc_hash: [u8; 8],
    pub version: u8,
    pub unk0: u8,
    pub unk1: u8,
    pub block_size_kb: u8,
    pub offset_bytes: u8,
    pub size_bytes: u8,
    pub key_size_in_bytes: u8,
    pub checksum_size: u8,
    pub num_elements: u32_le,
}

fn parse_footer(content: &[u8]) -> Option<Footer> {
    let r = &mut ByteReader::new(content);

    let mut res = Footer::parse(r)?;
    assert_eq!(res.version, 1);
    assert_eq!(res.unk0, 0);
    assert_eq!(res.unk1, 0);
    assert_eq!(res.checksum_size, 8);
    assert_eq!(res.key_size_in_bytes, 16);

    // TODO: avoid the extra conversions
    if res.num_elements.get() & 0xff000000 != 0 {
        // num_elements is BE in old versions, we've likely hit that.
        // swap the bytes to reintrepret.
        res.num_elements.set(res.num_elements.get().swap_bytes());
    }

    let _footer_checksum = r.take(res.checksum_size as usize)?;
    Some(res)
}
