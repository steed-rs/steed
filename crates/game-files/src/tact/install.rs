use std::collections::HashSet;

use super::keys::TactKeys;
use crate::casc::{blte::decode_blte, idx::Key};
use binstream::{u16_be, u32_be, ByteParse, ByteReader, StringZero};
use bitvec::{prelude::Msb0, vec::BitVec};

#[derive(Debug)]
pub struct InstallManifest {
    pub header: Header,
    pub tags: Vec<Tag>,
    pub files: Vec<File>,
}

impl InstallManifest {
    pub fn files_with_tags<'a, 'b>(
        &'a self,
        tags: &HashSet<String>,
    ) -> impl Iterator<Item = &'a File> {
        let mut files = BitVec::from_iter(std::iter::repeat(true).take(self.files.len()));

        let categories = self.tags.iter().map(|t| t.type_).max().unwrap_or(0);
        for category in 0..categories {
            for tag in self.tags.iter().filter(|t| t.type_ == category) {
                if tags.contains(&tag.name) {
                    files &= &tag.files;
                }
            }
        }

        files
            .into_iter()
            .enumerate()
            .filter_map(|(idx, val)| val.then_some(idx))
            .map(|idx| &self.files[idx])
    }
}

#[derive(ByteParse, Debug)]
pub struct Header {
    pub signature: [u8; 2],
    pub version: u8,
    pub hash_size: u8,
    pub num_tags: u16_be,
    pub num_entries: u32_be,
}

pub fn parse_install_manifest(tact_keys: &TactKeys, content: &[u8]) -> Option<InstallManifest> {
    let content = decode_blte(tact_keys, content)?;
    let r = &mut ByteReader::new(content.as_slice());

    let header = Header::parse(r)?;
    assert_eq!(b"IN", &header.signature);
    assert_eq!(16, header.hash_size);

    let tags = r.repeat_fn(
        |r| parse_tag(r, header.num_entries.get() as usize),
        header.num_tags.get() as usize,
    )?;
    let files = r.repeat::<File>(header.num_entries.get() as usize)?;

    Some(InstallManifest {
        header,
        tags,
        files,
    })
}

pub struct Tag {
    pub name: String,
    pub type_: u16,
    pub files: BitVec<u8, Msb0>,
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
    let files = r.take(div_ceil(num_entries, u8::BITS as usize))?.to_vec();
    let mut files = BitVec::from_vec(files);
    if files.len() > num_entries {
        files.drain(num_entries..);
    }
    Some(Tag { name, type_, files })
}

#[derive(ByteParse, Debug)]
pub struct File {
    pub file_name: StringZero,
    pub key: Key,
    pub size: u32_be,
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
