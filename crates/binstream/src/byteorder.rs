use crate::{ByteParse, ByteReader};
use byteorder::ByteOrder;
use std::{
    fmt::{self, Binary, Debug, Display, Formatter, LowerHex, Octal, UpperHex},
    marker::PhantomData,
};
use zerocopy::FromBytes;

pub use zerocopy::byteorder::*;

macro_rules! define {
    ($name:ident, $native:ident, $bytes:expr) => {
        #[derive(Copy, Clone, Eq, PartialEq, Hash, FromBytes)]
        #[repr(transparent)]
        pub struct $name<O>([u8; $bytes], PhantomData<O>);

        impl<O> Default for $name<O> {
            fn default() -> $name<O> {
                $name::ZERO
            }
        }

        impl<O> $name<O> {
            pub const ZERO: $name<O> = $name([0u8; $bytes], PhantomData);

            pub const fn from_bytes(bytes: [u8; $bytes]) -> $name<O> {
                $name(bytes, PhantomData)
            }
        }

        impl<O: ByteOrder> From<$native> for $name<O> {
            fn from(val: $native) -> Self {
                Self::new(val)
            }
        }

        impl<O: ByteOrder> From<$name<O>> for $native {
            fn from(val: $name<O>) -> Self {
                val.get()
            }
        }

        impl<O: ByteOrder> AsRef<[u8; $bytes]> for $name<O> {
            fn as_ref(&self) -> &[u8; $bytes] {
                &self.0
            }
        }

        impl<O: ByteOrder> AsMut<[u8; $bytes]> for $name<O> {
            fn as_mut(&mut self) -> &mut [u8; $bytes] {
                &mut self.0
            }
        }

        impl<O: ByteOrder> Display for $name<O> {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                Display::fmt(&self.get(), f)
            }
        }

        impl<O: ByteOrder> Debug for $name<O> {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.get())
            }
        }
    };
}

macro_rules! define_fixed {
    ( $(($name:ident, $native:ident, $bytes:expr, $read_method:ident, $write_method:ident ))* ) => {$(
        define!($name, $native, $bytes);

        impl<O: ByteOrder> $name<O> {
            pub fn new(n: $native) -> $name<O> {
                let mut out = $name::default();
                O::$write_method(&mut out.0[..], n);
                out
            }

            pub fn get(self) -> $native {
                O::$read_method(&self.0[..])
            }

            pub fn set(&mut self, n: $native) {
                O::$write_method(&mut self.0[..], n);
            }
        }
    )*};
}

macro_rules! define_uint {
    ( $(($name:ident, $native:ident, $bytes:expr ))* ) => {$(
        define!($name, $native, $bytes);

        impl<O: ByteOrder> $name<O> {
            pub fn new(n: $native) -> $name<O> {
                let mut out = $name::default();
                O::write_uint(&mut out.0[..], n, $bytes);
                out
            }

            pub fn get(self) -> $native {
                O::read_uint(&self.0[..], $bytes)
            }

            pub fn set(&mut self, n: $native) {
                O::write_uint(&mut self.0[..], n, $bytes);
            }
        }
    )*};
}

macro_rules! define_int_fmt {
    ( $($name:ident),* ) => {$(
        impl<O: ByteOrder> Octal for $name<O> {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                Octal::fmt(&self.get(), f)
            }
        }

        impl<O: ByteOrder> LowerHex for $name<O> {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                LowerHex::fmt(&self.get(), f)
            }
        }

        impl<O: ByteOrder> UpperHex for $name<O> {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                UpperHex::fmt(&self.get(), f)
            }
        }

        impl<O: ByteOrder> Binary for $name<O> {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                Binary::fmt(&self.get(), f)
            }
        }
    )*};
}

define_fixed! {
    (U24, u32, 3, read_u24, write_u24)
    (I24, i32, 3, read_i24, write_i24)
    (F32, f32, 4, read_f32, write_f32)
    (F64, f64, 8, read_f64, write_f64)
}

define_uint! {
    (U40, u64, 5)
}

define_int_fmt! {
    U24, U40,
    I24
}

macro_rules! impl_with_zerocopy {
    ( $( ($name:ident, $bytes:expr) ),* ) => {$(
        impl<O: ByteOrder> ByteParse for $name<O> {
            fn parse(r: &mut ByteReader) -> Option<Self> {
                let data = r.take_n::<$bytes>()?;
                Self::read_from(data.as_slice())
            }
        }
    )*};
}

impl_with_zerocopy! {
    (U16, 2),
    (U24, 3),
    (U32, 4),
    (U40, 5),
    (U64, 8),
    (U128, 16),
    (I16, 2),
    (I24, 3),
    (I32, 4),
    (I64, 8),
    (I128, 16),
    (F32, 4),
    (F64, 8)
}

#[allow(non_camel_case_types)]
mod aliases {
    use super::*;

    pub type u16_le = U16<LE>;
    pub type u24_le = U24<LE>;
    pub type u32_le = U32<LE>;
    pub type u40_le = U40<LE>;
    pub type u64_le = U64<LE>;
    pub type u128_le = U128<LE>;
    pub type i16_le = I16<LE>;
    pub type i24_le = I24<LE>;
    pub type i32_le = I32<LE>;
    pub type i64_le = I64<LE>;
    pub type i128_le = I128<LE>;
    pub type f32_le = F32<LE>;
    pub type f64_le = F64<LE>;

    pub type u16_be = U16<BE>;
    pub type u24_be = U24<BE>;
    pub type u32_be = U32<BE>;
    pub type u40_be = U40<BE>;
    pub type u64_be = U64<BE>;
    pub type u128_be = U128<BE>;
    pub type i16_be = I16<BE>;
    pub type i24_be = I24<BE>;
    pub type i32_be = I32<BE>;
    pub type i64_be = I64<BE>;
    pub type i128_be = I128<BE>;
    pub type f32_be = F32<BE>;
    pub type f64_be = F64<BE>;
}

pub use aliases::*;
