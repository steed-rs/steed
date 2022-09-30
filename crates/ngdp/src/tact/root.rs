use std::collections::HashMap;
use std::fmt::Debug;
use std::io::Cursor;

use binrw::BinRead;
use lookup3::hashlittle2;

use crate::casc::idx::Key;

#[derive(Debug)]
pub struct Root {
    pub total_file_count: u32,
    pub named_file_count: u32,
    pub record_types: Vec<RecordType>,
    // TODO: A lot of these are one entry only, could probably benefit from SmallVec
    pub record_types_by_file_data_id: HashMap<i32, Vec<u32>>,
    pub record_types_by_name_hash: HashMap<u64, Vec<u32>>,
}

impl Root {
    // TODO: Construct this in a streaming manner if memory becomes an issue
    fn new(total_file_count: u32, named_file_count: u32, blocks: Vec<repr::Block>) -> Root {
        let mut record_types = vec![];
        let mut record_types_by_file_data_id: HashMap<i32, Vec<u32>> = HashMap::new();
        let mut record_types_by_name_hash: HashMap<u64, Vec<u32>> = HashMap::new();

        for (record_type, block) in blocks.into_iter().enumerate() {
            let record_type = record_type as u32;
            let mut records_by_file_data_id = HashMap::new();
            let mut file_data_id_by_name_hash = HashMap::new();

            let mut file_data_id = -1;
            for (i, content_key) in block.content_keys.into_iter().enumerate() {
                file_data_id += block.file_data_id_deltas[i] + 1;

                let record = Record {
                    content_key: content_key,
                    name_hash: block.name_hashes.get(i).copied(),
                };

                record_types_by_file_data_id
                    .entry(file_data_id)
                    .or_default()
                    .push(record_type);

                let exists = records_by_file_data_id.insert(file_data_id, record.clone());
                assert!(
                    exists.is_none(),
                    "duplicate cas record for file data id {} - prev: {:?}, new: {:?}",
                    file_data_id,
                    exists,
                    record
                );

                if let Some(name_hash) = record.name_hash {
                    // println!("name hash: {name_hash:08x} => file data id: {file_data_id}");

                    record_types_by_name_hash
                        .entry(name_hash)
                        .or_default()
                        .push(record_type);

                    let exists = file_data_id_by_name_hash.insert(name_hash, file_data_id);
                    assert!(
                        exists.is_none(),
                        "duplicate file data id for name hash {} - prev: {:?}, new: {:?}",
                        name_hash,
                        exists,
                        file_data_id
                    );
                }
            }

            record_types.push(RecordType {
                content_flags: block.flags,
                locale_flags: block.locale,
                records_by_file_data_id,
                file_data_id_by_name_hash,
            });
        }

        Root {
            total_file_count,
            named_file_count,
            record_types,
            record_types_by_file_data_id,
            record_types_by_name_hash,
        }
    }

    pub fn lookup_path(&self, path: &str) -> Option<&[u32]> {
        let hash = Self::hashpath(path);
        println!("hash: {hash:08x}");
        self.record_types_by_name_hash.get(&hash).map(Vec::as_slice)
    }

    fn hashpath(path: &str) -> u64 {
        let path = path.to_uppercase().replace('/', "\\");
        let (pc, pb) = hashlittle2(path.as_bytes(), 0, 0);
        pb as u64 | ((pc as u64) << 32)
    }

    pub fn lookup_by_fileid_and_flags(
        &self,
        file_id: i32,
        content_flags: ContentFlags,
        locale_flags: LocaleFlags,
    ) -> Option<&Record> {
        let rec_types = self.record_types_by_file_data_id.get(&file_id)?;
        let rec_type = rec_types
            .iter()
            .copied()
            .map(|r| &self.record_types[r as usize])
            .find(|rec| {
                rec.content_flags.contains(content_flags) && rec.locale_flags.contains(locale_flags)
            })?;
        let record = rec_type.records_by_file_data_id.get(&file_id)?;
        Some(record)
    }
}

pub struct RecordType {
    pub content_flags: ContentFlags,
    pub locale_flags: LocaleFlags,
    pub records_by_file_data_id: HashMap<i32, Record>,
    pub file_data_id_by_name_hash: HashMap<u64, i32>,
}

impl std::fmt::Debug for RecordType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("RecordType")
            .field("content_flags", &self.content_flags)
            .field("locale_flags", &self.locale_flags)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct Record {
    pub content_key: Key,
    pub name_hash: Option<u64>,
}

pub fn parse_root(content: &[u8]) -> Result<Root, anyhow::Error> {
    let res = repr::Root::read(&mut Cursor::new(content))?;
    Ok(Root::new(
        res.total_file_count,
        res.named_file_count,
        res.blocks,
    ))
}

bitflags::bitflags! {
    pub struct LocaleFlags: u32 {
        const EN_US =     0x2;
        const KO_KR =     0x4;
        const FR_FR =    0x10;
        const DE_DE =    0x20;
        const ZH_CN =    0x40;
        const ES_ES =    0x80;
        const ZH_TW =   0x100;
        const EN_GB =   0x200;
        const EN_CN =   0x400;
        const EN_TW =   0x800;
        const ES_MX =  0x1000;
        const RU_RU =  0x2000;
        const PT_BR =  0x4000;
        const IT_IT =  0x8000;
        const PT_PT = 0x10000;
    }
}

bitflags::bitflags! {
    pub struct ContentFlags: u32 {
        const LOAD_ON_WINDOWS     =        0x8;            // macOS clients do not read block if flags & 0x108 != 0
        const LOAD_ON_MACOS       =       0x10;            // windows clients do not read block if flags & 0x110 != 0
        const LOW_VIOLENCE        =       0x80;
        const DO_NOT_LOAD         =      0x100;            // neither macOS nor windows clients read blocks with this flag set. LoadOnMysteryPlatformᵘ?
        const UPDATE_PLUGIN       =      0x800;            // only ever set for UpdatePlugin.dll and UpdatePlugin.dylib
        const UNKNOWN1            =    0x20000;
        const UNKNOWN2            =    0x40000;
        const UNKNOWN3            =    0x80000;
        const UNKNOWN4            =   0x100000;
        const UNKNOWN5            =   0x200000;
        const UNKNOWN6            =   0x400000;
        const UNKNOWN7            =   0x800000;
        const UNKNOWN8            =  0x2000000;
        const UNKNOWN9            =  0x4000000;
        const ENCRYPTED           =  0x8000000;
        const NO_NAME_HASH        = 0x10000000;
        const UNCOMMON_RESOLUTION = 0x20000000;            // denotes non-1280px wide cinematics
        const BUNDLE              = 0x40000000;
        const NO_COMPRESSION      = 0x80000000;
    }
}

// TODO: Support pre 8.2 representation
mod repr {
    use binrw::{until_eof, BinRead};

    use crate::casc::idx::Key;

    use super::{ContentFlags, LocaleFlags};

    #[derive(BinRead)]
    #[br(little, magic = b"TSFM")]
    pub struct Root {
        pub total_file_count: u32,
        pub named_file_count: u32,

        #[br(parse_with = until_eof, args(total_file_count != named_file_count))]
        pub blocks: Vec<Block>,
    }

    #[derive(BinRead)]
    #[br(little, import(allow_non_named_files: bool))]
    pub struct Block {
        pub num_records: u32,

        #[br(try_map = |x| ContentFlags::from_bits(x).ok_or_else(|| anyhow::anyhow!("ContentFlags: invalid bits")))]
        pub flags: ContentFlags,

        #[br(try_map = |x| LocaleFlags::from_bits(x).ok_or_else(|| anyhow::anyhow!("LocaleFlags: invalid bits")))]
        pub locale: LocaleFlags,

        #[br(count = num_records)]
        pub file_data_id_deltas: Vec<i32>,

        #[br(count = num_records)]
        pub content_keys: Vec<Key>,

        #[br(if(!(allow_non_named_files && flags.contains(ContentFlags::NO_NAME_HASH))), count = num_records)]
        pub name_hashes: Vec<u64>,
    }
}
