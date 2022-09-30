use std::{collections::HashSet, io::Cursor};

use binrw::BinRead;
use bitvec::{prelude::Msb0, vec::BitVec};

use super::keys::TactKeys;
use crate::casc::{blte::decode_blte, idx::Key};

#[derive(Debug)]
pub struct InstallManifest {
    pub version: u8,
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

#[derive(Debug)]
pub struct File {
    pub name: String,
    pub key: Key,
    pub size: u32,
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

pub fn parse_install_manifest(
    tact_keys: &TactKeys,
    content: &[u8],
) -> Result<InstallManifest, anyhow::Error> {
    let content = decode_blte(tact_keys, content)?;

    let res = repr::InstallManifest::read(&mut Cursor::new(content))?;
    assert_eq!(16, res.hash_size);

    let num_entries = res.num_entries as usize;
    Ok(InstallManifest {
        version: res.version,
        tags: res
            .tags
            .into_iter()
            .map(|t| Tag {
                name: t.name.to_string(),
                type_: t.type_,
                files: {
                    let mut files = BitVec::from_vec(t.files);
                    if files.len() > num_entries {
                        files.drain(num_entries..);
                    }
                    files
                },
            })
            .collect(),
        files: res
            .files
            .into_iter()
            .map(|f| File {
                name: f.name.to_string(),
                key: f.key,
                size: f.size,
            })
            .collect(),
    })
}

mod repr {
    use binrw::{BinRead, NullString};

    use crate::casc::idx::Key;

    #[derive(BinRead)]
    #[br(big, magic = b"IN")]
    pub struct InstallManifest {
        pub version: u8,
        pub hash_size: u8,
        pub num_tags: u16,
        pub num_entries: u32,

        #[br(args {
            count: num_tags as usize,
            inner: (num_entries,)
        })]
        pub tags: Vec<Tag>,

        #[br(count = num_entries)]
        pub files: Vec<File>,
    }

    #[derive(BinRead)]
    #[br(big, import(num_entries: u32))]
    pub struct Tag {
        pub name: NullString,
        pub type_: u16,
        #[br(count = div_ceil(num_entries as usize, u8::BITS as usize))]
        pub files: Vec<u8>,
    }

    #[derive(BinRead, Debug)]
    #[br(big)]
    pub struct File {
        pub name: NullString,
        pub key: Key,
        pub size: u32,
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
