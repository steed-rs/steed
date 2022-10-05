use std::io::Read;
use std::{fmt::Debug, io::Write};

use binrw::{BinRead, BinWrite};
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};

pub mod cdn;
pub mod config;
pub mod download;
pub mod encoding;
pub mod index;
pub mod install;
pub mod keys;
pub mod root;

/// MD5 hash of a file's uncompressed contents
#[derive(
    Clone, PartialEq, Eq, PartialOrd, Ord, Hash, BinRead, BinWrite, Serialize, Deserialize,
)]
pub struct ContentKey([u8; 16]);

impl ContentKey {
    pub fn from_data(data: &[u8]) -> Self {
        let mut hasher = Md5::new();
        hasher.write_all(data).unwrap();
        Self(hasher.finalize().into())
    }

    pub fn read_from_data(r: &mut impl Read) -> Result<Self, std::io::Error> {
        let mut hasher = Md5::new();
        std::io::copy(r, &mut hasher)?;
        Ok(Self(hasher.finalize().into()))
    }

    /// State that the file represented by this ContentKey is unencoded
    pub fn unencoded(self) -> EncodingKey {
        EncodingKey(self.0)
    }
}

/// MD5 hash based on part of the a files encoding:
/// * For chunkless BLTE files, the hash is of the entire file
/// * For chunked BLTE files, the hash is of the header, including chunk infos.
///   As the BLTE header contains hashes of the data, this serves to represent the entire file.
#[derive(
    Clone, PartialEq, Eq, PartialOrd, Ord, Hash, BinRead, BinWrite, Serialize, Deserialize,
)]
pub struct EncodingKey([u8; 16]);

macro_rules! impl_Key {
    ($($name:ident),*) => {$(
        impl $name {
            pub const ZERO: Self = Self([0u8; 16]);

            pub fn from_rev(mut input: [u8; 16]) -> Self {
                input.reverse();
                Self(input)
            }

            pub fn to_rev(&self) -> [u8; 16] {
                let mut res = self.0;
                res.reverse();
                res
            }

            pub fn from_hex(input: &str) -> Self {
                let mut res = [0; 16];
                hex::decode_to_slice(input, &mut res).unwrap();
                Self(res)
            }

            pub fn from_slice(input: &[u8]) -> Self  {
                Self(input.try_into().unwrap())
            }

            pub fn as_slice(&self) -> &[u8] {
                self.0.as_slice()
            }

            pub fn to_inner(&self) -> [u8; 16] {
                self.0
            }

            pub fn short(&self) -> [u8; 9] {
                // FIXME: Use const generic split once stable
                self.0[..9].try_into().unwrap()
            }

            pub fn parse(input: &str) -> Result<Self, hex::FromHexError> {
                let mut res = [0; 16];
                hex::decode_to_slice(input, &mut res)?;
                Ok(Self(res))
            }

            pub fn parse_rev(input: &str) -> Result<Self, hex::FromHexError> {
                let mut res = Self::parse(input)?;
                res.0.reverse();
                Ok(res)
            }
        }

        impl Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                for byte in self.0 {
                    write!(f, "{:02x}", byte)?;
                }
                Ok(())
            }
        }
    )*};
}

impl_Key!(ContentKey, EncodingKey);
