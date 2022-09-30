use crate::{
    casc::{blte::decode_blte, idx::Key, shmem::Shmem},
    tact::{
        config::BuildConfig,
        encoding::{parse_encoding, Encoding},
        keys::TactKeys,
    },
};
use anyhow::anyhow;
use binrw::{BinRead, BinWrite};
use byteorder::{ByteOrder, LE};
use lookup3::hashlittle;
use std::{
    collections::HashSet,
    fs::File,
    io::{Cursor, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use self::idx::Indexes;

pub mod blte;
pub mod idx;
pub mod shmem;

pub const NUM_INDEXES: usize = 16;
pub const MAX_DATA_SIZE: usize = 0x3fffffff;

#[derive(BinRead, BinWrite)]
#[brw(little)]
pub struct FileHeader {
    pub hash: [u8; 16],
    pub size: u32,
    pub _unk: [u8; 2],
    pub checksum_a: u32,
    pub checksum_b: u32,
}

// Verified table is the same as of Agent.exe v8020 (offset 0041:CB98)
const TABLE_16C57A8: [u32; 0x10] = [
    0x049396b8, 0x72a82a9b, 0xee626cca, 0x9917754f, 0x15de40b1, 0xf5a8a9b6, 0x421eac7e, 0xa9d55c9a,
    0x317fd40c, 0x04faf80d, 0x3d6be971, 0x52933cfd, 0x27f64b7d, 0xc6f5c11b, 0xd5757e3a, 0x6c388745,
];

impl FileHeader {
    pub const CHECKSUM_A_OFF: usize = 22;
    pub const CHECKSUM_B_OFF: usize = 26;
    pub const SIZE: usize = 30;

    pub fn write_to(
        &self,
        archive_index: u16,
        offset: u32,
        w: &mut impl Write,
    ) -> Result<(), anyhow::Error> {
        let mut buf = Vec::with_capacity(Self::SIZE);
        self.write(&mut Cursor::new(&mut buf))?;

        // Patch up checksums
        let (checksum_a, checksum_b) = Self::checksums(&buf, archive_index, offset);
        LE::write_u32(&mut buf[Self::CHECKSUM_A_OFF..], checksum_a);
        LE::write_u32(&mut buf[Self::CHECKSUM_B_OFF..], checksum_b);

        w.write(&buf)?;
        Ok(())
    }

    // Reverse engineered from Agent. Verified to match output from agent
    // Can be located by locating TABLE_16C57A8 and finding the only XREF
    pub fn checksums(data: &[u8], archive_index: u16, offset: u32) -> (u32, u32) {
        let checksum_a = hashlittle(&data[..Self::CHECKSUM_A_OFF], 0x3D6BE971);

        // For some ungodly reason the top two bits of the offset must be set to the bottom two bits of the archive index
        let offset = (offset & 0x3fffffff) | (archive_index as u32 & 3) << 30;

        let encoded_offset = offset + Self::SIZE as u32;
        let encoded_offset = TABLE_16C57A8[(encoded_offset & 0x0f) as usize] ^ encoded_offset;
        let encoded_offset = encoded_offset.to_le_bytes();

        let mut hashed_header = [0u8; 4];
        for i in 0..Self::CHECKSUM_B_OFF {
            hashed_header[(i + offset as usize) & 3] ^= data[i];
        }

        let mut checksum_b = [0u8; 4];
        for j in 0..4 {
            let i = j + Self::CHECKSUM_B_OFF + offset as usize;
            checksum_b[j] = hashed_header[i & 3] ^ encoded_offset[i & 3];
        }

        (checksum_a, u32::from_le_bytes(checksum_b))
    }
}

fn read_file(
    data_path: &Path,
    entry: &idx::Entry,
    tact_keys: &TactKeys,
) -> Result<Vec<u8>, anyhow::Error> {
    let data_file = data_path.join(format!("data.{:03}", entry.archive_index));
    let mut buf = vec![0; entry.size as usize];

    let mut file = File::open(data_file)?;
    file.seek(SeekFrom::Start(entry.offset as u64))?;
    file.read_exact(&mut buf)?;

    assert!(buf.len() > FileHeader::SIZE, "data block too small");

    let header = FileHeader::read(&mut Cursor::new(&buf))?;
    let _key = Key::from_rev(header.hash);

    let (checksum_a, checksum_b) = FileHeader::checksums(&buf, entry.archive_index, entry.offset);
    assert_eq!(checksum_a, header.checksum_a);
    assert_eq!(checksum_b, header.checksum_b);

    // TODO: Verify key?

    if buf.len() < header.size as usize {
        return Err(anyhow!(
            "entry size smaller than header size - got: {}, wanted: {}",
            buf.len(),
            header.size
        ));
    }

    let data = &buf[FileHeader::SIZE..header.size as usize];
    assert!(!data.is_empty());
    decode_blte(tact_keys, data).ok_or_else(|| anyhow!("couldn't blte decode file data"))
}

pub struct CASC {
    pub data_path: PathBuf,
    pub indexes: Indexes,
    pub encoding: Encoding,
    pub tact_keys: TactKeys,
}

impl CASC {
    pub fn new(root_path: &str, build_config: &BuildConfig) -> Result<CASC, anyhow::Error> {
        let root_path = Path::new(root_path);
        let data_path = root_path.join("Data/data");

        let num_indexes = data_path
            .read_dir()?
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.ends_with(".idx"))
            .map(|f| f.split_at(2).0.to_string())
            .collect::<HashSet<String>>()
            .len();

        assert_eq!(
            num_indexes, NUM_INDEXES,
            "num_indexes assumed to always be {}, suddenly it's not!",
            NUM_INDEXES
        );

        let shmem_data = std::fs::read(data_path.join("shmem"))?;
        let shmem = Shmem::parse(&shmem_data)?;
        dbg!(&shmem);

        let indexes = Indexes::read(&data_path, &shmem)?;

        let tact_keys = TactKeys::default();

        let encoding = {
            let decoded_encoding_hashsize = build_config
                .encoding
                .as_ref()
                .ok_or_else(|| anyhow!("build config had no encoding field"))?
                .encoded
                .as_ref()
                .expect("encoded hash for encoding file not found, can't progress");
            let key = Key::from_hex(&decoded_encoding_hashsize.hash);
            let entry = indexes.lookup(&key).unwrap();
            let file = read_file(&data_path, entry, &tact_keys)?;
            parse_encoding(&file)?
        };

        Ok(CASC {
            data_path,
            indexes,
            encoding,
            tact_keys,
        })
    }

    pub fn read_by_ckey(&self, ckey: &Key) -> Result<Vec<u8>, anyhow::Error> {
        let ce_entry = self
            .encoding
            .lookup_by_ckey(ckey)
            .ok_or_else(|| anyhow!("couldn't find encoding for ckey. ckey = {:?}", ckey))?;
        let ekey = &ce_entry.ekeys[0];
        let entry = self
            .indexes
            .lookup(ekey)
            .ok_or_else(|| anyhow!("couldn't find entry for ekey. ekey = {:?}", ekey))?;
        read_file(&self.data_path, entry, &self.tact_keys)
    }
}
