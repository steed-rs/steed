use std::io::Cursor;

use binrw::{until_eof, BinRead, BinWrite, VecArgs};
use binstream::BE;
use byteorder::ByteOrder;

#[derive(BinRead, BinWrite, Default)]
#[allow(non_camel_case_types)]
pub struct u40([u8; 5]);

macro_rules! defer_fmt {
    ($type:ty: $getter:ident => $($trait:ident),* ) => {$(
        impl std::fmt::$trait for $type {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                std::fmt::$trait::fmt(&<$type>::$getter(self), f)
            }
        }
    )*};
}

defer_fmt!(u40: get => Debug, Display, LowerHex, UpperHex);

impl u40 {
    pub const ZERO: u40 = u40([0; 5]);

    const MASK_30: u64 = 0x3fffffff;
    const MASK_10: u64 = 0x3ff;

    pub fn get(&self) -> u64 {
        BE::read_uint(&self.0, 5)
    }

    pub fn get_30_10(&self) -> (u32, u16) {
        let val = self.get();
        let large = (val & Self::MASK_30) as u32;
        let small = ((val >> 30) & Self::MASK_10) as u16;
        (large, small)
    }

    pub fn from_30_10(large: u32, small: u16) -> Option<u40> {
        if large > Self::MASK_30 as u32 || small > Self::MASK_10 as u16 {
            return None;
        }
        let val = (large as u64) & Self::MASK_30 | ((small as u64) & Self::MASK_10) << 30;

        let mut res = [0u8; 5];
        BE::write_uint(&mut res, val, 5);
        Some(u40(res))
    }
}

pub struct Block<T>(pub Vec<T>);

impl<T: BinRead> BinRead for Block<T> {
    type Args = VecArgs<T::Args>;

    fn read_options<R: std::io::Read + std::io::Seek>(
        reader: &mut R,
        options: &binrw::ReadOptions,
        args: Self::Args,
    ) -> binrw::BinResult<Self> {
        let mut buf = vec![0; args.count];
        reader.read_exact(&mut buf)?;
        until_eof(&mut Cursor::new(buf), options, args.inner).map(Block)
    }
}
