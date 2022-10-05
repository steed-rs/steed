use std::{
    fmt::{Debug, Display},
    str::FromStr,
};

use nom::{
    branch::alt,
    bytes::complete::{tag, take},
    character::complete::{char, u64, u8},
    combinator::*,
    multi::many0,
    sequence::{delimited, preceded, separated_pair, terminated, tuple},
    Finish,
};

use crate::util::parse_hex_bytes;

#[derive(Clone)]
pub enum ESpec {
    Raw,
    Zip(Zip),
    Encrypted(Encrypted),
    Blocks(Blocks),
}

impl Display for ESpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ESpec::Raw => write!(f, "n"),
            ESpec::Zip(v) => {
                if v.level == 9 && v.bits == 15 {
                    write!(f, "z")
                } else if v.level != 9 && v.bits == 15 {
                    write!(f, "z:{}", v.level)
                } else if v.bits == 0 {
                    write!(f, "z:{{{},mpq}}", v.level)
                } else {
                    write!(f, "z:{{{},{}}}", v.level, v.bits)
                }
            }
            ESpec::Encrypted(v) => {
                write!(
                    f,
                    "e:{{{},{},{}}}",
                    hex::encode_upper(v.key),
                    hex::encode(v.iv),
                    v.inner
                )
            }
            ESpec::Blocks(v) => {
                if !v.blocks.is_empty() {
                    write!(f, "b:{{")?;
                    for block in &v.blocks {
                        write!(f, "{},", block)?;
                    }
                    write!(f, "{}}}", v.final_)
                } else {
                    write!(f, "b:{}", v.final_)
                }
            }
        }
    }
}

impl Debug for ESpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Raw => write!(f, "Raw"),
            Self::Zip(arg0) => <Zip as Debug>::fmt(arg0, f),
            Self::Encrypted(arg0) => <Encrypted as Debug>::fmt(arg0, f),
            Self::Blocks(arg0) => <Blocks as Debug>::fmt(arg0, f),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Zip {
    pub level: u8,
    pub bits: u8,
}

#[derive(Clone)]
pub struct Encrypted {
    pub key: [u8; 8],
    pub iv: [u8; 4],
    pub inner: Box<ESpec>,
}

impl std::fmt::Debug for Encrypted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Encrypted")
            .field("key", &format_args!("{:02X?}", self.key))
            .field("iv", &format_args!("{:02X?}", self.iv))
            .field("inner", &self.inner)
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct Blocks {
    pub blocks: Vec<Block>,
    pub final_: Box<Block>,
}

#[derive(Clone, Debug)]
pub struct Block {
    pub size: u64,
    pub count: Option<u64>,
    pub inner: ESpec,
}

impl Display for Block {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.size {
            v if v & 0xfffff == 0 => write!(f, "{}M", v >> 20)?,
            v if v & 0x3ff == 0 => write!(f, "{}K", v >> 10)?,
            v => write!(f, "{}", v)?,
        };
        match self.count {
            Some(1) => {}
            Some(v) => write!(f, "*{}", v)?,
            None => write!(f, "*")?,
        };
        write!(f, "={}", self.inner)
    }
}

impl FromStr for ESpec {
    type Err = nom::error::VerboseError<String>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        complete(parse_espec)(s)
            .finish()
            .map(|(_rest, res)| res)
            .map_err(|e| nom::error::VerboseError {
                errors: e
                    .errors
                    .into_iter()
                    .map(|(i, k)| (i.to_string(), k))
                    .collect(),
            })
    }
}

type IResult<'a, O> = nom::IResult<&'a str, O, nom::error::VerboseError<&'a str>>;

fn parse_espec(input: &str) -> IResult<ESpec> {
    alt((
        map(parse_raw, |_| ESpec::Raw),
        map(parse_zip, ESpec::Zip),
        map(parse_encrypted, ESpec::Encrypted),
        map(parse_blocks, ESpec::Blocks),
    ))(input)
}

fn parse_raw(input: &str) -> IResult<()> {
    value((), char('n'))(input)
}

fn parse_zip(input: &str) -> IResult<Zip> {
    preceded(
        char('z'),
        opt(preceded(
            char(':'),
            alt((
                delimited(
                    char('{'),
                    separated_pair(u8, char(','), alt((u8, value(0, tag("mpq"))))),
                    char('}'),
                ),
                map(u8, |level| (level, 15)),
            )),
        )),
    )(input)
    .map(|(r, v)| {
        (r, {
            let (level, bits) = v.unwrap_or((9, 15));
            Zip { level, bits }
        })
    })
}

fn parse_encrypted(input: &str) -> IResult<Encrypted> {
    preceded(
        tag("e:"),
        delimited(
            char('{'),
            map(
                tuple((
                    map_opt(take(16usize), parse_hex_bytes::<8>),
                    char(','),
                    map_opt(take(8usize), parse_hex_bytes::<4>),
                    char(','),
                    parse_espec,
                )),
                |(key, _, iv, _, inner)| Encrypted {
                    key,
                    iv,
                    inner: Box::new(inner),
                },
            ),
            char('}'),
        ),
    )(input)
}

fn parse_blocks(input: &str) -> IResult<Blocks> {
    fn block_size(input: &str) -> IResult<u64> {
        map(
            tuple((u64, opt(alt((char('K'), char('M')))))),
            |(size, unit)| match unit {
                Some('K') => size * (1 << 10),
                Some('M') => size * (1 << 20),
                _ => size,
            },
        )(input)
    }
    fn block_size_spec(input: &str) -> IResult<(u64, u64)> {
        map(
            tuple((block_size, opt(preceded(char('*'), u64)))),
            |(size, count)| (size, count.unwrap_or(1)),
        )(input)
    }
    fn block_subchunk(input: &str) -> IResult<Block> {
        map(
            separated_pair(block_size_spec, char('='), parse_espec),
            |((size, count), inner)| Block {
                size,
                count: Some(count),
                inner,
            },
        )(input)
    }
    fn final_size_spec(input: &str) -> IResult<(u64, Option<u64>)> {
        alt((
            map(char('*'), |_| (0, None)),
            map(terminated(block_size, char('*')), |size| (size, None)),
            map(block_size_spec, |(size, count)| (size, Some(count))),
        ))(input)
    }
    fn final_subchunk(input: &str) -> IResult<Block> {
        map(
            separated_pair(final_size_spec, char('='), parse_espec),
            |((size, count), inner)| Block { size, count, inner },
        )(input)
    }
    map(
        preceded(
            tag("b:"),
            alt((
                map(final_subchunk, |v| (vec![], v)),
                delimited(
                    char('{'),
                    tuple((many0(terminated(block_subchunk, char(','))), final_subchunk)),
                    char('}'),
                ),
            )),
        ),
        |(blocks, final_)| Blocks {
            blocks,
            final_: Box::new(final_),
        },
    )(input)
}
