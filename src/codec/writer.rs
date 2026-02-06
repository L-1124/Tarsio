use crate::codec::consts::TarsType;

use bytes::BufMut;

/// Tars 数据流编码器(写入器).
///
/// 用于将 Rust 数据类型序列化为 Tars 二进制格式.
/// 支持内存缓冲区 (`Vec<u8>`) 以及任何实现了 `bytes::BufMut` 的类型.
pub struct TarsWriter<B = Vec<u8>> {
    buffer: B,
}

impl Default for TarsWriter<Vec<u8>> {
    fn default() -> Self {
        Self::new()
    }
}

impl TarsWriter<Vec<u8>> {
    /// 创建一个新的 TarsWriter.
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(128),
        }
    }

    pub fn into_inner(self) -> Vec<u8> {
        self.buffer
    }

    pub fn reserve(&mut self, additional: usize) {
        self.buffer.reserve(additional);
    }

    /// 重置写入器(针对 Vec 的特化实现).
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

impl<B: BufMut> TarsWriter<B> {
    /// 使用指定的缓冲区创建 TarsWriter.
    pub fn with_buffer(buffer: B) -> Self {
        Self { buffer }
    }

    /// 获取编码后的字节流.
    #[inline]
    pub fn get_buffer(&self) -> &[u8]
    where
        B: AsRef<[u8]>,
    {
        self.buffer.as_ref()
    }

    /// 写入标签和类型头部信息.
    ///
    /// 自动处理标签 < 15 和标签 >= 15 的两种头部格式.
    #[inline]
    pub fn write_tag(&mut self, tag: u8, type_id: TarsType) {
        let type_val = type_id as u8;
        if tag < 15 {
            // 低 4 位是类型,高 4 位是标签
            let header = (tag << 4) | type_val;
            self.buffer.put_u8(header);
        } else {
            // 高 4 位全 1(15),接着写入标签字节,低 4 位是类型
            let header = (15 << 4) | type_val;
            self.buffer.put_u8(header);
            self.buffer.put_u8(tag);
        }
    }

    /// 写入整数(自动选择最小宽度).
    ///
    /// 根据数值大小自动选择 Int1、Int2、Int4、Int8 或 ZeroTag 类型.
    /// 这是 Tars 协议的一种压缩优化.
    #[inline]
    pub fn write_int(&mut self, tag: u8, value: i64) {
        if value == 0 {
            self.write_tag(tag, TarsType::ZeroTag);
        } else if value >= i8::MIN as i64 && value <= i8::MAX as i64 {
            self.write_tag(tag, TarsType::Int1);
            self.buffer.put_u8(value as u8);
        } else if value >= i16::MIN as i64 && value <= i16::MAX as i64 {
            self.write_tag(tag, TarsType::Int2);
            self.buffer.put_i16(value as i16);
        } else if value >= i32::MIN as i64 && value <= i32::MAX as i64 {
            self.write_tag(tag, TarsType::Int4);
            self.buffer.put_i32(value as i32);
        } else {
            self.write_tag(tag, TarsType::Int8);
            self.buffer.put_i64(value);
        }
    }

    /// 写入单精度浮点数.
    #[inline]
    pub fn write_float(&mut self, tag: u8, value: f32) {
        if value == 0.0 {
            self.write_tag(tag, TarsType::ZeroTag);
            return;
        }
        self.write_tag(tag, TarsType::Float);
        self.buffer.put_f32(value);
    }

    /// 写入双精度浮点数.
    #[inline]
    pub fn write_double(&mut self, tag: u8, value: f64) {
        if value == 0.0 {
            self.write_tag(tag, TarsType::ZeroTag);
            return;
        }
        self.write_tag(tag, TarsType::Double);
        self.buffer.put_f64(value);
    }

    /// 写入字符串.
    #[inline]
    pub fn write_string(&mut self, tag: u8, value: &str) {
        let bytes = value.as_bytes();
        let len = bytes.len();
        if len <= 255 {
            self.write_tag(tag, TarsType::String1);
            self.buffer.put_u8(len as u8);
        } else {
            self.write_tag(tag, TarsType::String4);
            self.buffer.put_u32(len as u32);
        }
        self.buffer.put_slice(bytes);
    }

    /// 写入字节数组(SimpleList).
    #[inline]
    pub fn write_bytes(&mut self, tag: u8, value: &[u8]) {
        self.write_tag(tag, TarsType::SimpleList);
        // 元素类型字节:0 表示字节
        self.buffer.put_u8(0);
        // 写入长度,使用 write_int(Tag 0)
        self.write_int(0, value.len() as i64);
        self.buffer.put_slice(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证数值 0 是否能被优化为 ZeroTag 以压缩字节大小.
    #[test]
    fn test_write_int_with_zero_value_produces_zero_tag() {
        let mut writer = TarsWriter::new();
        writer.write_int(0, 0);
        assert_eq!(writer.get_buffer(), b"\x0c"); // 标签 0,ZeroTag
    }

    #[test]
    fn test_write_float_and_double_with_zero_value_produces_zero_tag() {
        let mut writer = TarsWriter::new();
        writer.write_float(0, 0.0);
        assert_eq!(writer.get_buffer(), b"\x0c"); // 标签 0,ZeroTag

        writer.clear();
        writer.write_double(1, 0.0);
        assert_eq!(writer.get_buffer(), b"\x1c"); // 标签 1,ZeroTag
    }

    /// 验证小整数是否被正确编码为 Int1 类型.
    #[test]
    fn test_write_int_with_small_value_produces_int1_type() {
        let mut writer = TarsWriter::new();
        writer.write_int(0, 1);
        assert_eq!(writer.get_buffer(), b"\x00\x01"); // 标签 0,Int1,值 1
    }

    /// 验证超过 1 字节范围的整数是否被正确编码为 Int2 类型.
    #[test]
    fn test_write_int_with_i16_range_value_produces_int2_type() {
        let mut writer = TarsWriter::new();
        writer.write_int(0, 256);
        assert_eq!(writer.get_buffer(), b"\x01\x01\x00"); // 标签 0,Int2,值 256(0x0100)
    }

    /// 验证字符串的编码布局,包含 Tag、类型标记、长度及内容.
    #[test]
    fn test_write_string_with_short_value_produces_string1_type() {
        let mut writer = TarsWriter::new();
        writer.write_string(0, "a");
        assert_eq!(writer.get_buffer(), b"\x06\x01\x61"); // 标签 0,String1,长度 1,'a'
    }

    /// 验证二进制字节数组的编码布局,遵循 SimpleList 规范.
    #[test]
    fn test_write_bytes_with_valid_data_produces_simple_list_type() {
        let mut writer = TarsWriter::new();
        writer.write_bytes(0, b"abc");
        assert_eq!(writer.get_buffer(), b"\x0d\x00\x00\x03abc");
    }

    /// 验证当 Tag >= 15 时,编码器是否正确生成双字节扩展头部.
    #[test]
    fn test_write_int_with_high_tag_produces_expanded_header() {
        let mut writer = TarsWriter::new();
        writer.write_int(15, 1);
        assert_eq!(writer.get_buffer(), b"\xf0\x0f\x01"); // 标签 15,Int1,值 1
    }
}
