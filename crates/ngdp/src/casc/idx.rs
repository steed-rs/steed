use binrw::{BinRead, BinWrite};
use byteorder::{ByteOrder, BE, LE};
use lookup3::hashlittle2;
use std::collections::BTreeMap;
use std::convert::TryInto;
use std::fmt::Debug;
use std::io::Cursor;
use std::path::Path;

use crate::tact::EncodingKey;

use super::shmem::Shmem;
use super::NUM_INDEXES;

#[derive(Debug)]
pub struct Index {
    pub index: u8,
    pub entries: BTreeMap<[u8; 9], Entry>,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub archive_index: u16,
    pub offset: u32,
    pub size: u32,
}

impl Index {
    pub fn new(index: u8) -> Index {
        Index {
            index,
            entries: BTreeMap::new(),
        }
    }

    pub fn parse(content: &[u8], index: u8) -> Result<Index, anyhow::Error> {
        let repr::Index { header, entries } = repr::Index::read(&mut Cursor::new(content))?;

        let (pc, _pb) = hashlittle2(&content[8..][..header.header_hash_size as usize], 0, 0);
        assert_eq!(pc, header.header_hash, "index header hash did not match");
        assert_eq!(7, header.unk0);
        assert_eq!(index, header.bucket_index);
        assert_eq!(0, header.unk1);
        assert_eq!(4, header.entry_size_bytes);
        assert_eq!(5, header.entry_offset_bytes);
        assert_eq!(9, header.entry_key_bytes);
        assert_eq!(30, header.archive_file_header_bytes);
        assert_eq!(0x4000000000, header.archive_total_size_maximum);

        let mut entry_map = BTreeMap::new();

        // Entries hash is calculated by feeding along pc and pb for each 18 byte entry
        let (mut pc, mut pb) = (0, 0);
        for (idx, entry) in entries.into_iter().enumerate() {
            (pc, pb) = hashlittle2(&content[40 + 18 * idx..][..18], pc, pb);

            let index_offset = BE::read_uint(&entry.offset, 5);
            let archive_index = ((index_offset >> 30) & 0x3ff) as u16;
            let offset = (index_offset & 0x3fffffff) as u32;

            let key = entry.key;
            let entry = Entry {
                archive_index,
                offset,
                size: entry.size,
            };

            // BTreeMap::from_iter is seemingly faster on sorted input, but we lose the ability to check for duplicate keys
            let exists = entry_map.insert(key, entry.clone());
            if let Some(old_entry) = exists {
                eprintln!(
                    "duplicate key: {:?}. old value: {:?}, new value: {:?}",
                    key, old_entry, entry
                );
            }
        }
        assert_eq!(pc, header.entries_hash);

        Ok(Index {
            index,
            entries: entry_map,
        })
    }

    pub fn write(&self, buf: &mut Vec<u8>) -> Result<(), anyhow::Error> {
        const HEADER_HASH_SIZE: usize = 16;

        let start = buf.len();

        let header = repr::Header {
            header_hash_size: HEADER_HASH_SIZE as u32,
            header_hash: 0xdeadbeef,
            unk0: 7,
            bucket_index: self.index as u8,
            unk1: 0,
            entry_size_bytes: 4,
            entry_offset_bytes: 5,
            entry_key_bytes: 9,
            archive_file_header_bytes: 30,
            archive_total_size_maximum: 0x4000000000,
            _padding: Default::default(),
            entries_size: 18 * self.entries.len() as u32,
            entries_hash: 0xdeadbeef,
        };

        let entries = self
            .entries
            .iter()
            .map(|(key, entry)| {
                assert!(entry.offset <= 0x3fffffff);
                assert!(entry.archive_index <= 0x3ff);
                let index_offset = entry.offset as u64 | (entry.archive_index as u64) << 30;

                let mut offset = [0u8; 5];
                BE::write_uint(&mut offset, index_offset, 5);

                repr::Entry {
                    key: *key,
                    offset,
                    size: entry.size,
                }
            })
            .collect();

        let index = repr::Index { header, entries };

        let mut cursor = Cursor::new(buf);
        index.write(&mut cursor)?;

        let end = cursor.position() as usize;
        let buf = cursor.into_inner();

        // Patch up header hash
        let data = &mut buf[start..end];
        let (header_hash, _) = hashlittle2(&data[8..][..HEADER_HASH_SIZE], 0, 0);
        LE::write_u32(&mut data[4..8], header_hash);

        // Patch up entries hash
        let (mut pc, mut pb) = (0, 0);
        for idx in 0..index.entries.len() {
            (pc, pb) = hashlittle2(&data[40 + 18 * idx..][..18], pc, pb);
        }
        LE::write_u32(&mut data[36..40], pc);

        Ok(())
    }
}

pub struct Indexes {
    indexes: [Index; NUM_INDEXES],
}

impl Indexes {
    pub fn new(indexes: Vec<Index>) -> Indexes {
        assert_eq!(NUM_INDEXES, indexes.len());
        Indexes {
            indexes: indexes.try_into().unwrap(),
        }
    }

    pub fn read(path: &Path, shmem: &Shmem) -> Result<Indexes, anyhow::Error> {
        let mut indexes = vec![];
        assert!(shmem.index_versions.len() <= 0xff);
        for (index, version) in shmem.index_versions.iter().enumerate() {
            let name = format!("{:02x}{:08x}.idx", index, version);
            let index_data = std::fs::read(path.join(name))?;
            let index = Index::parse(&index_data, index as u8)?;
            indexes.push(index);
        }
        Ok(Indexes::new(indexes))
    }

    pub fn lookup(&self, k: &EncodingKey) -> Option<&Entry> {
        let bucket = Self::get_bucket(k) as usize;
        let index = &self.indexes[bucket];
        index.entries.get(&k.short())
    }

    pub fn insert(&mut self, k: &EncodingKey, entry: Entry) -> (usize, Option<Entry>) {
        let bucket = Self::get_bucket(k) as usize;
        let index = &mut self.indexes[bucket];
        (bucket, index.entries.insert(k.short(), entry))
    }

    pub fn lookup_cross_ref(&self, k: &EncodingKey) -> Option<&Entry> {
        let bucket = Self::get_bucket_cross_ref(k) as usize;
        let index = &self.indexes[bucket];
        index.entries.get(&k.short())
    }

    pub fn iter_all_entries(&self) -> impl Iterator<Item = (&[u8; 9], &Entry)> {
        self.indexes.iter().flat_map(|f| f.entries.iter())
    }

    pub fn write(&self, versions: [u32; 16], path: &Path) -> Result<(), anyhow::Error> {
        let mut buf = Vec::with_capacity(0x120000);
        for (index, version) in self.indexes.iter().zip(versions) {
            let filename = format!("{:02x}{:08x}.idx", index.index, version);
            let path = path.join(filename);

            buf.clear();
            index.write(&mut buf)?;

            std::fs::write(path, &buf)?;
        }

        Ok(())
    }

    fn get_bucket(k: &EncodingKey) -> u8 {
        let k = k.to_inner();
        let i = k[0] ^ k[1] ^ k[2] ^ k[3] ^ k[4] ^ k[5] ^ k[6] ^ k[7] ^ k[8];
        (i & 0xf) ^ (i >> 4)
    }

    fn get_bucket_cross_ref(k: &EncodingKey) -> u8 {
        let i = Self::get_bucket(k);
        (i + 1) % 16
    }
}

impl Default for Indexes {
    fn default() -> Self {
        Self {
            indexes: std::array::from_fn(|index| Index::new(index as u8)),
        }
    }
}

mod repr {
    use binrw::{BinRead, BinWrite};

    #[derive(BinRead, BinWrite, Debug)]
    #[brw(little)]
    pub struct Index {
        pub header: Header,
        #[br(count = header.entries_size / 18)]
        #[bw(pad_size_to = 0x120000 - 40)]
        pub entries: Vec<Entry>,
    }

    #[derive(BinRead, BinWrite, Debug)]
    #[brw(little)]
    pub struct Header {
        pub header_hash_size: u32,
        pub header_hash: u32,
        pub unk0: u16,
        pub bucket_index: u8,
        pub unk1: u8,
        pub entry_size_bytes: u8,
        pub entry_offset_bytes: u8,
        pub entry_key_bytes: u8,
        pub archive_file_header_bytes: u8,
        pub archive_total_size_maximum: u64,
        pub _padding: [u8; 8],
        pub entries_size: u32,
        pub entries_hash: u32,
    }

    #[derive(BinRead, BinWrite, Debug)]
    #[brw(little)]
    pub struct Entry {
        pub key: [u8; 9],
        pub offset: [u8; 5],
        pub size: u32,
    }
}
