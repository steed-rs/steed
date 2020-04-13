use binstream::{u16_be, u32_be, u40_be, ByteParse, ByteReader, StringZero};
use std::fmt::Debug;

use crate::casc::idx::Key;

pub struct Encoding {
    pub hash_size_ckey: u8,
    pub hash_size_ekey: u8,
    pub especs: Vec<StringZero>,
    pub cekey_page_headers: Vec<PageHeader>,
    pub cekey_pages: Vec<CEKeyPage>,
    pub ekey_spec_page_headers: Vec<PageHeader>,
    pub ekey_spec_pages: Vec<EKeySpecKeyPage>,
}

impl Encoding {
    pub fn lookup_by_ckey(&self, ckey: &Key) -> Option<&CEKeyEntry> {
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
            .iter()
            .find(|entry| entry.ckey.as_slice() == ckey)
    }

    fn _lookup_by_ekey(&self, ekey: &[u8]) -> Option<&CEKeyEntry> {
        // DO NOT USE THIS
        assert_eq!(self.hash_size_ekey as usize, ekey.len());

        for page in &self.cekey_pages {
            for entry in &page.entries {
                for entry_ekey in &entry.ekeys {
                    if entry_ekey.as_slice() == ekey {
                        return Some(entry);
                    }
                }
            }
        }

        None
    }

    pub fn lookup_espec(&self, ekey: &[u8]) -> Option<(&EKeySpecEntry, &str)> {
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
            .iter()
            .find(|entry| entry.ekey.as_slice() == ekey)
            .map(|e| (e, self.especs[e.espec_index.get() as usize].0.as_str()))
    }
}

#[derive(ByteParse)]
struct Header {
    signature: [u8; 2],
    version: u8,
    hash_size_ckey: u8,
    hash_size_ekey: u8,
    cekey_page_table_page_size_kb: u16_be,
    ekey_spec_page_table_page_size_kb: u16_be,
    cekey_page_table_count: u32_be,
    ekey_spec_page_table_count: u32_be,
    unk: u8,
    espec_block_size: u32_be,
}

pub fn parse_encoding(content: &[u8]) -> Option<Encoding> {
    let r = &mut ByteReader::new(content);

    let header = Header::parse(r)?;

    assert_eq!(b"EN", &header.signature);
    assert_eq!(1, header.version);
    assert_eq!(16, header.hash_size_ckey);
    assert_eq!(16, header.hash_size_ekey);
    assert_eq!(0, header.unk);

    let espec_block = r.take(header.espec_block_size.get() as usize)?;

    let especs = {
        let r = &mut ByteReader::new(espec_block);
        r.many0::<StringZero>()
    };

    // C key => E key table
    let cekey_page_headers = r.repeat::<PageHeader>(header.cekey_page_table_count.get() as usize)?;

    let cekey_pages = r.repeat_fn(
        |r| parse_cekey_page(header.cekey_page_table_page_size_kb.get(), r),
        header.cekey_page_table_count.get() as usize,
    )?;

    // E key => E spec table
    let ekey_spec_page_headers =
        r.repeat::<PageHeader>(header.ekey_spec_page_table_count.get() as usize)?;

    let ekey_spec_pages = r.repeat_fn(
        |r| parse_ekey_spec_page(header.ekey_spec_page_table_page_size_kb.get(), r),
        header.ekey_spec_page_table_count.get() as usize,
    )?;

    Some(Encoding {
        hash_size_ckey: header.hash_size_ckey,
        hash_size_ekey: header.hash_size_ekey,
        especs,
        cekey_page_headers,
        cekey_pages,
        ekey_spec_page_headers,
        ekey_spec_pages,
    })
}

#[derive(ByteParse)]
pub struct PageHeader {
    pub first_key: Key,
    pub page_md5: [u8; 16],
}

pub struct CEKeyPage {
    pub entries: Vec<CEKeyEntry>,
}

fn parse_cekey_page(cekey_page_table_page_size_kb: u16, r: &mut ByteReader) -> Option<CEKeyPage> {
    let page_size = cekey_page_table_page_size_kb as usize * 1024;
    let page = r.take(page_size)?;
    let entries = {
        let r = &mut ByteReader::new(page);
        r.many1::<CEKeyEntry>()?
    };
    Some(CEKeyPage { entries })
}

#[derive(Debug)]
pub struct CEKeyEntry {
    pub file_size: u64,
    pub ckey: Key,
    pub ekeys: Vec<Key>,
}

impl ByteParse for CEKeyEntry {
    fn parse(r: &mut ByteReader) -> Option<Self> {
        let key_count: u8 = r.parse()?;
        let file_size = r.parse::<u40_be>()?.get();
        let ckey = Key::parse(r)?;
        let ekeys = r.repeat::<Key>(key_count as usize)?;
        Some(CEKeyEntry {
            file_size,
            ckey,
            ekeys,
        })
    }
}

pub struct EKeySpecKeyPage {
    pub entries: Vec<EKeySpecEntry>,
}

fn parse_ekey_spec_page(
    ekey_spec_page_table_page_size_kb: u16,
    r: &mut ByteReader,
) -> Option<EKeySpecKeyPage> {
    let page_size = ekey_spec_page_table_page_size_kb as usize * 1024;
    let page = r.take(page_size)?;
    let entries = {
        let r = &mut ByteReader::new(page);
        r.many1::<EKeySpecEntry>()?
    };
    Some(EKeySpecKeyPage { entries })
}

#[derive(ByteParse, Debug)]
pub struct EKeySpecEntry {
    pub ekey: Key,
    pub espec_index: u32_be,
    pub file_size: u40_be,
}
