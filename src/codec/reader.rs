use crate::codec::consts::TarsType;
use crate::codec::error::{Error, Result};
use std::convert::TryFrom;

/// Tars 数据流读取器.
///
/// 直接基于字节切片 (`&[u8]`) 实现。
/// 内部维护了解码深度 (`depth`),以防止恶意的深度嵌套攻击.
pub struct TarsReader<'a> {
    data: &'a [u8],
    pos: usize,
    depth: usize,
}

impl<'a> TarsReader<'a> {
    /// 创建一个新的读取器.
    ///
    /// # 参数
    ///
    /// * `bytes`: 包含 Tars 编码数据的字节切片.
    #[inline]
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            data: bytes,
            pos: 0,
            depth: 0,
        }
    }

    /// 获取当前偏移量.
    #[inline]
    pub fn position(&self) -> u64 {
        self.pos as u64
    }

    /// 检查是否已到达末尾.
    #[inline]
    pub fn is_end(&self) -> bool {
        self.pos >= self.data.len()
    }

    #[inline]
    pub fn remaining(&self) -> &'a [u8] {
        if self.pos < self.data.len() {
            &self.data[self.pos..]
        } else {
            &[]
        }
    }

    #[inline]
    fn ensure_available(&self, required: usize) -> Result<()> {
        let available = self.data.len() - self.pos;
        if available < required {
            Err(Error::buffer_overflow(self.pos, required, available))
        } else {
            Ok(())
        }
    }

    /// 读取字段头部信息 (Tag 和 Type).
    ///
    /// 解析紧接在当前游标位置的头部字节.
    /// 涵盖了单字节头部和双字节头部(当标签 >= 15 时)的处理.
    ///
    /// # 错误
    ///
    /// 如果缓冲区剩余字节不足以解析头部,返回 `Error::BufferOverflow`.
    /// 如果类型 ID 非法 (不在 0-13 范围内),返回 `Error::InvalidType`.
    #[inline]
    pub fn read_head(&mut self) -> Result<(u8, TarsType)> {
        let start_pos = self.pos;
        if self.pos >= self.data.len() {
            return Err(Error::buffer_overflow(start_pos, 1, 0));
        }
        let b = self.data[self.pos];
        self.pos += 1;

        let type_id = b & 0x0F;
        let mut tag = (b & 0xF0) >> 4;

        if tag == 15 {
            if self.pos >= self.data.len() {
                self.pos = start_pos;
                return Err(Error::buffer_overflow(start_pos, 2, 1));
            }
            tag = self.data[self.pos];
            self.pos += 1;
        }

        let tars_type = TarsType::try_from(type_id).map_err(|id| {
            self.pos = start_pos;
            Error::invalid_type(start_pos, id, "0..=13")
        })?;

        Ok((tag, tars_type))
    }

    /// 预览头部信息而不移动指针.
    pub fn peek_head(&mut self) -> Result<(u8, TarsType)> {
        let pos = self.pos;
        let res = self.read_head();
        self.pos = pos;
        res
    }

    /// 读取有符号整数.
    ///
    /// 根据 `type_id` 自动识别整数宽度 (i8/i16/i32/i64),并统一返回 `i64`.
    #[inline]
    pub fn read_int(&mut self, type_id: TarsType) -> Result<i64> {
        let start_pos = self.pos;
        match type_id {
            TarsType::ZeroTag => Ok(0),
            TarsType::Int1 => {
                self.ensure_available(1).map_err(|e| {
                    let mut e = e;
                    if let Error::BufferOverflow { offset, .. } = &mut e {
                        *offset = start_pos;
                    }
                    e
                })?;
                let v = self.data[self.pos] as i8;
                self.pos += 1;
                Ok(v as i64)
            }
            TarsType::Int2 => {
                self.ensure_available(2).map_err(|e| {
                    let mut e = e;
                    if let Error::BufferOverflow { offset, .. } = &mut e {
                        *offset = start_pos;
                    }
                    e
                })?;
                let bytes = self.data[self.pos..self.pos + 2].try_into().unwrap();
                let v = i16::from_be_bytes(bytes);
                self.pos += 2;
                Ok(v as i64)
            }
            TarsType::Int4 => {
                self.ensure_available(4).map_err(|e| {
                    let mut e = e;
                    if let Error::BufferOverflow { offset, .. } = &mut e {
                        *offset = start_pos;
                    }
                    e
                })?;
                let bytes = self.data[self.pos..self.pos + 4].try_into().unwrap();
                let v = i32::from_be_bytes(bytes);
                self.pos += 4;
                Ok(v as i64)
            }
            TarsType::Int8 => {
                self.ensure_available(8).map_err(|e| {
                    let mut e = e;
                    if let Error::BufferOverflow { offset, .. } = &mut e {
                        *offset = start_pos;
                    }
                    e
                })?;
                let bytes = self.data[self.pos..self.pos + 8].try_into().unwrap();
                let v = i64::from_be_bytes(bytes);
                self.pos += 8;
                Ok(v)
            }
            _ => Err(Error::new(
                start_pos,
                format!("Cannot read int from type {:?}", type_id),
            )),
        }
    }

    /// 读取无符号整数(向上转型为 u64).
    #[inline]
    pub fn read_uint(&mut self, type_id: TarsType) -> Result<u64> {
        let start_pos = self.pos;
        match type_id {
            TarsType::ZeroTag => Ok(0),
            TarsType::Int1 => {
                self.ensure_available(1).map_err(|e| {
                    let mut e = e;
                    if let Error::BufferOverflow { offset, .. } = &mut e {
                        *offset = start_pos;
                    }
                    e
                })?;
                let v = self.data[self.pos];
                self.pos += 1;
                Ok(v as u64)
            }
            TarsType::Int2 => {
                self.ensure_available(2).map_err(|e| {
                    let mut e = e;
                    if let Error::BufferOverflow { offset, .. } = &mut e {
                        *offset = start_pos;
                    }
                    e
                })?;
                let bytes = self.data[self.pos..self.pos + 2].try_into().unwrap();
                let v = u16::from_be_bytes(bytes);
                self.pos += 2;
                Ok(v as u64)
            }
            TarsType::Int4 => {
                self.ensure_available(4).map_err(|e| {
                    let mut e = e;
                    if let Error::BufferOverflow { offset, .. } = &mut e {
                        *offset = start_pos;
                    }
                    e
                })?;
                let bytes = self.data[self.pos..self.pos + 4].try_into().unwrap();
                let v = u32::from_be_bytes(bytes);
                self.pos += 4;
                Ok(v as u64)
            }
            TarsType::Int8 => {
                self.ensure_available(8).map_err(|e| {
                    let mut e = e;
                    if let Error::BufferOverflow { offset, .. } = &mut e {
                        *offset = start_pos;
                    }
                    e
                })?;
                let bytes = self.data[self.pos..self.pos + 8].try_into().unwrap();
                let v = u64::from_be_bytes(bytes);
                self.pos += 8;
                Ok(v)
            }
            _ => Err(Error::new(
                start_pos,
                format!("Cannot read uint from type {:?}", type_id),
            )),
        }
    }

    /// 跳过指定的 Tars 类型值.
    fn skip_element(&mut self, type_id: TarsType) -> Result<()> {
        match type_id {
            TarsType::Int1 => self.skip(1),
            TarsType::Int2 => self.skip(2),
            TarsType::Int4 => self.skip(4),
            TarsType::Int8 => self.skip(8),
            TarsType::Float => self.skip(4),
            TarsType::Double => self.skip(8),
            TarsType::String1 => {
                let len = self.read_u8()? as usize;
                self.skip(len)
            }
            TarsType::String4 => {
                self.ensure_available(4)?;
                let bytes: [u8; 4] = self.data[self.pos..self.pos + 4].try_into().unwrap();
                let len = u32::from_be_bytes(bytes) as usize;
                self.pos += 4;
                self.skip(len)
            }
            TarsType::StructBegin => {
                loop {
                    let (_, t) = self.read_head()?;
                    if t == TarsType::StructEnd {
                        break;
                    }
                    self.skip_recursive(t)?;
                }
                Ok(())
            }
            TarsType::StructEnd => Ok(()),
            TarsType::ZeroTag => Ok(()),
            TarsType::SimpleList => {
                let t = self.read_u8()?; // 内部类型(byte)
                if t != 0 {
                    return Err(Error::new(
                        self.pos,
                        format!("SimpleList must contain Byte (0), got {}", t),
                    ));
                }
                let size = self.read_size()?;
                self.skip(size as usize)
            }
            TarsType::Map => {
                let size = self.read_size()?;
                for _ in 0..size * 2 {
                    let (_, t) = self.read_head()?;
                    self.skip_recursive(t)?;
                }
                Ok(())
            }
            TarsType::List => {
                let size = self.read_size()?;
                for _ in 0..size {
                    let (_, t) = self.read_head()?;
                    self.skip_recursive(t)?;
                }
                Ok(())
            }
        }
    }

    #[inline]
    fn skip_recursive(&mut self, type_id: TarsType) -> Result<()> {
        match type_id {
            TarsType::StructBegin | TarsType::List | TarsType::Map => self.skip_field(type_id),
            _ => self.skip_element(type_id),
        }
    }

    #[inline]
    fn skip(&mut self, len: usize) -> Result<()> {
        self.ensure_available(len)?;
        self.pos += len;
        Ok(())
    }

    /// 读取单精度浮点数.
    #[inline]
    pub fn read_float(&mut self, type_id: TarsType) -> Result<f32> {
        match type_id {
            TarsType::ZeroTag => Ok(0.0),
            TarsType::Float => {
                let start_pos = self.pos;
                self.ensure_available(4).map_err(|e| {
                    let mut e = e;
                    if let Error::BufferOverflow { offset, .. } = &mut e {
                        *offset = start_pos;
                    }
                    e
                })?;
                let bytes = self.data[self.pos..self.pos + 4].try_into().unwrap();
                let v = f32::from_be_bytes(bytes);
                self.pos += 4;
                Ok(v)
            }
            _ => Err(Error::new(
                self.pos,
                format!("Cannot read float from type {:?}", type_id),
            )),
        }
    }

    /// 读取双精度浮点数.
    #[inline]
    pub fn read_double(&mut self, type_id: TarsType) -> Result<f64> {
        match type_id {
            TarsType::ZeroTag => Ok(0.0),
            TarsType::Float => self.read_float(type_id).map(|v| v as f64),
            TarsType::Double => {
                let start_pos = self.pos;
                self.ensure_available(8).map_err(|e| {
                    let mut e = e;
                    if let Error::BufferOverflow { offset, .. } = &mut e {
                        *offset = start_pos;
                    }
                    e
                })?;
                let bytes = self.data[self.pos..self.pos + 8].try_into().unwrap();
                let v = f64::from_be_bytes(bytes);
                self.pos += 8;
                Ok(v)
            }
            _ => Err(Error::new(
                self.pos,
                format!("Cannot read double from type {:?}", type_id),
            )),
        }
    }

    /// 读取字符串 payload(原始字节,不校验 UTF-8).
    ///
    /// 仅做长度与越界检查,返回指向输入缓冲区的切片.
    #[inline]
    pub fn read_string(&mut self, type_id: TarsType) -> Result<&'a [u8]> {
        let start_pos = self.pos;

        let len = match type_id {
            TarsType::String1 => {
                self.ensure_available(1).map_err(|e| {
                    let mut e = e;
                    if let Error::BufferOverflow { offset, .. } = &mut e {
                        *offset = start_pos;
                    }
                    e
                })?;
                let l = self.data[self.pos] as usize;
                self.pos += 1;
                l
            }
            TarsType::String4 => {
                self.ensure_available(4).map_err(|e| {
                    let mut e = e;
                    if let Error::BufferOverflow { offset, .. } = &mut e {
                        *offset = start_pos;
                    }
                    e
                })?;
                let bytes = self.data[self.pos..self.pos + 4].try_into().unwrap();
                let l = u32::from_be_bytes(bytes) as usize;
                self.pos += 4;
                l
            }
            _ => {
                return Err(Error::new(
                    start_pos,
                    format!("Cannot read string bytes from type {:?}", type_id),
                ));
            }
        };

        let slice = self.read_bytes(len)?;
        Ok(slice)
    }

    /// 跳过当前字段.
    pub fn skip_field(&mut self, type_id: TarsType) -> Result<()> {
        if self.depth > 100 {
            return Err(Error::new(
                self.pos,
                "Max recursion depth exceeded in skip_field",
            ));
        }

        self.depth += 1;
        let res = self.skip_element(type_id); // Reused skip_element logic
        self.depth -= 1;
        res
    }

    pub fn read_simplelist_bytes(&mut self) -> Result<&'a [u8]> {
        let subtype = self.read_u8()?;
        if subtype != 0 {
            return Err(Error::new(
                self.pos,
                format!("SimpleList must contain Byte (0), got {}", subtype),
            ));
        }

        let len = self.read_size()?;
        if len < 0 {
            return Err(Error::new(self.pos, "Invalid SimpleList size"));
        }

        let len = len as usize;
        let slice = self.read_bytes(len)?;
        Ok(slice)
    }

    /// 读取字节数组(零拷贝).
    #[inline]
    pub fn read_bytes(&mut self, len: usize) -> Result<&'a [u8]> {
        self.ensure_available(len)?;
        let slice = &self.data[self.pos..self.pos + len];
        self.pos += len;
        Ok(slice)
    }

    /// 读取一个字节.
    #[inline]
    pub fn read_u8(&mut self) -> Result<u8> {
        self.ensure_available(1)?;
        let v = self.data[self.pos];
        self.pos += 1;
        Ok(v)
    }

    /// 读取 Tars 容器的大小(List/Map/SimpleList 长度).
    #[inline]
    pub fn read_size(&mut self) -> Result<i32> {
        let (_, t) = self.read_head()?;
        self.read_int(t).map(|v| v as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::writer::TarsWriter;
    use proptest::prelude::*;

    #[test]
    fn test_read_head_with_valid_data_returns_correct_tag_and_type() {
        let data = b"\x10";
        let mut reader = TarsReader::new(data);
        let (tag, t) = reader.read_head().unwrap();
        assert_eq!(tag, 1);
        assert_eq!(t, TarsType::Int1);

        let data = b"\xF0\x0F";
        let mut reader = TarsReader::new(data);
        let (tag, t) = reader.read_head().unwrap();
        assert_eq!(tag, 15);
        assert_eq!(t, TarsType::Int1);
    }

    #[test]
    fn test_read_head_with_large_tag_returns_correct_expanded_tag() {
        let data = [0xF0, 0xFF];
        let mut reader = TarsReader::new(&data);
        let (tag, t) = reader.read_head().unwrap();
        assert_eq!(tag, 255);
        assert_eq!(t, TarsType::Int1);
    }

    #[test]
    fn test_read_head_with_truncated_large_tag_returns_overflow_error() {
        let data = [0xF0];
        let mut reader = TarsReader::new(&data);
        assert!(matches!(
            reader.read_head(),
            Err(Error::BufferOverflow {
                offset: 0,
                required: 2,
                available: 1
            })
        ));
    }

    #[test]
    fn test_peek_head_returns_tag_and_type_without_advancing_cursor() {
        let data = [0x12];
        let mut reader = TarsReader::new(&data);
        let (tag, t) = reader.peek_head().unwrap();
        assert_eq!(tag, 1);
        assert_eq!(t, TarsType::Int4);
        assert_eq!(reader.position(), 0);

        let (tag2, t2) = reader.read_head().unwrap();
        assert_eq!(tag, tag2);
        assert_eq!(t, t2);
        assert_eq!(reader.position(), 1);
    }

    #[test]
    fn test_read_int_with_various_types_returns_correct_i64() {
        let data = b"\x00\x00\x01\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00\x01";
        let mut reader = TarsReader::new(data);
        assert_eq!(reader.read_int(TarsType::Int1).unwrap(), 0);
        assert_eq!(reader.read_int(TarsType::Int2).unwrap(), 1);
        assert_eq!(reader.read_int(TarsType::Int4).unwrap(), 1);
        assert_eq!(reader.read_int(TarsType::Int8).unwrap(), 1);
        assert_eq!(reader.read_int(TarsType::ZeroTag).unwrap(), 0);
    }

    #[test]
    fn test_read_int_with_non_integer_type_returns_semantic_error() {
        let data = [];
        let mut reader = TarsReader::new(&data);
        let err = reader.read_int(TarsType::String1).unwrap_err();
        assert!(matches!(err, Error::Custom { msg, .. } if msg.contains("Cannot read int")));
    }

    #[test]
    fn test_read_int_with_truncated_buffer_returns_overflow_error() {
        let data = [0x00];
        let mut reader = TarsReader::new(&data);
        assert!(matches!(
            reader.read_int(TarsType::Int2),
            Err(Error::BufferOverflow { required: 2, .. })
        ));
        assert!(matches!(
            reader.read_int(TarsType::Int8),
            Err(Error::BufferOverflow { required: 8, .. })
        ));
    }

    #[test]
    fn test_read_int_with_insufficient_buffer_returns_overflow_error() {
        let data = [0x00];
        let mut reader = TarsReader::new(&data);

        assert!(matches!(
            reader.read_int(TarsType::Int4),
            Err(Error::BufferOverflow { required: 4, .. })
        ));
    }

    #[test]
    fn test_read_uint_with_all_valid_types_returns_correct_u64() {
        let data1 = [255];
        let mut reader = TarsReader::new(&data1);
        assert_eq!(reader.read_uint(TarsType::Int1).unwrap(), 255);

        let data2 = [0xFF, 0xFF];
        let mut reader = TarsReader::new(&data2);
        assert_eq!(reader.read_uint(TarsType::Int2).unwrap(), 65535);

        let data4 = [0xFF, 0xFF, 0xFF, 0xFF];
        let mut reader = TarsReader::new(&data4);
        assert_eq!(reader.read_uint(TarsType::Int4).unwrap(), 4294967295);

        let data8 = [0, 0, 0, 0, 0, 0, 0, 1];
        let mut reader = TarsReader::new(&data8);
        assert_eq!(reader.read_uint(TarsType::Int8).unwrap(), 1);

        assert_eq!(reader.read_uint(TarsType::ZeroTag).unwrap(), 0);
    }

    #[test]
    fn test_read_float_and_double_returns_correct_f32_and_f64() {
        let mut w = TarsWriter::new();
        w.write_float(0, 1.5f32);
        w.write_double(1, 2.5f64);

        let data = w.get_buffer();
        let mut reader = TarsReader::new(data);

        let (_, t1) = reader.read_head().unwrap();
        assert_eq!(t1, TarsType::Float);
        assert_eq!(reader.read_float(t1).unwrap(), 1.5f32);

        let (_, t2) = reader.read_head().unwrap();
        assert_eq!(t2, TarsType::Double);
        assert_eq!(reader.read_double(t2).unwrap(), 2.5f64);
    }

    #[test]
    fn test_read_float_with_zero_tag_returns_zero() {
        let mut w = TarsWriter::new();
        w.write_float(0, 0.0);

        let data = w.get_buffer();
        let mut reader = TarsReader::new(data);
        let (_, t) = reader.read_head().unwrap();
        assert_eq!(t, TarsType::ZeroTag);
        assert_eq!(reader.read_float(t).unwrap(), 0.0);
        assert!(reader.is_end());
    }

    #[test]
    fn test_read_double_accepts_float_and_zero_tag() {
        let mut w = TarsWriter::new();
        w.write_float(0, 1.25f32);
        w.write_double(1, 0.0);

        let data = w.get_buffer();
        let mut reader = TarsReader::new(data);

        let (_, t1) = reader.read_head().unwrap();
        assert_eq!(t1, TarsType::Float);
        assert_eq!(reader.read_double(t1).unwrap(), 1.25f64);

        let (_, t2) = reader.read_head().unwrap();
        assert_eq!(t2, TarsType::ZeroTag);
        assert_eq!(reader.read_double(t2).unwrap(), 0.0);
        assert!(reader.is_end());
    }

    #[test]
    fn test_read_string_with_string1_and_string4_returns_correct_bytes() {
        let data = b"\x05Hello\x00\x00\x00\x05World";
        let mut reader = TarsReader::new(data);
        assert_eq!(reader.read_string(TarsType::String1).unwrap(), b"Hello");
        assert_eq!(reader.read_string(TarsType::String4).unwrap(), b"World");
    }

    #[test]
    fn test_read_string_returns_slice() {
        let data = b"\x05Hello";
        let mut reader = TarsReader::new(data);
        assert_eq!(reader.read_string(TarsType::String1).unwrap(), b"Hello");
    }

    #[test]
    fn test_read_string_with_truncated_payload_returns_overflow() {
        let data = b"\x05He";
        let mut reader = TarsReader::new(data);
        assert!(matches!(
            reader.read_string(TarsType::String1),
            Err(Error::BufferOverflow { required: 5, .. })
        ));
    }

    #[test]
    fn test_read_string_with_errors_returns_correct_error_variants() {
        let data = [];
        let mut reader = TarsReader::new(&data);

        let err = reader.read_string(TarsType::Int1).unwrap_err();
        assert!(
            matches!(err, Error::Custom { msg, .. } if msg.contains("Cannot read string bytes"))
        );

        let err = reader.read_string(TarsType::String1).unwrap_err();
        assert!(matches!(err, Error::BufferOverflow { required: 1, .. }));
    }

    #[test]
    fn test_read_string_allows_invalid_utf8_bytes() {
        let data = [0x01, 0xFF];
        let mut reader = TarsReader::new(&data);
        let bytes = reader.read_string(TarsType::String1).unwrap();
        assert_eq!(bytes, b"\xFF");
    }

    #[test]
    fn test_read_bytes_and_u8_with_valid_buffer_returns_correct_binary_data() {
        let data = [0x01, 0x02, 0x03];
        let mut reader = TarsReader::new(&data);

        assert_eq!(reader.read_u8().unwrap(), 0x01);
        assert_eq!(reader.read_bytes(2).unwrap(), &[0x02, 0x03]);
        assert!(matches!(
            reader.read_bytes(1),
            Err(Error::BufferOverflow { required: 1, .. })
        ));
    }

    #[test]
    fn test_skip_field_with_struct_type_advances_cursor_to_end() {
        let data = b"\x1A\x10\x01\x0B";
        let mut reader = TarsReader::new(data);
        let (tag, t) = reader.read_head().unwrap();
        assert_eq!(tag, 1);
        assert_eq!(t, TarsType::StructBegin);
        reader.skip_field(t).unwrap();
        assert!(reader.is_end());
    }

    #[test]
    fn test_skip_element_with_map_type_advances_cursor_to_end() {
        let mut w = TarsWriter::new();
        w.write_tag(0, TarsType::Map);
        w.write_int(0, 1);
        w.write_int(0, 42);
        w.write_string(1, "x");

        let mut reader = TarsReader::new(w.get_buffer());
        let (_tag, t) = reader.read_head().unwrap();
        assert_eq!(t, TarsType::Map);
        reader.skip_element(t).unwrap();
        assert!(reader.is_end());
    }

    #[test]
    fn test_skip_field_with_deeply_nested_list_returns_recursion_error() {
        let mut w = TarsWriter::new();

        for _ in 0..102 {
            w.write_tag(0, TarsType::List);
            w.write_int(0, 1);
        }
        w.write_tag(0, TarsType::ZeroTag);

        let mut reader = TarsReader::new(w.get_buffer());
        let (_tag, t) = reader.read_head().unwrap();

        let err = reader.skip_field(t).unwrap_err();
        match err {
            Error::Custom { msg, .. } => assert!(msg.contains("depth exceeded")),
            _ => panic!("Expected Custom error, got {:?}", err),
        }
    }

    #[test]
    fn test_skip_field_with_simple_list_validates_subtype_is_byte() {
        let mut w = TarsWriter::new();
        w.write_bytes(0, b"abc");

        let data = w.get_buffer();
        let mut reader = TarsReader::new(data);
        let (_, t) = reader.read_head().unwrap();
        reader.skip_field(t).unwrap();
        assert!(reader.is_end());

        let bad_data = [(TarsType::SimpleList as u8), 1, 0];
        let mut reader = TarsReader::new(&bad_data);
        let (_, t) = reader.read_head().unwrap();
        let err = reader.skip_field(t).unwrap_err();
        assert!(
            matches!(err, Error::Custom { msg, .. } if msg.contains("SimpleList must contain Byte"))
        );
    }

    #[test]
    fn test_skip_element_with_simple_list_invalid_subtype_returns_error() {
        let bad_data = [(TarsType::SimpleList as u8), 1, 0];
        let mut reader = TarsReader::new(&bad_data);
        let (_, t) = reader.read_head().unwrap();
        let err = reader.skip_element(t).unwrap_err();
        assert!(
            matches!(err, Error::Custom { msg, .. } if msg.contains("SimpleList must contain Byte"))
        );
    }

    #[test]
    fn test_remaining_returns_unconsumed_slice() {
        let data = [0x01u8, 0x02, 0x03];
        let mut reader = TarsReader::new(&data);
        assert_eq!(reader.remaining(), &data);
        assert_eq!(reader.read_u8().unwrap(), 0x01);
        assert_eq!(reader.remaining(), &data[1..]);
    }

    #[test]
    fn test_read_simplelist_bytes_reads_payload() {
        let mut w = TarsWriter::new();
        w.write_bytes(0, b"abc");
        let data = w.get_buffer();

        let mut reader = TarsReader::new(data);
        let (_, t) = reader.read_head().unwrap();
        assert_eq!(t, TarsType::SimpleList);

        let payload = reader.read_simplelist_bytes().unwrap();
        assert_eq!(payload, b"abc");
        assert!(reader.is_end());
    }

    #[test]
    fn test_read_simplelist_bytes_with_invalid_subtype_returns_error() {
        let data = [TarsType::SimpleList as u8, 1u8, 0x00, 0x01, 0x61];
        let mut reader = TarsReader::new(&data);
        let (_, t) = reader.read_head().unwrap();
        assert_eq!(t, TarsType::SimpleList);
        let err = reader.read_simplelist_bytes().unwrap_err();
        assert!(matches!(err, Error::Custom { .. }));
    }

    #[test]
    fn test_read_simplelist_bytes_with_negative_size_returns_error() {
        let data = [TarsType::SimpleList as u8, 0u8, 0x00, 0xFF];
        let mut reader = TarsReader::new(&data);
        let (_, t) = reader.read_head().unwrap();
        assert_eq!(t, TarsType::SimpleList);
        let err = reader.read_simplelist_bytes().unwrap_err();
        assert!(matches!(err, Error::Custom { .. }));
    }

    proptest! {
        #[test]
        fn test_reader_robustness_with_random_input_is_panic_free(data in proptest::collection::vec(any::<u8>(), 0..100)) {
            let mut reader = TarsReader::new(&data);
            let _ = reader.read_head();
            let _ = reader.read_int(TarsType::Int4);
        }
    }

    #[test]
    fn test_read_head_atomic_on_failure() {
        // Tag 15 (0xF0) but missing second byte
        let data = [0xF0];
        let mut reader = TarsReader::new(&data);
        assert!(matches!(
            reader.read_head(),
            Err(Error::BufferOverflow {
                offset: 0,
                required: 2,
                available: 1
            })
        ));
        // Verify pos is reverted to 0
        assert_eq!(reader.position(), 0);

        // Invalid type id (e.g. 14 if it were invalid, but 0-13 are valid. 14 is List. 15 is reserved?)
        // TarsType::try_from accepts 0..=13. 14, 15 are invalid.
        // 0x0E (Type 14)
        let data_invalid_type = [0x0E];
        let mut reader2 = TarsReader::new(&data_invalid_type);
        assert!(matches!(
            reader2.read_head(),
            Err(Error::InvalidType {
                offset: 0,
                type_id: 14,
                expected_types: "0..=13"
            })
        ));
        assert_eq!(reader2.position(), 0);
    }
}
