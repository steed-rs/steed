use std::collections::HashSet;

use crate::casc::{blte::decode_blte, idx::Key};
use binstream::{u16_be, u32_be, u40_be, ByteParse, ByteReader};
use bitvec::{prelude::Msb0, vec::BitVec};

use super::keys::TactKeys;

#[derive(Debug)]
pub struct DownloadManifest {
    pub signature: [u8; 2],
    pub version: u8,
    pub hash_size: u8,
    pub has_checksum_in_entry: u8,
    pub number_of_flag_bytes: u8,
    pub base_priority: u8,
    pub unk: Option<[u8; 3]>,
    pub entries: Vec<Entry>,
    pub tags: Vec<Tag>,
}

impl DownloadManifest {
    pub fn entries_with_tags<'a, 'b>(
        &'a self,
        tags: &HashSet<String>,
    ) -> impl Iterator<Item = &'a Entry> {
        let mut entries = BitVec::from_iter(std::iter::repeat(true).take(self.entries.len()));

        let categories = self.tags.iter().map(|t| t.type_).max().unwrap_or(0);
        for category in 0..categories {
            for tag in self.tags.iter().filter(|t| t.type_ == category) {
                if tags.contains(&tag.name) {
                    entries &= &tag.entries;
                }
            }
        }

        entries
            .into_iter()
            .enumerate()
            .filter_map(|(idx, val)| val.then_some(idx))
            .map(|idx| &self.entries[idx])
    }
}

pub fn parse_download_manifest(tact_keys: &TactKeys, content: &[u8]) -> Option<DownloadManifest> {
    let content = decode_blte(tact_keys, content)?;
    let r = &mut ByteReader::new(content.as_slice());

    let signature = r.take_n::<2>()?;
    let version: u8 = r.parse()?;
    let hash_size: u8 = r.parse()?;
    let has_checksum_in_entry: u8 = r.parse()?;
    let num_entries = r.parse::<u32_be>()?.get();
    let num_tags = r.parse::<u16_be>()?.get();
    let number_of_flag_bytes = r.cond::<u8>(version >= 2)?.unwrap_or(0);
    let base_priority = r.cond::<u8>(version >= 3)?.unwrap_or(0);
    let unk = r.cond::<[u8; 3]>(version >= 3)?;

    assert_eq!(b"DL", &signature);
    assert_eq!(16, hash_size);

    let entries = r.repeat_fn(
        |r| parse_entry(r, has_checksum_in_entry, number_of_flag_bytes),
        num_entries as usize,
    )?;
    let tags = r.repeat_fn(|r| parse_tag(r, num_entries as usize), num_tags as usize)?;

    Some(DownloadManifest {
        signature,
        version,
        hash_size,
        has_checksum_in_entry,
        number_of_flag_bytes,
        base_priority,
        unk,
        entries,
        tags,
    })
}

#[derive(Debug)]
pub struct Entry {
    pub key: Key,
    pub file_size: u64,
    pub download_priority: u8,
    pub checksum: Option<u32_be>,
    pub flags: Vec<u8>,
}

pub fn parse_entry(
    r: &mut ByteReader,
    has_checksum_in_entry: u8,
    number_of_flag_bytes: u8,
) -> Option<Entry> {
    let key = Key::parse(r)?;
    let file_size = r.parse::<u40_be>()?.get();
    let download_priority: u8 = r.parse()?;
    let checksum = r.cond::<u32_be>(has_checksum_in_entry != 0)?;
    let flags = r.take(number_of_flag_bytes as usize)?.to_vec();
    Some(Entry {
        key,
        file_size,
        download_priority,
        checksum,
        flags,
    })
}

pub struct Tag {
    pub name: String,
    pub type_: u16,
    pub entries: BitVec<u8, Msb0>,
}

impl std::fmt::Debug for Tag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tag")
            .field("name", &self.name)
            .field("type_", &self.type_)
            .finish()
    }
}

pub fn parse_tag(r: &mut ByteReader, num_entries: usize) -> Option<Tag> {
    let name = r.string_zero()?.to_string();
    let type_ = r.parse::<u16_be>()?.get();
    let entries = r.take(div_ceil(num_entries, u8::BITS as usize))?.to_vec();
    let mut entries = BitVec::from_vec(entries);
    if entries.len() > num_entries {
        entries.drain(num_entries..);
    }
    Some(Tag {
        name,
        type_,
        entries,
    })
}

pub const fn div_ceil(lhs: usize, rhs: usize) -> usize {
    // TODO: use usize::div_ceil once stable
    let d = lhs / rhs;
    let r = lhs % rhs;
    if r > 0 && rhs > 0 {
        d + 1
    } else {
        d
    }
}
