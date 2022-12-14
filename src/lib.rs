//!Minimal `no_std` UUID implementation.
//!
//!## Features:
//!
//!- `md5`   - Enables v3;
//!- `orng`  - Enables v4 using OS random, allowing unique UUIDs;
//!- `prng`  - Enables v4 using pseudo random, allowing unique, but predictable UUIDs;
//!- `sha1`  - Enables v5;
//!- `serde` - Enables `serde` support;
//!- `std`   - Enables usages of `std` facilities like getting current time.

#![no_std]
#![warn(missing_docs)]
#![cfg_attr(feature = "cargo-clippy", allow(clippy::style))]

#[cfg(feature = "std")]
extern crate std;

use core::{fmt, time, mem};

#[cfg(feature = "serde")]
mod serde;

type StrBuf = str_buf::StrBuf<36>;
#[repr(transparent)]
///Textual representation of UUID
pub struct TextRepr(str_buf::StrBuf<36>);

impl TextRepr {
    #[inline(always)]
    ///Returns raw bytes
    pub const fn as_bytes(&self) -> &[u8] {
        self.0.as_slice()
    }

    #[inline(always)]
    ///Returns string slice
    pub const fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl core::ops::Deref for TextRepr {
    type Target = str;

    #[inline(always)]
    fn deref(&self) -> &str {
        self.0.as_str()
    }
}

impl PartialEq<TextRepr> for &str {
    #[inline(always)]
    fn eq(&self, other: &TextRepr) -> bool {
        *self == other.as_str()
    }
}

impl PartialEq<TextRepr> for str {
    #[inline(always)]
    fn eq(&self, other: &TextRepr) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<str> for TextRepr {
    #[inline(always)]
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for TextRepr {
    #[inline(always)]
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl fmt::Debug for TextRepr {
    #[inline(always)]
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), fmt)
    }
}

const SEP: u8 = b'-';

#[inline(always)]
const fn byte_to_hex(byt: u8, idx: usize) -> u8 {
    const BASE: usize = 4;
    const BASE_DIGIT: usize = (1 << BASE) - 1;
    const HEX_DIGITS: [u8; 16] = [b'0', b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'a', b'b', b'c', b'd', b'e', b'f'];

    HEX_DIGITS[((byt as usize) >> (BASE * idx)) & BASE_DIGIT]
}

#[inline]
const fn hex_to_byte(hex: &[u8], cursor: usize) -> Result<u8, ParseError> {
    let left = match hex[cursor] {
        chr @ b'0'..=b'9' => chr - b'0',
        chr @ b'a'..=b'f' => chr - b'a' + 10,
        chr @ b'A'..=b'F' => chr - b'A' + 10,
        chr => return Err(ParseError::InvalidByte(chr, cursor)),
    };

    let right = match hex[cursor + 1] {
        chr @ b'0'..=b'9' => chr - b'0',
        chr @ b'a'..=b'f' => chr - b'a' + 10,
        chr @ b'A'..=b'F' => chr - b'A' + 10,
        chr => return Err(ParseError::InvalidByte(chr, cursor + 1)),
    };

    Ok(left * 16 + right)
}

macro_rules! hex_to_byte_try {
    ($bytes:expr, $cursor:expr) => {
        match hex_to_byte($bytes, $cursor) {
            Ok(result) => result,
            Err(error) => return Err(error),
        }
    }
}

///When this namespace is specified, the name string is a fully-qualified domain name
pub const NAMESPACE_DNS: Uuid = Uuid::from_bytes([
     0x6b, 0xa7, 0xb8, 0x10, 0x9d, 0xad, 0x11, 0xd1, 0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30, 0xc8
]);

///When this namespace is specified, the name string is a URL
pub const NAMESPACE_URL: Uuid = Uuid::from_bytes([
    0x6b, 0xa7, 0xb8, 0x11, 0x9d, 0xad, 0x11, 0xd1, 0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30, 0xc8
]);

///When this namespace is specified, the name string is an ISO OID
pub const NAMESPACE_OID: Uuid = Uuid::from_bytes([
    0x6b, 0xa7, 0xb8, 0x12, 0x9d, 0xad, 0x11, 0xd1, 0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30, 0xc8
]);

///When this namespace is specified, the name string is an X.500 DN in DER or a text output format.
pub const NAMESPACE_X500: Uuid = Uuid::from_bytes([
    0x6b, 0xa7, 0xb8, 0x14, 0x9d, 0xad, 0x11, 0xd1, 0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30, 0xc8
]);

/// The version of the UUID, denoting the generating algorithm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Version {
    /// Special case for `nil` UUID.
    Nil = 0,
    /// Version 1: MAC address.
    Mac,
    /// Version 2: DCE Security.
    Dce,
    /// Version 3: MD5 hash.
    Md5,
    /// Version 4: Random.
    Random,
    /// Version 5: SHA-1 hash.
    Sha1,
}

#[derive(Clone, Debug, Copy)]
///Timestamp for use with `v1` algorithm.
pub struct Timestamp {
    ticks: u64,
    counter: u16
}

const V1_NS_TICKS: u64 = 0x01B2_1DD2_1381_4000;

impl Timestamp {
    #[inline(always)]
    ///Creates timestamp from raw parts, as per RFC4122.
    ///
    ///- `ticks` is number of 100-nanoseconds intervals elapsed since 15 Oct 1582 00:00:00.00.
    ///- `counter` is value used to differentiate between timestamps generated to avoid collision
    ///in case of rapid generation.
    pub const fn from_parts(ticks: u64, counter: u16) -> Self {
        Self {
            ticks,
            counter,
        }
    }

    #[inline]
    ///Creates instance from unix timestamp, namely it takes seconds and subsec_nanos.
    ///
    ///Note it doesn't set counter, if needed it must be set manually
    pub const fn from_unix(time: time::Duration) -> Self {
        let ticks = V1_NS_TICKS + time.as_secs() * 10_000_000 + (time.subsec_nanos() as u64) / 100;
        Self::from_parts(ticks, 0)
    }

    #[cfg(feature = "std")]
    #[inline]
    ///Creates instance using current time, namely calculating duration since epoch.
    ///
    ///Note it doesn't set counter, if needed it must be set manually
    ///
    ///Only available when `std` feature is enabled.
    pub fn now() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now().duration_since(UNIX_EPOCH).expect("System time is behind unix epoch");
        Self::from_unix(now)
    }

    #[inline]
    ///Sets counter to further avoid chance of collision between timestamps.
    ///
    ///Useful if clock is not guaranteed to be monotonically increasing.
    ///Otherwise there is no benefit in setting the counter.
    pub const fn set_counter(mut self, counter: u16) -> Self {
        self.counter = counter;
        self
    }

    #[inline(always)]
    ///Retrieves timestamp as raw parts
    pub const fn into_parts(self) -> (u64, u16) {
        (self.ticks, self.counter)
    }
}

const UUID_SIZE: usize = 16;

#[derive(Clone, Copy, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[repr(transparent)]
///Universally unique identifier, consisting of 128-bits, as according to RFC4122
pub struct Uuid {
    data: [u8; UUID_SIZE]
}

impl Uuid {
    #[inline]
    ///Creates zero UUID
    pub const fn nil() -> Self {
        Self::from_bytes([0; UUID_SIZE])
    }

    #[inline]
    ///Creates new Uuid from raw bytes.
    pub const fn from_bytes(data: [u8; UUID_SIZE]) -> Self {
        Self { data }
    }

    #[inline]
    ///Creates new Uuid from byte slice, if its size is 16, otherwise `None`
    pub const fn from_slice(data: &[u8]) -> Option<Uuid> {
        #[cold]
        const fn fail() -> Option<Uuid> {
            None
        }

        if data.len() != UUID_SIZE {
            return fail();
        }

        Some(Self::from_bytes([
            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
            data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15],
        ]))
    }

    #[inline]
    ///Creates `UUID` from `GUID` converting integer fields to big endian while `d4` is copied as
    ///it is.
    ///
    ///Note that it is assumed that `GUID` was correctly created in little endian format (e.g.
    ///using winapi `CoCreateGuid` which internally uses `UuidCreate` which in turn relies on
    ///secure random)
    ///Effectively winapi `GUID` would result in creation of `UUID` of `Random` variant.
    pub const fn from_guid(d1: u32, d2: u16, d3: u16, d4: [u8; 8]) -> Self {
        let d1 = d1.to_be_bytes();
        let d2 = d2.to_be_bytes();
        let d3 = d3.to_be_bytes();
        Self::from_bytes([
            d1[0], d1[1], d1[2], d1[3],
            d2[0], d2[1],
            d3[0], d3[1],
            d4[0],
            d4[1],
            d4[2],
            d4[3],
            d4[4],
            d4[5],
            d4[6],
            d4[7],
        ])
    }

    #[inline]
    ///Access underlying bytes as slice.
    pub const fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    #[inline]
    ///Get underlying raw bytes
    pub const fn bytes(&self) -> [u8; UUID_SIZE] {
        self.data
    }

    #[inline]
    ///Returns `time_low` part of uuid.
    ///
    ///Refer to https://en.wikipedia.org/wiki/Universally_unique_identifier#Format
    pub const fn time_low(&self) -> u32 {
        u32::from_ne_bytes([self.data[0], self.data[1], self.data[2], self.data[3]])
    }

    #[inline]
    ///Returns `time_mid` part of uuid.
    ///
    ///Refer to https://en.wikipedia.org/wiki/Universally_unique_identifier#Format
    pub const fn time_mid(&self) -> u16 {
        u16::from_ne_bytes([self.data[4], self.data[5]])
    }

    #[inline]
    ///Returns `time_high_and_version` part of uuid.
    ///
    ///Refer to https://en.wikipedia.org/wiki/Universally_unique_identifier#Format
    pub const fn time_high_and_version(&self) -> u16 {
        u16::from_ne_bytes([self.data[6], self.data[7]])
    }

    #[inline]
    ///Returns `clock_sequence` part of uuid.
    ///
    ///Refer to https://en.wikipedia.org/wiki/Universally_unique_identifier#Format
    pub const fn clock_sequence(&self) -> u16 {
        u16::from_ne_bytes([self.data[8], self.data[9]])
    }

    #[inline]
    ///Returns `node` part of uuid.
    ///
    ///Refer to https://en.wikipedia.org/wiki/Universally_unique_identifier#Format
    pub const fn node(&self) -> [u8; 6] {
        [self.data[10], self.data[11], self.data[12], self.data[13], self.data[14], self.data[15]]
    }

    #[inline]
    ///Checks if `UUID` version is equal to the provided `version`
    pub const fn is_version(&self, version: Version) -> bool {
        (self.data[6] >> 4) == version as u8
    }

    #[inline]
    ///Checks if `UUID` variant is set, only cares about RFC4122 byte
    pub const fn is_variant(&self) -> bool {
        (self.data[8] & 0xc0) == 0x80
    }

    ///Generates UUID from time and mac address
    pub const fn v1(timestamp: Timestamp, mac: [u8; 6]) -> Self {
        let time_low = (timestamp.ticks & 0xFFFF_FFFF) as u32;
        let time_mid = ((timestamp.ticks >> 32) & 0xFFFF) as u16;
        let time_high_and_version = (((timestamp.ticks >> 48) & 0x0FFF) as u16) | (1 << 12);

        Self::from_bytes([
            (time_low >> 24) as u8,
            (time_low >> 16) as u8,
            (time_low >> 8) as u8,
            time_low as u8,
            (time_mid >> 8) as u8,
            time_mid as u8,
            (time_high_and_version >> 8) as u8,
            time_high_and_version as u8,
            (((timestamp.counter & 0x3F00) >> 8) as u8) | 0x80,
            (timestamp.counter & 0xFF) as u8,
            mac[0],
            mac[1],
            mac[2],
            mac[3],
            mac[4],
            mac[5]
        ])
    }

    #[cfg(feature = "md5")]
    ///Generates UUID `v3` by using `md5` hasher
    ///
    ///Only available when `md5` feature is enabled.
    pub const fn v3(namespace: Uuid, name: &[u8]) -> Self {
        let hash = lhash::Md5::new().const_update(&namespace.data)
                                    .const_update(name)
                                    .const_result();

        Self::from_bytes([
            hash[0], hash[1], hash[2], hash[3], hash[4], hash[5], hash[6], hash[7],
            hash[8], hash[9], hash[10], hash[11], hash[12], hash[13], hash[14], hash[15],
        ]).set_variant().set_version(Version::Md5)
    }

    #[inline]
    ///Constructs UUID `v4` from provided bytes, assuming they are random.
    ///
    ///It is up to user to guarantee that it is random.
    ///
    ///This function only sets corresponding bits for `v4`
    pub const fn v4_from(random: [u8; UUID_SIZE]) -> Self {
        Self::from_bytes(random).set_variant().set_version(Version::Random)
    }

    #[cfg(feature = "osrng")]
    ///Generates UUID `v4` using OS RNG from [getrandom](https://crates.io/crates/getrandom)
    ///
    ///Only available when `osrng` feature is enabled.
    pub fn v4() -> Self {
        let mut bytes = [0; UUID_SIZE];
        if let Err(error) = getrandom::getrandom(&mut bytes[..]) {
            panic!("OS RNG is not available for use: {}", error)
        }

        Self::v4_from(bytes)
    }

    #[cfg(feature = "prng")]
    ///Generates UUID `v4` using PRNG from [wyhash](https://crates.io/crates/wy)
    ///
    ///Only available when `prng` feature is enabled.
    ///
    ///This random variant generates predictable UUID, even though they are unique.
    ///Which means that each time program starts, it is initialized with the same seed and
    ///therefore would repeat UUIDs
    ///
    ///This random is useful when you want to generate predictable but unique UUIDs
    ///Otherwise use `v4`
    pub fn v4_prng() -> Self {
        static RANDOM: squares_rnd::Rand = squares_rnd::Rand::new(1);
        let right = u128::from(RANDOM.next_u64());
        let left = u128::from(RANDOM.next_u64());
        Self::v4_from(((left << 64) |  right).to_ne_bytes())
    }

    #[cfg(feature = "sha1")]
    ///Generates UUID `v5` by using `sha1` hasher
    ///
    ///Only available when `sha1` feature is enabled.
    pub const fn v5(namespace: Uuid, name: &[u8]) -> Self {
        let sha1 = lhash::Sha1::new().const_update(&namespace.data)
                                     .const_update(name)
                                     .const_result();

        Self::from_bytes([
            sha1[0], sha1[1], sha1[2], sha1[3], sha1[4], sha1[5], sha1[6], sha1[7],
            sha1[8], sha1[9], sha1[10], sha1[11], sha1[12], sha1[13], sha1[14], sha1[15],
        ]).set_variant().set_version(Version::Sha1)
    }

    #[inline]
    ///Adds variant byte to the corresponding field.
    ///
    ///This implementation only cares about RFC4122, there is no option to set other variant.
    ///
    ///Useful when user is supplied with random bytes, and wants to create UUID from it.
    pub const fn set_variant(mut self) -> Self {
        self.data[8] = (self.data[8] & 0x3f) | 0x80;
        self
    }

    #[inline]
    ///Adds version byte to the corresponding field.
    ///
    ///Useful when user is supplied with random bytes, and wants to create UUID from it.
    pub const fn set_version(mut self, version: Version) -> Self {
        self.data[6] = (self.data[6] & 0x0f) | ((version as u8) << 4);
        self
    }

    ///Creates new instance by parsing provided bytes.
    ///
    ///Use this when you want to avoid performing utf-8 checks and directly feed bytes.
    ///As long as supplied bytes contain valid ascii characters it will parse successfully.
    ///Otherwise it shall fail with invalid character.
    ///
    ///Supports only simple sequence of characters and `-` separated.
    pub const fn parse_ascii_bytes(input: &[u8]) -> Result<Self, ParseError> {
        if input.len() == StrBuf::capacity() {
            if input[8] != SEP {
                return Err(ParseError::InvalidGroup(1));
            } else if input[13] != SEP {
                return Err(ParseError::InvalidGroup(2));
            } else if input[18] != SEP {
                return Err(ParseError::InvalidGroup(3));
            } else if input[23] != SEP {
                return Err(ParseError::InvalidGroup(4));
            }

            Ok(Self::from_bytes([
                hex_to_byte_try!(input, 0),
                hex_to_byte_try!(input, 2),
                hex_to_byte_try!(input, 4),
                hex_to_byte_try!(input, 6),
                //+1 for `-`
                hex_to_byte_try!(input, 8 + 1),
                hex_to_byte_try!(input, 10 + 1),
                //+1 for `-`
                hex_to_byte_try!(input, 12 + 2),
                hex_to_byte_try!(input, 14 + 2),
                //+1 for `-`
                hex_to_byte_try!(input, 16 + 3),
                hex_to_byte_try!(input, 18 + 3),
                //+1 for `-`
                hex_to_byte_try!(input, 20 + 4),
                hex_to_byte_try!(input, 22 + 4),
                hex_to_byte_try!(input, 24 + 4),
                hex_to_byte_try!(input, 26 + 4),
                hex_to_byte_try!(input, 28 + 4),
                hex_to_byte_try!(input, 30 + 4),
            ]))
        } else if input.len() == StrBuf::capacity() - 4 {
            Ok(Self::from_bytes([
                hex_to_byte_try!(input, 0),
                hex_to_byte_try!(input, 2),
                hex_to_byte_try!(input, 4),
                hex_to_byte_try!(input, 6),
                hex_to_byte_try!(input, 8),
                hex_to_byte_try!(input, 10),
                hex_to_byte_try!(input, 12),
                hex_to_byte_try!(input, 14),
                hex_to_byte_try!(input, 16),
                hex_to_byte_try!(input, 18),
                hex_to_byte_try!(input, 20),
                hex_to_byte_try!(input, 22),
                hex_to_byte_try!(input, 24),
                hex_to_byte_try!(input, 26),
                hex_to_byte_try!(input, 28),
                hex_to_byte_try!(input, 30),
            ]))
        } else {
            Err(ParseError::InvalidLength(input.len()))
        }
    }

    #[inline(always)]
    ///Creates new instance by parsing provided string.
    ///
    ///Supports only simple sequence of characters and `-` separated.
    pub const fn parse_str(input: &str) -> Result<Self, ParseError> {
        Self::parse_ascii_bytes(input.as_bytes())
    }

    #[inline]
    ///Creates textual representation of UUID in a static buffer.
    pub const fn to_str(&self) -> TextRepr {
        let storage = [
            mem::MaybeUninit::new(byte_to_hex(self.data[0], 1)),
            mem::MaybeUninit::new(byte_to_hex(self.data[0], 0)),
            mem::MaybeUninit::new(byte_to_hex(self.data[1], 1)),
            mem::MaybeUninit::new(byte_to_hex(self.data[1], 0)),
            mem::MaybeUninit::new(byte_to_hex(self.data[2], 1)),
            mem::MaybeUninit::new(byte_to_hex(self.data[2], 0)),
            mem::MaybeUninit::new(byte_to_hex(self.data[3], 1)),
            mem::MaybeUninit::new(byte_to_hex(self.data[3], 0)),
            mem::MaybeUninit::new(SEP),
            mem::MaybeUninit::new(byte_to_hex(self.data[4], 1)),
            mem::MaybeUninit::new(byte_to_hex(self.data[4], 0)),
            mem::MaybeUninit::new(byte_to_hex(self.data[5], 1)),
            mem::MaybeUninit::new(byte_to_hex(self.data[5], 0)),
            mem::MaybeUninit::new(SEP),
            mem::MaybeUninit::new(byte_to_hex(self.data[6], 1)),
            mem::MaybeUninit::new(byte_to_hex(self.data[6], 0)),
            mem::MaybeUninit::new(byte_to_hex(self.data[7], 1)),
            mem::MaybeUninit::new(byte_to_hex(self.data[7], 0)),
            mem::MaybeUninit::new(SEP),
            mem::MaybeUninit::new(byte_to_hex(self.data[8], 1)),
            mem::MaybeUninit::new(byte_to_hex(self.data[8], 0)),
            mem::MaybeUninit::new(byte_to_hex(self.data[9], 1)),
            mem::MaybeUninit::new(byte_to_hex(self.data[9], 0)),
            mem::MaybeUninit::new(SEP),
            mem::MaybeUninit::new(byte_to_hex(self.data[10], 1)),
            mem::MaybeUninit::new(byte_to_hex(self.data[10], 0)),
            mem::MaybeUninit::new(byte_to_hex(self.data[11], 1)),
            mem::MaybeUninit::new(byte_to_hex(self.data[11], 0)),
            mem::MaybeUninit::new(byte_to_hex(self.data[12], 1)),
            mem::MaybeUninit::new(byte_to_hex(self.data[12], 0)),
            mem::MaybeUninit::new(byte_to_hex(self.data[13], 1)),
            mem::MaybeUninit::new(byte_to_hex(self.data[13], 0)),
            mem::MaybeUninit::new(byte_to_hex(self.data[14], 1)),
            mem::MaybeUninit::new(byte_to_hex(self.data[14], 0)),
            mem::MaybeUninit::new(byte_to_hex(self.data[15], 1)),
            mem::MaybeUninit::new(byte_to_hex(self.data[15], 0)),
        ];

        unsafe {
            TextRepr(StrBuf::from_storage(storage, StrBuf::capacity() as u8))
        }
    }
}

impl fmt::Debug for Uuid {
    #[inline(always)]
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.write_str(self.to_str().as_str())
    }
}

impl fmt::Display for Uuid {
    #[inline(always)]
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.write_str(self.to_str().as_str())
    }
}

impl Default for Uuid {
    #[inline(always)]
    fn default() -> Self {
        Self::nil()
    }
}

impl AsRef<[u8]> for Uuid {
    #[inline(always)]
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl From<[u8; UUID_SIZE]> for Uuid {
    #[inline(always)]
    fn from(bytes: [u8; UUID_SIZE]) -> Self {
        Self::from_bytes(bytes)
    }
}

impl core::str::FromStr for Uuid {
    type Err = ParseError;

    #[inline(always)]
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::parse_ascii_bytes(input.as_bytes())
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
///Error happening when parsing invalid uuid.
pub enum ParseError {
    ///Input has invalid length.
    InvalidLength(usize),
    ///Groups is invalid
    ///
    ///1. Group number;
    InvalidGroup(u8),
    ///Group has invalid len.
    ///
    ///1. Group number;
    ///3. Actual len;
    InvalidGroupLen(u8, usize),
    ///Invalid character is encountered.
    ///
    ///1. Character byte;
    ///2. Position from 0;
    InvalidByte(u8, usize)
}

impl fmt::Display for ParseError {
    #[inline(always)]
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::InvalidLength(len) => fmt.write_fmt(format_args!("Invalid length {}", len)),
            ParseError::InvalidGroup(idx) => fmt.write_fmt(format_args!("Group {} has unexpected length", idx)),
            ParseError::InvalidGroupLen(idx, len) => fmt.write_fmt(format_args!("Group {} has unexpected length {}", idx, len)),
            ParseError::InvalidByte(byte, pos) => fmt.write_fmt(format_args!("Invalid character '{:x}' at position {}", byte, pos)),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::byte_to_hex;

    #[test]
    fn should_convert_byte_to_hex() {
        assert_eq!([byte_to_hex(254, 1), byte_to_hex(254, 0)], *b"fe");
        assert_eq!([byte_to_hex(255, 1), byte_to_hex(255, 0)], *b"ff");
        assert_eq!([byte_to_hex(1, 1), byte_to_hex(1, 0)], *b"01");
        assert_eq!([byte_to_hex(15, 1), byte_to_hex(15, 0)], *b"0f");
        assert_eq!([byte_to_hex(0, 1), byte_to_hex(0, 0)], *b"00");
    }
}
