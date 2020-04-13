use ::byteorder::ByteOrder;
use std::{borrow::Cow, marker::PhantomData};
use zerocopy::FromBytes;

pub use crate::byteorder::*;
pub use binstream_derive::ByteParse;

pub mod byteorder;

pub fn asciiz(val: &[u8]) -> Cow<str> {
    let first_zero = val.iter().position(|&b| b == 0).unwrap_or(val.len());
    let val = &val[..first_zero];
    String::from_utf8_lossy(val)
}

pub struct ByteReader<'a> {
    source: &'a [u8],
    offset: usize,
}

pub struct Mark(usize);

impl<'a> ByteReader<'a> {
    pub fn new(source: &'a [u8]) -> ByteReader<'a> {
        ByteReader { source, offset: 0 }
    }

    pub fn check_enough_bytes(&self, count: usize) -> Option<()> {
        let left = self.source.len() - self.offset;
        if left < count {
            None
        } else {
            Some(())
        }
    }

    pub fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        self.check_enough_bytes(count)?;

        let start = self.offset;
        let end = self.offset + count;
        self.offset = end;
        Some(&self.source[start..end])
    }

    pub fn take_n<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.check_enough_bytes(N)?;

        let start = self.offset;
        let end = self.offset + N;
        self.offset = end;
        Some(
            self.source[start..end]
                .try_into()
                .expect("internal: error in bounds check"),
        )
    }

    pub fn rest(&self) -> &'a [u8] {
        &self.source[self.offset..]
    }

    pub fn uint<O: ByteOrder>(&mut self, n: usize) -> Option<u64> {
        let input = self.take(n)?;
        let res = O::read_uint(input, n);
        Some(res)
    }

    pub fn string_zero(&mut self) -> Option<Cow<'a, str>> {
        self.check_enough_bytes(1)?;
        let val = asciiz(&self.source[self.offset..]);
        self.offset += val.len() + 1; // Skip the trailing zero as well
        Some(val)
    }

    pub fn mark(&self) -> Mark {
        Mark(self.offset)
    }

    pub fn restore(&mut self, mark: Mark) {
        self.offset = mark.0;
    }

    pub fn parse<T: ByteParse>(&mut self) -> Option<T> {
        T::parse(self)
    }

    pub fn peek<T: ByteParse>(&mut self) -> Option<T> {
        let mark = self.mark();
        let res = T::parse(self);
        self.restore(mark);
        res
    }

    pub fn cond<T: ByteParse>(&mut self, cond: bool) -> Option<Option<T>> {
        if cond {
            T::parse(self).map(Some)
        } else {
            Some(None)
        }
    }

    pub fn repeat<T: ByteParse>(&mut self, count: usize) -> Option<Vec<T>> {
        self.repeat_fn(T::parse, count)
    }

    pub fn repeat_fn<T, F>(&mut self, f: F, count: usize) -> Option<Vec<T>>
    where
        F: Fn(&mut ByteReader) -> Option<T>,
    {
        (0..count).map(|_| f(self)).collect()
    }

    pub fn many0<T: ByteParse>(&mut self) -> Vec<T> {
        let mut res = vec![];
        loop {
            let mark = self.mark();
            match T::parse(self) {
                Some(v) => res.push(v),
                None => {
                    self.restore(mark);
                    break;
                }
            }
        }
        res
    }

    pub fn many1<T: ByteParse>(&mut self) -> Option<Vec<T>> {
        self.many1_fn(T::parse)
    }

    pub fn many1_fn<T, F>(&mut self, f: F) -> Option<Vec<T>>
    where
        F: Fn(&mut ByteReader) -> Option<T>,
    {
        let mut res = vec![];
        loop {
            let mark = self.mark();
            match f(self) {
                Some(v) => res.push(v),
                None if res.is_empty() => return None,
                None => {
                    self.restore(mark);
                    break;
                }
            }
        }
        Some(res)
    }
}

pub trait ByteParse: Sized {
    fn parse(r: &mut ByteReader) -> Option<Self>;

    fn parse_slice(source: &[u8]) -> Option<Self> {
        let r = &mut ByteReader::new(source);
        Self::parse(r)
    }
}

pub const fn assert_is_byte_parse<T: ByteParse>() {}

impl ByteParse for () {
    fn parse(_r: &mut ByteReader) -> Option<Self> {
        Some(())
    }
}

impl ByteParse for u8 {
    fn parse(r: &mut ByteReader) -> Option<Self> {
        r.take_n::<1>().map(|v| v[0])
    }
}

impl ByteParse for i8 {
    fn parse(r: &mut ByteReader) -> Option<Self> {
        r.take_n::<1>().map(|v| v[0] as i8)
    }
}

// TODO: Specialize for [u8; N]
// TODO: Optimize if/when we add fixed size specific parsing
impl<const N: usize, T: ByteParse> ByteParse for [T; N] {
    fn parse(r: &mut ByteReader) -> Option<Self> {
        // TODO: Use MaybeUninit::uninit_array() once stable
        // TODO: Or with core::array::try_from_fn once stable
        let mut res = Vec::with_capacity(N);
        for _ in 0..N {
            let val = T::parse(r)?;
            res.push(val);
        }

        let res = res.try_into().ok().unwrap();
        Some(res)
    }
}

impl<A: ByteParse, B: ByteParse> ByteParse for (A, B) {
    fn parse(r: &mut ByteReader) -> Option<Self> {
        let a = A::parse(r)?;
        let b = B::parse(r)?;
        Some((a, b))
    }
}

impl<T: 'static> ByteParse for PhantomData<T> {
    fn parse(_r: &mut ByteReader) -> Option<Self> {
        Some(PhantomData)
    }
}

#[repr(transparent)]
pub struct StringZero(pub String);

impl ByteParse for StringZero {
    fn parse(r: &mut ByteReader) -> Option<Self> {
        r.string_zero().map(|s| StringZero(s.to_string()))
    }
}

impl std::fmt::Debug for StringZero {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl std::fmt::Display for StringZero {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(FromBytes)]
pub struct StringZeroFixed<T>(T);

impl<const N: usize> StringZeroFixed<[u8; N]> {
    pub fn to_str(&self) -> Cow<'_, str> {
        asciiz(&self.0)
    }
}

impl<const N: usize> ByteParse for StringZeroFixed<[u8; N]> {
    fn parse(r: &mut ByteReader) -> Option<Self> {
        r.take_n().map(StringZeroFixed)
    }
}

impl<const N: usize> std::fmt::Debug for StringZeroFixed<[u8; N]> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.to_str())
    }
}

impl<const N: usize> std::fmt::Display for StringZeroFixed<[u8; N]> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

pub struct ParseVia<T, S>(pub T, PhantomData<S>);

impl<T, S> ByteParse for ParseVia<T, S>
where
    T: From<S>,
    S: ByteParse,
{
    fn parse(r: &mut ByteReader) -> Option<Self> {
        let val = S::parse(r)?;
        let val: T = val.into();
        Some(ParseVia(val, PhantomData))
    }
}

pub struct Ascii<const N: usize>(pub [u8; N]);

impl<const N: usize> std::fmt::Debug for Ascii<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "'{}'", self.0.escape_ascii())
    }
}

impl<const N: usize> ByteParse for Ascii<N> {
    fn parse(r: &mut ByteReader) -> Option<Self> {
        Some(Ascii(r.take_n()?))
    }
}

pub struct ZeroCopy<T>(pub T);

impl<T: zerocopy::FromBytes> ByteParse for ZeroCopy<T> {
    fn parse(r: &mut ByteReader) -> Option<Self> {
        let data = r.take(std::mem::size_of::<T>())?;
        T::read_from(data).map(ZeroCopy)
    }
}
