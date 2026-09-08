pub mod error;

use std::io::{self, Error, ErrorKind};

use uuid::Uuid;

use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Clientbound,
    Serverbound,
}

pub trait PacketInfo {
    const ID: u32;
    const DIRECTION: Direction;
}

pub struct PacketReader<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> PacketReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    pub fn position(&self) -> usize {
        self.position
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.position)
    }

    pub fn is_finished(&self) -> bool {
        self.position == self.data.len()
    }

    pub fn read_u8(&mut self) -> Result<u8> {
        let byte = *self
            .data
            .get(self.position)
            .ok_or(Error::from(io::ErrorKind::UnexpectedEof))?;
        self.position += 1;
        Ok(byte)
    }

    pub fn read_i8(&mut self) -> Result<i8> {
        Ok(self.read_u8()? as i8)
    }

    pub fn read_u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.read_bytes(2)?.try_into().unwrap()))
    }

    pub fn read_i16(&mut self) -> Result<i16> {
        Ok(i16::from_be_bytes(self.read_bytes(2)?.try_into().unwrap()))
    }

    pub fn read_u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.read_bytes(4)?.try_into().unwrap()))
    }

    pub fn read_i32(&mut self) -> Result<i32> {
        Ok(i32::from_be_bytes(self.read_bytes(4)?.try_into().unwrap()))
    }

    pub fn read_u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.read_bytes(8)?.try_into().unwrap()))
    }

    pub fn read_i64(&mut self) -> Result<i64> {
        Ok(i64::from_be_bytes(self.read_bytes(8)?.try_into().unwrap()))
    }

    pub fn read_u128(&mut self) -> Result<u128> {
        Ok(u128::from_be_bytes(self.read_bytes(16)?.try_into().unwrap()))
    }

    pub fn read_i128(&mut self) -> Result<i128> {
        Ok(i128::from_be_bytes(self.read_bytes(16)?.try_into().unwrap()))
    }

    pub fn read_f32(&mut self) -> Result<f32> {
        Ok(f32::from_be_bytes(self.read_bytes(4)?.try_into().unwrap()))
    }

    pub fn read_f64(&mut self) -> Result<f64> {
        Ok(f64::from_be_bytes(self.read_bytes(8)?.try_into().unwrap()))
    }

    pub fn read_bool(&mut self) -> Result<bool> {
        match self.read_u8()? {
            0 => Ok(false),
            1 => Ok(true),
            value => Err(io::Error::new(
                ErrorKind::InvalidData,
                format!("invalid boolean value: {value}"),
            )
            .into()),
        }
    }

    pub fn read_string(&mut self) -> Result<String> {
        let len = self.read_u32()? as usize;
        let bytes = self.read_bytes(len)?;

        String::from_utf8(bytes.to_vec()).map_err(|error| {
            io::Error::new(ErrorKind::InvalidData, format!("invalid UTF-8 string: {error}"))
                .into()
        })
    }

    pub fn read_bytes(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self.position.checked_add(count).ok_or_else(|| {
            io::Error::new(ErrorKind::InvalidData, "packet read position overflow")
        })?;

        let bytes = self
            .data
            .get(self.position..end)
            .ok_or(Error::from(io::ErrorKind::UnexpectedEof))?;

        self.position = end;
        Ok(bytes)
    }

    pub fn read_byte_array(&mut self) -> Result<Vec<u8>> {
        let len = self.read_u32()? as usize;
        Ok(self.read_bytes(len)?.to_vec())
    }
    
    fn read_uuid(&mut self) -> Result<Uuid> {
        let bytes = self.read_bytes(16)?;
        Uuid::from_slice(bytes).map_err(|error| {
            io::Error::new(ErrorKind::InvalidData, format!("invalid UUID: {error}"))
                .into()
        })
    }
}

pub struct PacketWriter {
    data: Vec<u8>,
}

impl PacketWriter {
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: Vec::with_capacity(capacity),
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn write_u8(&mut self, value: u8) {
        self.data.push(value);
    }

    pub fn write_i8(&mut self, value: i8) {
        self.data.push(value as u8);
    }

    pub fn write_u16(&mut self, value: u16) {
        self.data.extend_from_slice(&value.to_be_bytes());
    }

    pub fn write_i16(&mut self, value: i16) {
        self.data.extend_from_slice(&value.to_be_bytes());
    }

    pub fn write_u32(&mut self, value: u32) {
        self.data.extend_from_slice(&value.to_be_bytes());
    }

    pub fn write_i32(&mut self, value: i32) {
        self.data.extend_from_slice(&value.to_be_bytes());
    }

    pub fn write_u64(&mut self, value: u64) {
        self.data.extend_from_slice(&value.to_be_bytes());
    }

    pub fn write_i64(&mut self, value: i64) {
        self.data.extend_from_slice(&value.to_be_bytes());
    }

    pub fn write_u128(&mut self, value: u128) {
        self.data.extend_from_slice(&value.to_be_bytes());
    }

    pub fn write_i128(&mut self, value: i128) {
        self.data.extend_from_slice(&value.to_be_bytes());
    }

    pub fn write_f32(&mut self, value: f32) {
        self.data.extend_from_slice(&value.to_be_bytes());
    }

    pub fn write_f64(&mut self, value: f64) {
        self.data.extend_from_slice(&value.to_be_bytes());
    }

    pub fn write_bool(&mut self, value: bool) {
        self.write_u8(value as u8);
    }

    pub fn write_string(&mut self, value: &str) -> Result<()> {
        let bytes = value.as_bytes();
        let len = u32::try_from(bytes.len()).map_err(|_| {
            io::Error::new(ErrorKind::InvalidInput, "string is too large to encode")
        })?;

        self.write_u32(len);
        self.write_bytes(bytes);
        Ok(())
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) {
        self.data.extend_from_slice(bytes);
    }

    pub fn write_byte_array(&mut self, bytes: &[u8]) -> Result<()> {
        let len = u32::try_from(bytes.len()).map_err(|_| {
            io::Error::new(ErrorKind::InvalidInput, "byte array is too large to encode")
        })?;

        self.write_u32(len);
        self.write_bytes(bytes);
        Ok(())
    }

    pub fn into_inner(self) -> Vec<u8> {
        self.data
    }
    
    fn write_uuid(&mut self, arg: Uuid) -> Result<()> {
        self.write_bytes(arg.as_bytes());
        Ok(())
    }
}

impl Default for PacketWriter {
    fn default() -> Self {
        Self::new()
    }
}

pub trait PacketField: Sized {
    fn decode(reader: &mut PacketReader<'_>) -> Result<Self>;
    fn encode(&self, writer: &mut PacketWriter) -> Result<()>;
}

macro_rules! impl_packet_field {
    ($ty:ty, $read:ident, $write:ident) => {
        impl PacketField for $ty {
            fn decode(reader: &mut PacketReader<'_>) -> Result<Self> {
                reader.$read()
            }

            fn encode(&self, writer: &mut PacketWriter) -> Result<()> {
                writer.$write(*self);
                Ok(())
            }
        }
    };
}

impl_packet_field!(u8, read_u8, write_u8);
impl_packet_field!(i8, read_i8, write_i8);
impl_packet_field!(u16, read_u16, write_u16);
impl_packet_field!(i16, read_i16, write_i16);
impl_packet_field!(u32, read_u32, write_u32);
impl_packet_field!(i32, read_i32, write_i32);
impl_packet_field!(u64, read_u64, write_u64);
impl_packet_field!(i64, read_i64, write_i64);
impl_packet_field!(u128, read_u128, write_u128);
impl_packet_field!(i128, read_i128, write_i128);
impl_packet_field!(f32, read_f32, write_f32);
impl_packet_field!(f64, read_f64, write_f64);
impl_packet_field!(bool, read_bool, write_bool);

impl PacketField for String {
    fn decode(reader: &mut PacketReader<'_>) -> Result<Self> {
        reader.read_string()
    }

    fn encode(&self, writer: &mut PacketWriter) -> Result<()> {
        writer.write_string(self)
    }
}


impl<T: PacketField> PacketField for Option<T> {
    fn decode(reader: &mut PacketReader<'_>) -> Result<Self> {
        match reader.read_u8()? {
            0 => Ok(None),
            1 => Ok(Some(T::decode(reader)?)),
            value => Err(io::Error::new(
                ErrorKind::InvalidData,
                format!("invalid Option tag: {value}"),
            )
            .into()),
        }
    }

    fn encode(&self, writer: &mut PacketWriter) -> Result<()> {
        match self {
            None => {
                writer.write_u8(0);
                Ok(())
            }
            Some(value) => {
                writer.write_u8(1);
                value.encode(writer)
            }
        }
    }
}

impl<T: PacketField> PacketField for Vec<T> {
    fn decode(reader: &mut PacketReader<'_>) -> Result<Self> {
        let len = reader.read_u32()? as usize;
        let mut values = Vec::with_capacity(len);

        for _ in 0..len {
            values.push(T::decode(reader)?);
        }

        Ok(values)
    }

    fn encode(&self, writer: &mut PacketWriter) -> Result<()> {
        let len = u32::try_from(self.len()).map_err(|_| {
            io::Error::new(ErrorKind::InvalidInput, "vector is too large to encode")
        })?;

        writer.write_u32(len);
        for value in self {
            value.encode(writer)?;
        }

        Ok(())
    }
}

impl<T: PacketField, const N: usize> PacketField for [T; N] {
    fn decode(reader: &mut PacketReader<'_>) -> Result<Self> {
        let mut array: [T; N] = std::array::from_fn(|_| T::decode(reader).unwrap());
        for i in 0..N {
            array[i] = T::decode(reader)?;
        }
        Ok(array)
    }

    fn encode(&self, writer: &mut PacketWriter) -> Result<()> {
        for value in self {
            value.encode(writer)?;
        }
        Ok(())
    }
}

impl<T: PacketField, U: PacketField> PacketField for (T, U) {
    fn decode(reader: &mut PacketReader<'_>) -> Result<Self> {
        let first = T::decode(reader)?;
        let second = U::decode(reader)?;
        Ok((first, second))
    }

    fn encode(&self, writer: &mut PacketWriter) -> Result<()> {
        self.0.encode(writer)?;
        self.1.encode(writer)?;
        Ok(())
    }
}

impl PacketField for Uuid {
    fn decode(reader: &mut PacketReader<'_>) -> Result<Self> {
        reader.read_uuid()
    }

    fn encode(&self, writer: &mut PacketWriter) -> Result<()> {
        writer.write_uuid(*self)
    }
}

pub trait Packet: PacketInfo + Sized {
    fn id(&self) -> u32;
    fn direction(&self) -> Direction;
    fn decode(reader: &mut PacketReader<'_>) -> Result<Self>;
    fn encode(&self, writer: &mut PacketWriter) -> Result<()>;
}
