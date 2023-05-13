use std::{collections::HashSet, io::Cursor};

use binrw::BinRead;
use bitvec::{prelude::Msb0, vec::BitVec};

use super::{keys::TactKeys, EncodingKey};
use crate::{blte::decode_blte, util::hexdump};

#[derive(Debug)]
pub struct DownloadManifest {
    pub base_priority: u8,
    pub entries: Vec<Entry>,
    pub tags: Vec<Tag>,
}

impl DownloadManifest {
    pub fn entries_with_tags<'a>(
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

pub fn parse_download_manifest(
    tact_keys: &TactKeys,
    content: &[u8],
) -> Result<DownloadManifest, anyhow::Error> {
    let content = decode_blte(tact_keys, content)?;
    hexdump(&content, 0, 256);

    let res = repr::DownloadManifest::read(&mut Cursor::new(content))?;
    assert_eq!(16, res.hash_size);

    let entry_count = res.entry_count as usize;
    Ok(DownloadManifest {
        base_priority: res.base_priority.unwrap_or(0),
        entries: res
            .entries
            .into_iter()
            .map(|e| Entry {
                key: EncodingKey::from_slice(&e.key),
                file_size: e.file_size.get(),
                download_priority: e.download_priority,
                checksum: e.checksum,
                flags: e.flags,
            })
            .collect(),
        tags: res
            .tags
            .into_iter()
            .map(|t| Tag {
                name: t.name.to_string(),
                type_: t.type_,
                entries: {
                    let mut entries = BitVec::from_vec(t.entries);
                    if entries.len() > entry_count {
                        entries.drain(entry_count..);
                    }
                    entries
                },
            })
            .collect(),
    })
}

#[derive(Debug)]
pub struct Entry {
    pub key: EncodingKey,
    pub file_size: u64,
    pub download_priority: u8,
    pub checksum: Option<u32>,
    pub flags: Vec<u8>,
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

mod repr {
    use binrw::{BinRead, NullString};

    use crate::binrw_ext::u40;

    #[derive(BinRead)]
    #[br(big, magic = b"DL")]
    pub struct DownloadManifest {
        pub version: u8,
        pub hash_size: u8,
        pub has_checksum_in_entry: u8,
        pub entry_count: u32,
        pub tag_count: u16,

        #[br(if(version >= 2))]
        pub number_of_flag_bytes: Option<u8>,

        #[br(if(version >= 3))]
        pub base_priority: Option<u8>,

        #[br(if(version >= 3))]
        pub _pad: Option<[u8; 3]>,

        #[br(args{
            count: entry_count as usize,
            inner: (version, hash_size, has_checksum_in_entry, number_of_flag_bytes)
        })]
        pub entries: Vec<Entry>,

        #[br(args{
            count: tag_count as usize,
            inner: (entry_count,)
        })]
        pub tags: Vec<Tag>,
    }

    #[derive(BinRead)]
    #[br(big, import(
        version: u8,
        hash_size: u8,
        has_checksum_in_entry: u8,
        number_of_flag_bytes: Option<u8>
    ))]
    pub struct Entry {
        #[br(count = hash_size)]
        pub key: Vec<u8>,
        pub file_size: u40,
        pub download_priority: u8,

        #[br(if(has_checksum_in_entry != 0))]
        pub checksum: Option<u32>,

        #[br(if(version >= 2), count = number_of_flag_bytes.unwrap_or(0))]
        pub flags: Vec<u8>,
    }

    #[derive(BinRead)]
    #[br(big, import(num_entries: u32))]
    pub struct Tag {
        pub name: NullString,
        pub type_: u16,

        #[br(count = div_ceil(num_entries as usize, u8::BITS as usize))]
        pub entries: Vec<u8>,
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
}
