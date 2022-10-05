use binrw::{BinRead, BinWrite, NullString};
use std::{fmt::Debug, io::Cursor};

use super::{idx::Indexes, MAX_DATA_SIZE, NUM_INDEXES};
use crate::binrw_ext::u40;

pub struct Shmem {
    pub data_path: String,
    pub index_versions: [u32; NUM_INDEXES],
    pub unused_bytes: Vec<UnusedBytes>,
}

#[derive(Debug, Clone)]
pub struct UnusedBytes {
    pub data_file_missing: u16,
    pub data_number: u16,
    pub count: u32,
    pub offset: u32,
}

impl Shmem {
    pub fn new(data_path: &str) -> Shmem {
        Shmem {
            data_path: data_path.to_string(),
            index_versions: [0; 16],
            unused_bytes: (0..255)
                .map(|idx| UnusedBytes {
                    data_file_missing: 1,
                    data_number: idx,
                    count: 0,
                    offset: 0,
                })
                .collect(),
        }
    }

    pub fn reserve_bytes(&mut self, count: usize) -> Option<UnusedBytes> {
        let count: u32 = count.try_into().ok()?;

        let (idx, slot) = self
            .unused_bytes
            .iter_mut()
            .enumerate()
            .find(|(_idx, ub)| ub.count >= count || ub.data_file_missing == 1)?;

        let res = UnusedBytes {
            data_file_missing: slot.data_file_missing,
            data_number: slot.data_number,
            count,
            offset: slot.offset,
        };

        let new_count = if slot.data_file_missing == 0 {
            // Existing file
            slot.count.checked_sub(count)?
        } else {
            // New file
            (MAX_DATA_SIZE as u32).checked_sub(count)?
        };

        let new_offset = slot.offset.checked_add(count)?;

        if new_count == 0 {
            self.unused_bytes.remove(idx);
        } else {
            slot.data_file_missing = 0;
            slot.count = new_count;
            slot.offset = new_offset;
        }

        Some(res)
    }

    // Might not even be neccesary, client doesn't seem to provide this info
    // TODO: fn free_bytes(&mut self, data_number, count, offset)

    pub fn rebuild_unused_from_index(&mut self, index: &Indexes) {
        let mut all_entries = Vec::from_iter(index.iter_all_entries().map(|(_k, e)| e));
        all_entries.sort_by_key(|e| (e.archive_index, e.offset));

        let mut unused_bytes: Vec<UnusedBytes> = vec![];
        for entry in all_entries {
            let mut last = unused_bytes.pop().unwrap_or(UnusedBytes {
                data_file_missing: 1,
                data_number: entry.archive_index,
                count: 0,
                offset: 0,
            });

            let last_end = last.offset + last.count;
            if entry.archive_index == last.data_number && entry.offset == last_end {
                last.count += entry.size;
                unused_bytes.push(last);
            } else {
                unused_bytes.push(UnusedBytes {
                    data_file_missing: 0,
                    data_number: entry.archive_index,
                    count: entry.size,
                    offset: entry.offset,
                });
            }
        }

        // Add uncreated data files
        let last_archive = unused_bytes.last().map(|e| e.data_number).unwrap_or(0);
        unused_bytes.extend((last_archive..0xff).map(|i| UnusedBytes {
            data_file_missing: 1,
            data_number: i,
            count: 0,
            offset: 0,
        }));

        self.unused_bytes = unused_bytes;
    }
}

impl Debug for Shmem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShmemInfo")
            .field("data_path", &self.data_path)
            .field(
                "index_versions",
                &format_args!("{:02x?}", &self.index_versions),
            )
            .field("unused_bytes", &self.unused_bytes)
            .finish()
    }
}

impl Shmem {
    pub fn parse(content: &'_ [u8]) -> Result<Shmem, anyhow::Error> {
        let repr::Shmem { blocks } = repr::Shmem::read(&mut Cursor::new(content))?;

        let block4 = match blocks.get(0) {
            Some(repr::Block::Block4(block)) => block,
            Some(repr::Block::Block5(block5)) => {
                eprintln!("Encountered shmem block 5, not parsing any further");
                return Ok(Shmem {
                    data_path: block5.data_path.to_string(),
                    index_versions: block5.index_versions,
                    unused_bytes: vec![],
                });
            }
            val => panic!("unexpected first shmem block: {:?}", val),
        };
        dbg!(&block4);

        let block1 = match blocks.get(1) {
            Some(repr::Block::Block1(block)) => block,
            val => panic!("unexpected second shmem block: {:?}", val),
        };
        dbg!(&block1.next_block);

        assert_eq!(
            block4.next_block, 336,
            "header size changed, somethings new"
        );

        Ok(Shmem {
            data_path: block4.data_path.to_string(),
            index_versions: block4.index_versions,
            unused_bytes: block1
                .unused_byte_counts
                .iter()
                .zip(block1.unused_byte_positions.iter())
                .map(|(count, offset)| {
                    let (count, data_file_missing) = count.get_30_10();
                    let (offset, data_number) = offset.get_30_10();

                    match data_file_missing {
                        0 | 1 => {}
                        val => panic!("unexpected value for data_file_missing: {}", val),
                    };

                    UnusedBytes {
                        data_file_missing,
                        data_number,
                        count,
                        offset,
                    }
                })
                .collect(),
        })
    }

    pub fn write(&self, buf: &mut Vec<u8>) -> Result<(), anyhow::Error> {
        let block4 = repr::Block4 {
            next_block: 336,
            data_path: NullString::from(self.data_path.as_str()),
            blocks: [repr::BlockEntry {
                size: 10936,
                offset: 336,
            }],
            index_versions: self.index_versions,
        };

        let mut unused_byte_counts = [u40::ZERO; 1090];
        let mut unused_byte_positions = [u40::ZERO; 1090];

        assert!(self.unused_bytes.len() <= 1090);
        for (idx, unused_bytes) in self.unused_bytes.iter().enumerate() {
            let count = u40::from_30_10(unused_bytes.count, unused_bytes.data_file_missing)
                .ok_or_else(|| anyhow::anyhow!("u30+u10 overflow while writing shmem"))?;

            let position = u40::from_30_10(unused_bytes.offset, unused_bytes.data_number)
                .ok_or_else(|| anyhow::anyhow!("u30+u10 overflow while writing shmem"))?;

            unused_byte_counts[idx] = count;
            unused_byte_positions[idx] = position;
        }

        let block1 = repr::Block1 {
            next_block: 1090,
            _padding1: Default::default(),
            unused_byte_counts,
            unused_byte_positions,
            _padding2: Default::default(),
        };

        let shmem = repr::Shmem {
            blocks: vec![
                repr::Block::Block4(block4),
                repr::Block::Block1(block1),
                repr::Block::Block0 { next_block: 0 },
            ],
        };

        let mut cursor = Cursor::new(buf);
        shmem.write(&mut cursor)?;

        Ok(())
    }
}

mod repr {
    use binrw::{until_eof, BinRead, BinWrite, NullString};

    use crate::binrw_ext::u40;

    #[derive(BinRead, BinWrite, Debug)]
    #[brw(little)]
    pub struct Shmem {
        #[br(parse_with = until_eof)]
        pub blocks: Vec<Block>,
    }

    #[derive(BinRead, BinWrite, Debug)]
    #[brw(little)]
    #[allow(clippy::large_enum_variant)]
    pub enum Block {
        #[brw(magic(0u32))]
        Block0 { next_block: u32 },
        #[brw(magic(1u32))]
        Block1(Block1),

        // Not sure if this is an actualy block containing padding, or just a coincidence.
        // Only present after block 5
        #[brw(magic(3u32))]
        Block3(Block3),

        #[brw(magic(4u32))]
        Block4(Block4),
        #[brw(magic(5u32))]
        Block5(Block5),
    }

    #[derive(BinRead, BinWrite, Debug)]
    #[brw(little)]
    pub struct Block1 {
        pub next_block: u32,
        pub _padding1: [u8; 24],
        pub unused_byte_counts: [u40; 1090],
        pub unused_byte_positions: [u40; 1090],
        pub _padding2: [u8; 4],
    }

    #[derive(BinRead, BinWrite, Debug)]
    #[brw(little)]
    pub struct Block3 {
        // There might be an empty 1 block, and 2 block included in this padding
        #[br(count = 20140)]
        pub padding: Vec<u8>,
    }

    #[derive(BinRead, BinWrite, Debug)]
    #[brw(little)]
    pub struct Block4 {
        pub next_block: u32,
        #[brw(pad_size_to = 0x100)]
        pub data_path: NullString,
        // entry count = (header.next_block.get() - sizeof(BlockHeader) - sizeof(data_path) - num_index * sizeof(index_versions[0])) / sizeof(BlockEntry)
        pub blocks: [BlockEntry; 1],
        pub index_versions: [u32; 16],
    }

    #[derive(BinRead, BinWrite, Debug)]
    #[brw(little)]
    pub struct Block5 {
        pub next_block: u32,
        #[brw(pad_size_to = 0x100)]
        pub data_path: NullString,
        // entry count = (header.next_block.get() - sizeof(BlockHeader) - sizeof(data_path) - num_index * sizeof(index_versions[0])) / sizeof(BlockEntry)
        pub blocks: [BlockEntry; 1],
        pub index_versions: [u32; 16],
    }

    #[derive(BinRead, BinWrite, Debug)]
    #[brw(little)]
    pub struct BlockEntry {
        pub size: u32,
        pub offset: u32,
    }
}
