use byteorder::WriteBytesExt;
use serde::{ser, ser::SerializeSeq, Serialize};
use std::io::{Seek, Write};
use std::{marker::PhantomData, str};

use crate::utils::*;
use crate::VariantValue;
use crate::{Basic, EncodingFormat};
use crate::{Error, Result};
use crate::{ObjectPath, Signature};

pub struct Serializer<'a, B, W> {
    pub(self) format: EncodingFormat,
    pub(self) write: &'a mut W,
    pub(self) bytes_written: usize,

    pub(self) signature: &'a str,
    pub(self) signature_pos: usize,

    // FIXME: Use ArrayString here?
    pub(self) variant_sign: Option<String>,

    b: PhantomData<B>,
}

impl<'a, B, W> Serializer<'a, B, W>
where
    B: byteorder::ByteOrder,
    W: Write + Seek,
{
    fn next_signature_char(&self) -> Option<char> {
        self.signature.chars().nth(self.signature_pos)
    }

    fn parse_signature_char(&mut self, expected: Option<char>) -> Result<()> {
        if self
            .next_signature_char()
            .map(|c| expected.map(|ec| c != ec).unwrap_or(false))
            .unwrap_or(true)
        {
            // TODO: Better error here with more info
            return Err(Error::IncorrectType);
        }
        self.signature_pos += 1;

        Ok(())
    }

    fn add_padding(&mut self, alignment: usize) -> Result<usize> {
        let padding = padding_for_n_bytes(self.bytes_written, alignment);
        if padding > 0 {
            let byte = [0_u8; 1];
            for _ in 0..padding {
                self.write_all(&byte).map_err(Error::Io)?;
            }
        }

        Ok(padding)
    }

    fn prep_serialize_basic<T>(&mut self) -> Result<()>
    where
        T: Basic,
    {
        self.parse_signature_char(Some(T::SIGNATURE_CHAR))?;
        self.add_padding(T::ALIGNMENT)?;

        Ok(())
    }
}

pub fn to_write<T: ?Sized, B, W>(value: &T, write: &mut W, format: EncodingFormat) -> Result<usize>
where
    T: Serialize + VariantValue,
    B: byteorder::ByteOrder,
    W: Write + Seek,
{
    let signature = T::signature();
    let mut serializer = Serializer::<B, W> {
        format,
        signature: &signature,
        signature_pos: 0,
        write,
        bytes_written: 0,
        variant_sign: None,
        b: PhantomData,
    };
    value.serialize(&mut serializer)?;
    Ok(serializer.bytes_written)
}

// FIXME: to_write() would be better, then to_bytes() can be a think wrapper over it
pub fn to_bytes<T: ?Sized, B>(value: &T, format: EncodingFormat) -> Result<Vec<u8>>
where
    T: Serialize + VariantValue,
    B: byteorder::ByteOrder,
{
    let signature = T::signature();
    let mut cursor = std::io::Cursor::new(vec![]);
    let mut serializer = Serializer::<B, _> {
        format,
        signature: &signature,
        signature_pos: 0,
        write: &mut cursor,
        bytes_written: 0,
        variant_sign: None,
        b: PhantomData,
    };
    value.serialize(&mut serializer)?;
    Ok(cursor.into_inner())
}

impl<'a, B, W> Write for Serializer<'a, B, W>
where
    B: byteorder::ByteOrder,
    W: Write + Seek,
{
    /// Write `buf` and increment `bytes_written` field.
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.write.write(buf).map(|n| {
            self.bytes_written += n;

            n
        })
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.write.flush()
    }
}

impl<'a, 'b, B, W> ser::Serializer for &'b mut Serializer<'a, B, W>
where
    B: byteorder::ByteOrder,
    W: Write + Seek,
{
    type Ok = ();
    type Error = Error;

    type SerializeSeq = SeqSerializer<'a, 'b, B, W>;
    type SerializeTuple = StructSerializer<'a, 'b, B, W>;
    type SerializeTupleStruct = StructSerializer<'a, 'b, B, W>;
    type SerializeTupleVariant = StructSerializer<'a, 'b, B, W>;
    type SerializeMap = SeqSerializer<'a, 'b, B, W>;
    type SerializeStruct = StructSerializer<'a, 'b, B, W>;
    type SerializeStructVariant = StructSerializer<'a, 'b, B, W>;

    fn serialize_bool(self, v: bool) -> Result<()> {
        self.prep_serialize_basic::<bool>()?;
        self.write_u32::<B>(v as u32).map_err(Error::Io)
    }

    fn serialize_i8(self, v: i8) -> Result<()> {
        // No i8 type in D-Bus/GVariant, let's pretend it's i16
        self.write_i16::<B>(v as i16).map_err(Error::Io)
    }

    fn serialize_i16(self, v: i16) -> Result<()> {
        self.prep_serialize_basic::<i16>()?;
        self.write_i16::<B>(v).map_err(Error::Io)
    }

    fn serialize_i32(self, v: i32) -> Result<()> {
        self.prep_serialize_basic::<i32>()?;
        self.write_i32::<B>(v).map_err(Error::Io)
    }

    fn serialize_i64(self, v: i64) -> Result<()> {
        self.prep_serialize_basic::<i64>()?;
        self.write_i64::<B>(v).map_err(Error::Io)
    }

    fn serialize_u8(self, v: u8) -> Result<()> {
        self.prep_serialize_basic::<u8>()?;
        // Endianness is irrelevant for single bytes.
        self.write_u8(v).map_err(Error::Io)
    }

    fn serialize_u16(self, v: u16) -> Result<()> {
        self.prep_serialize_basic::<u16>()?;
        self.write_u16::<B>(v).map_err(Error::Io)
    }

    fn serialize_u32(self, v: u32) -> Result<()> {
        self.prep_serialize_basic::<u32>()?;
        self.write_u32::<B>(v).map_err(Error::Io)
    }

    fn serialize_u64(self, v: u64) -> Result<()> {
        self.prep_serialize_basic::<u64>()?;
        self.write_u64::<B>(v).map_err(Error::Io)
    }

    fn serialize_f32(self, v: f32) -> Result<()> {
        // No f32 type in D-Bus/GVariant, let's pretend it's f64
        self.serialize_f64(v as f64)
    }

    fn serialize_f64(self, v: f64) -> Result<()> {
        self.prep_serialize_basic::<f64>()?;
        self.write_f64::<B>(v).map_err(Error::Io)
    }

    fn serialize_char(self, v: char) -> Result<()> {
        // No char type in D-Bus/GVariant, let's pretend it's a string
        self.serialize_str(&v.to_string())
    }

    fn serialize_str(self, v: &str) -> Result<()> {
        match self.next_signature_char() {
            Some(ObjectPath::SIGNATURE_CHAR) | Some(<&str>::SIGNATURE_CHAR) => {
                self.add_padding(<&str>::ALIGNMENT)?;
                self.write_u32::<B>(usize_to_u32(v.len()))
                    .map_err(Error::Io)?;
            }
            Some(c) if c == Signature::SIGNATURE_CHAR || c == VARIANT_SIGNATURE_CHAR => {
                self.write_u8(usize_to_u8(v.len())).map_err(Error::Io)?;

                if c == VARIANT_SIGNATURE_CHAR {
                    self.variant_sign = Some(String::from(v));
                }
            }
            _ => {
                // TODO: Better error here with more info
                return Err(Error::IncorrectType);
            }
        }
        self.parse_signature_char(None)?;
        self.write_all(&v.as_bytes()).map_err(Error::Io)?;
        self.write_all(&b"\0"[..]).map_err(Error::Io)?;

        Ok(())
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<()> {
        let mut seq = self.serialize_seq(Some(v.len()))?;
        for byte in v {
            seq.serialize_element(byte)?;
        }

        seq.end()
    }

    fn serialize_none(self) -> Result<()> {
        // FIXME: Corresponds to GVariant's `Maybe` type, which is empty (no bytes) for None.
        todo!();
    }

    fn serialize_some<T>(self, _value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        // FIXME: Corresponds to GVariant's `Maybe` type.
        todo!();
    }

    // FIXME: What am i supposed to do with this strange type?
    fn serialize_unit(self) -> Result<()> {
        self.serialize_none()
    }

    fn serialize_unit_struct(self, name: &'static str) -> Result<()> {
        // Not sure what else can we do with this?
        self.serialize_str(name)
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
    ) -> Result<()> {
        self.serialize_u32(variant_index)
    }

    fn serialize_newtype_struct<T>(self, _name: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)?;

        Ok(())
    }

    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        value: &T,
    ) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)?;

        Ok(())
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq> {
        self.parse_signature_char(Some(ARRAY_SIGNATURE_CHAR))?;
        self.add_padding(ARRAY_ALIGNMENT)?;
        // Length in bytes (unfortunately not the same as len passed to us here) which we initially
        // set to 0.
        self.write_u32::<B>(0_u32).map_err(Error::Io)?;

        let next_signature_char = self
            .next_signature_char()
            .ok_or_else(|| Error::InvalidSignature(String::from(self.signature)))?;
        let alignment = alignment_for_signature_char(next_signature_char, self.format);
        let start = self.bytes_written;
        // D-Bus expects us to add padding for the first element even when there is no first
        // element (i-e empty array) so we add padding already.
        let first_padding = self.add_padding(alignment)?;
        let element_signature_pos = self.signature_pos;

        Ok(SeqSerializer {
            serializer: self,
            start,
            element_signature_pos,
            first_padding,
        })
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple> {
        self.serialize_struct("", len)
    }

    fn serialize_tuple_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        self.serialize_struct(name, len)
    }

    fn serialize_tuple_variant(
        self,
        name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        self.serialize_struct(name, len)
    }

    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap> {
        self.serialize_seq(len)
    }

    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeStruct> {
        let end_parens = match self.next_signature_char() {
            Some(VARIANT_SIGNATURE_CHAR) => None,
            Some(c) => {
                self.parse_signature_char(Some(c))?;
                self.add_padding(STRUCT_ALIGNMENT)?;

                if c == STRUCT_SIG_START_CHAR {
                    Some(STRUCT_SIG_END_CHAR)
                } else if c == DICT_ENTRY_SIG_START_CHAR {
                    Some(DICT_ENTRY_SIG_END_CHAR)
                } else {
                    return Err(Error::IncorrectType);
                }
            }
            _ => {
                // TODO: Better error here with more info
                return Err(Error::IncorrectType);
            }
        };

        Ok(StructSerializer {
            serializer: self,
            end_parens,
        })
    }

    fn serialize_struct_variant(
        self,
        name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        self.serialize_struct(name, len)
    }
}

// TODO: Put this in a separate file
pub struct SeqSerializer<'a, 'b, B, W> {
    serializer: &'b mut Serializer<'a, B, W>,
    start: usize,
    // where value signature starts
    element_signature_pos: usize,
    // First element's padding
    first_padding: usize,
}

impl<'a, 'b, B, W> SeqSerializer<'a, 'b, B, W>
where
    B: byteorder::ByteOrder,
    W: Write + Seek,
{
    pub(self) fn end_seq(self) -> Result<()> {
        if self.start + self.first_padding == self.serializer.bytes_written {
            // Empty sequence so we need to parse the element signature.
            let rest_of_signature =
                Signature::from(&self.serializer.signature[self.element_signature_pos..]);
            let element_signature = slice_signature(&rest_of_signature)?;
            self.serializer.signature_pos += element_signature.len();
        }

        // Set size of array in bytes
        let array_len = self.serializer.bytes_written - self.start;
        let len = usize_to_u32(array_len - self.first_padding);
        self.serializer
            .write
            .seek(std::io::SeekFrom::End(-(array_len as i64) - 4))
            .map_err(Error::Io)?;
        self.serializer
            .write
            .write_u32::<B>(len)
            .map_err(Error::Io)?;
        self.serializer
            .write
            .seek(std::io::SeekFrom::Current(array_len as i64))
            .map_err(Error::Io)?;

        Ok(())
    }
}

impl<'a, 'b, B, W> ser::SerializeSeq for SeqSerializer<'a, 'b, B, W>
where
    B: byteorder::ByteOrder,
    W: Write + Seek,
{
    type Ok = ();
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        if self.start != self.serializer.bytes_written {
            // The signature needs to be rewinded before encoding each element.
            self.serializer.signature_pos = self.element_signature_pos;
        }
        value.serialize(&mut *self.serializer)
    }

    fn end(self) -> Result<()> {
        self.end_seq()
    }
}

// TODO: Put this in a separate file
pub struct StructSerializer<'a, 'b, B, W> {
    serializer: &'b mut Serializer<'a, B, W>,
    end_parens: Option<char>,
}

impl<'a, 'b, B, W> StructSerializer<'a, 'b, B, W>
where
    B: byteorder::ByteOrder,
    W: Write + Seek,
{
    fn serialize_struct_element<T>(&mut self, name: Option<&'static str>, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        match name {
            Some("zvariant::Variant::Value") => {
                // Serializing the value of a Variant, which means signature was serialized
                // already, and also put aside for us to be picked here.
                let signature = self
                    .serializer
                    .variant_sign
                    .take()
                    // FIXME: Better error?
                    .ok_or_else(|| {
                        Error::InvalidSignature(String::from(self.serializer.signature))
                    })?;

                let mut serializer = Serializer::<B, W> {
                    format: self.serializer.format,
                    signature: &signature,
                    write: &mut self.serializer.write,
                    signature_pos: 0,
                    bytes_written: self.serializer.bytes_written,
                    variant_sign: None,
                    b: PhantomData,
                };
                value.serialize(&mut serializer)?;
                self.serializer.bytes_written = serializer.bytes_written;

                Ok(())
            }
            _ => value.serialize(&mut *self.serializer),
        }
    }

    fn end_struct(self) -> Result<()> {
        if let Some(c) = self.end_parens {
            self.serializer.parse_signature_char(Some(c))?;
        }

        Ok(())
    }
}

impl<'a, 'b, B, W> ser::SerializeTuple for StructSerializer<'a, 'b, B, W>
where
    B: byteorder::ByteOrder,
    W: Write + Seek,
{
    type Ok = ();
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.serialize_struct_element(None, value)
    }

    fn end(self) -> Result<()> {
        self.end_struct()
    }
}

impl<'a, 'b, B, W> ser::SerializeTupleStruct for StructSerializer<'a, 'b, B, W>
where
    B: byteorder::ByteOrder,
    W: Write + Seek,
{
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.serialize_struct_element(None, value)
    }

    fn end(self) -> Result<()> {
        self.end_struct()
    }
}

impl<'a, 'b, B, W> ser::SerializeTupleVariant for StructSerializer<'a, 'b, B, W>
where
    B: byteorder::ByteOrder,
    W: Write + Seek,
{
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.serialize_struct_element(None, value)
    }

    fn end(self) -> Result<()> {
        self.end_struct()
    }
}

impl<'a, 'b, B, W> ser::SerializeMap for SeqSerializer<'a, 'b, B, W>
where
    B: byteorder::ByteOrder,
    W: Write + Seek,
{
    type Ok = ();
    type Error = Error;

    // TODO: The Serde data model allows map keys to be any serializable type. We can only support keys of
    // basic types so the implementation below will produce invalid encoding if the key serializes
    // is something other than a basic type.
    //
    // We need to validate that map keys are of basic type. We do this by using a different Serializer
    // to serialize the key (instead of `&mut **self`) and having that other serializer only implement
    // `serialize_*` for basic types and return an error on any other data type.
    fn serialize_key<T>(&mut self, key: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        if self.start == self.serializer.bytes_written {
            // First key
            self.serializer
                .parse_signature_char(Some(DICT_ENTRY_SIG_START_CHAR))?;
        } else {
            // The signature needs to be rewinded before encoding each element.
            self.serializer.signature_pos = self.element_signature_pos + 1;
        }
        self.serializer.add_padding(DICT_ENTRY_ALIGNMENT)?;

        key.serialize(&mut *self.serializer)
    }

    fn serialize_value<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(&mut *self.serializer)
    }

    fn end(self) -> Result<()> {
        self.end_seq()
    }
}

impl<'a, 'b, B, W> ser::SerializeStruct for StructSerializer<'a, 'b, B, W>
where
    B: byteorder::ByteOrder,
    W: Write + Seek,
{
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.serialize_struct_element(Some(key), value)
    }

    fn end(self) -> Result<()> {
        self.end_struct()
    }
}

impl<'a, 'b, B, W> ser::SerializeStructVariant for StructSerializer<'a, 'b, B, W>
where
    B: byteorder::ByteOrder,
    W: Write + Seek,
{
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.serialize_struct_element(Some(key), value)
    }

    fn end(self) -> Result<()> {
        self.end_struct()
    }
}
