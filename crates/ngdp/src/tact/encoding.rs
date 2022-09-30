use binrw::BinRead;
use std::io::Cursor;

use crate::casc::idx::Key;

pub struct Encoding {
    pub hash_size_ckey: u8,
    pub hash_size_ekey: u8,
    pub especs: Vec<String>,
    pub cekey_page_headers: Vec<repr::PageHeader>,
    pub cekey_pages: Vec<repr::CEKeyPage>,
    pub ekey_spec_page_headers: Vec<repr::PageHeader>,
    pub ekey_spec_pages: Vec<repr::EKeySpecPage>,
}

impl Encoding {
    pub fn lookup_by_ckey(&self, ckey: &Key) -> Option<&repr::CEKeyEntry> {
        let ckey = ckey.as_slice();
        assert_eq!(self.hash_size_ckey as usize, ckey.len());

        let mut page_idx = 0;
        let mut prev_first_key = self.cekey_page_headers[0].first_key.as_slice();
        for (i, page_header) in self.cekey_page_headers.iter().enumerate().skip(1) {
            let next_first_key = page_header.first_key.as_slice();
            if ckey >= prev_first_key && ckey <= next_first_key {
                break;
            } else {
                page_idx = i;
                prev_first_key = next_first_key;
            }
        }

        self.cekey_pages[page_idx]
            .entries
            .0
            .iter()
            .find(|entry| entry.ckey.as_slice() == ckey)
    }

    fn _lookup_by_ekey(&self, ekey: &[u8]) -> Option<&repr::CEKeyEntry> {
        // DO NOT USE THIS
        assert_eq!(self.hash_size_ekey as usize, ekey.len());

        for page in &self.cekey_pages {
            for entry in &page.entries.0 {
                for entry_ekey in &entry.ekeys {
                    if entry_ekey.as_slice() == ekey {
                        return Some(entry);
                    }
                }
            }
        }

        None
    }

    pub fn lookup_espec(&self, ekey: &[u8]) -> Option<(&repr::EKeySpecEntry, &str)> {
        assert_eq!(self.hash_size_ckey as usize, ekey.len());

        let mut page_idx = 0;
        let mut prev_first_key = self.ekey_spec_page_headers[0].first_key.as_slice();
        for (i, page_header) in self.ekey_spec_page_headers.iter().enumerate().skip(1) {
            let next_first_key = page_header.first_key.as_slice();
            if ekey >= prev_first_key && ekey <= next_first_key {
                break;
            } else {
                page_idx = i;
                prev_first_key = next_first_key;
            }
        }

        self.ekey_spec_pages[page_idx]
            .entries
            .0
            .iter()
            .find(|entry| entry.ekey.as_slice() == ekey)
            .map(|e| (e, self.especs[e.espec_index as usize].as_str()))
    }
}

pub fn parse_encoding(content: &[u8]) -> Result<Encoding, anyhow::Error> {
    let res = repr::EncodingTable::read(&mut Cursor::new(content))?;

    assert_eq!(1, res.version);
    assert_eq!(16, res.hash_size_ckey);
    assert_eq!(16, res.hash_size_ekey);
    assert_eq!(0, res.unk);

    Ok(Encoding {
        hash_size_ckey: res.hash_size_ckey,
        hash_size_ekey: res.hash_size_ekey,
        especs: res
            .espec_block
            .0
            .into_iter()
            .map(|s| s.to_string())
            .collect(),
        cekey_page_headers: res.cekey_page_headers,
        cekey_pages: res.cekey_pages,
        ekey_spec_page_headers: res.ekey_spec_page_headers,
        ekey_spec_pages: res.ekey_spec_pages,
    })
}

mod repr {
    use binrw::{BinRead, NullString};

    use crate::{
        binrw_ext::{u40, Block},
        casc::idx::Key,
    };

    #[derive(BinRead)]
    #[br(big, magic = b"EN")]
    pub struct EncodingTable {
        pub version: u8,
        pub hash_size_ckey: u8,
        pub hash_size_ekey: u8,
        pub cekey_page_table_page_size_kb: u16,
        pub ekey_spec_page_table_page_size_kb: u16,
        pub cekey_page_table_count: u32,
        pub ekey_spec_page_table_count: u32,
        pub unk: u8,
        pub espec_block_size: u32,

        #[br(count = espec_block_size)]
        pub espec_block: Block<NullString>,

        #[br(count = cekey_page_table_count)]
        pub cekey_page_headers: Vec<PageHeader>,

        #[br(args {
            count: cekey_page_table_count as usize,
            inner: (cekey_page_table_page_size_kb,)
        })]
        pub cekey_pages: Vec<CEKeyPage>,

        #[br(count = ekey_spec_page_table_count)]
        pub ekey_spec_page_headers: Vec<PageHeader>,

        #[br(args {
            count: ekey_spec_page_table_count as usize,
            inner: (ekey_spec_page_table_page_size_kb,)
        })]
        pub ekey_spec_pages: Vec<EKeySpecPage>,
    }

    #[derive(BinRead)]
    pub struct PageHeader {
        pub first_key: Key,
        pub page_md5: [u8; 16],
    }

    #[derive(BinRead)]
    #[br(import(cekey_page_table_page_size_kb: u16))]
    pub struct CEKeyPage {
        #[br(count = cekey_page_table_page_size_kb as usize * 1024)]
        pub entries: Block<CEKeyEntry>,
    }

    #[derive(BinRead, Debug)]
    #[br(big)]
    pub struct CEKeyEntry {
        pub key_count: u8,
        pub file_size: u40,
        pub ckey: Key,
        #[br(count = key_count)]
        pub ekeys: Vec<Key>,
    }

    #[derive(BinRead)]
    #[br(import(ekey_spec_page_table_page_size_kb: u16))]
    pub struct EKeySpecPage {
        #[br(count = ekey_spec_page_table_page_size_kb as usize * 1024)]
        pub entries: Block<EKeySpecEntry>,
    }

    #[derive(BinRead, Debug)]
    #[br(big)]
    pub struct EKeySpecEntry {
        pub ekey: Key,
        pub espec_index: u32,
        pub file_size: u40,
    }
}
