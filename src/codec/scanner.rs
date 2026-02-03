use crate::codec::consts::TarsType;
use crate::codec::error::{Error, Result};
use std::io::{Cursor, Read}; // Added Read for read_exact

/// 轻量级结构扫描器。
///
/// 用于在不进行完整解码（不分配内存）的情况下，快速验证 Tars 数据的结构合法性。
/// 适用于网关或路由层，仅需探测包结构而不关心具体值。
pub struct TarsScanner<'a> {
    cursor: Cursor<&'a [u8]>,
    depth: usize,
    max_depth: usize,
}

impl<'a> TarsScanner<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            cursor: Cursor::new(bytes),
            depth: 0,
            max_depth: 100,
        }
    }

    #[inline]
    pub fn is_end(&self) -> bool {
        self.cursor.position() >= self.cursor.get_ref().len() as u64
    }

    /// 验证 Struct 结构的合法性。
    ///
    /// 递归遍历数据流，确保所有字段的 Tag、Type 和长度信息是自洽的。
    ///
    /// # 检查项
    ///
    /// 1. 字段类型有效性 (Type ID 是否合法)。
    /// 2. 容器长度合法性 (不会导致读取越界)。
    /// 3. 结构体嵌套配对 (StructBegin 与 StructEnd)。
    /// 4. 递归深度限制 (防止 Stack Overflow)。
    ///
    /// # 错误
    ///
    /// * `Error::BufferOverflow`: 数据截断或长度错误。
    /// * `Error::InvalidType`: 遇到未知的 Type ID。
    /// * `Error::Custom`: 递归过深或 SimpleList 类型错误。
    pub fn validate_struct(&mut self) -> Result<()> {
        if self.depth > self.max_depth {
            return Err(Error::new(
                self.cursor.position() as usize,
                "Max recursion depth exceeded",
            ));
        }
        self.depth += 1;

        while !self.is_end() {
            let (_tag, tars_type) = self.read_head()?;
            if tars_type == TarsType::StructEnd {
                self.depth -= 1;
                return Ok(());
            }
            self.skip_field(tars_type)?;
        }

        // If we reached end without StructEnd, it's only okay if we are at root depth 1
        // (for raw packets that are just a sequence of fields)
        if self.depth == 1 {
            Ok(())
        } else {
            Err(Error::BufferOverflow {
                offset: self.cursor.position() as usize,
            })
        }
    }

    #[inline]
    fn read_head(&mut self) -> Result<(u8, TarsType)> {
        let pos = self.cursor.position();
        let mut buf = [0u8; 1];
        self.cursor
            .read_exact(&mut buf)
            .map_err(|_| Error::BufferOverflow {
                offset: pos as usize,
            })?;
        let b = buf[0];
        let type_id = b & 0x0F;
        let mut tag = (b & 0xF0) >> 4;
        if tag == 15 {
            let mut buf = [0u8; 1];
            self.cursor
                .read_exact(&mut buf)
                .map_err(|_| Error::BufferOverflow {
                    offset: self.cursor.position() as usize,
                })?;
            tag = buf[0];
        }
        let tars_type = TarsType::try_from(type_id).map_err(|id| Error::InvalidType {
            offset: pos as usize,
            type_id: id,
        })?;
        Ok((tag, tars_type))
    }

    fn skip_field(&mut self, tars_type: TarsType) -> Result<()> {
        match tars_type {
            TarsType::Int1 => self.skip(1),
            TarsType::Int2 => self.skip(2),
            TarsType::Int4 => self.skip(4),
            TarsType::Int8 => self.skip(8),
            TarsType::Float => self.skip(4),
            TarsType::Double => self.skip(8),
            TarsType::String1 => {
                let mut buf = [0u8; 1];
                self.cursor
                    .read_exact(&mut buf)
                    .map_err(|_| Error::BufferOverflow {
                        offset: self.cursor.position() as usize,
                    })?;
                let len = buf[0];
                self.skip(len as u64)
            }
            TarsType::String4 => {
                let mut buf = [0u8; 4];
                self.cursor
                    .read_exact(&mut buf)
                    .map_err(|_| Error::BufferOverflow {
                        offset: self.cursor.position() as usize,
                    })?;
                let len = u32::from_be_bytes(buf);
                self.skip(len as u64)
            }
            TarsType::Map => {
                let size = self.read_size()?;
                for _ in 0..size * 2 {
                    let (_, t) = self.read_head()?;
                    self.skip_field(t)?;
                }
                Ok(())
            }
            TarsType::List => {
                let size = self.read_size()?;
                for _ in 0..size {
                    let (_, t) = self.read_head()?;
                    self.skip_field(t)?;
                }
                Ok(())
            }
            TarsType::SimpleList => {
                let mut buf = [0u8; 1];
                self.cursor
                    .read_exact(&mut buf)
                    .map_err(|_| Error::BufferOverflow {
                        offset: self.cursor.position() as usize,
                    })?;
                let t = buf[0];
                if t != 0 {
                    return Err(Error::new(
                        self.cursor.position() as usize,
                        "SimpleList must contain Byte",
                    ));
                }
                let len = self.read_size()?;
                self.skip(len as u64)
            }
            TarsType::StructBegin => self.validate_struct(),
            TarsType::StructEnd => Ok(()),
            TarsType::ZeroTag => Ok(()),
        }
    }

    #[inline]
    fn skip(&mut self, len: u64) -> Result<()> {
        let pos = self.cursor.position();
        let new_pos = pos + len;
        if new_pos > self.cursor.get_ref().len() as u64 {
            return Err(Error::BufferOverflow {
                offset: pos as usize,
            });
        }
        self.cursor.set_position(new_pos);
        Ok(())
    }

    fn read_size(&mut self) -> Result<i32> {
        let (_, t) = self.read_head()?;
        match t {
            TarsType::ZeroTag => Ok(0),
            TarsType::Int1 => {
                let mut buf = [0u8; 1];
                self.cursor
                    .read_exact(&mut buf)
                    .map_err(|_| Error::BufferOverflow {
                        offset: self.cursor.position() as usize,
                    })?;
                Ok(buf[0] as i8 as i32)
            }
            TarsType::Int2 => {
                let mut buf = [0u8; 2];
                self.cursor
                    .read_exact(&mut buf)
                    .map_err(|_| Error::BufferOverflow {
                        offset: self.cursor.position() as usize,
                    })?;
                Ok(i16::from_be_bytes(buf) as i32)
            }
            TarsType::Int4 => {
                let mut buf = [0u8; 4];
                self.cursor
                    .read_exact(&mut buf)
                    .map_err(|_| Error::BufferOverflow {
                        offset: self.cursor.position() as usize,
                    })?;
                Ok(i32::from_be_bytes(buf))
            }
            _ => Err(Error::new(
                self.cursor.position() as usize,
                "Invalid size type",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证扫描器对简单结构体的全量校验能力。
    #[test]
    fn test_validate_struct_with_valid_simple_data_returns_ok() {
        // struct { 0: int1(1), 1: string("a") }
        let data = b"\x00\x01\x16\x01\x61";
        let mut scanner = TarsScanner::new(data);
        assert!(scanner.validate_struct().is_ok());
    }

    /// 验证扫描器对截断数据的边界处理，确保返回溢出错误。
    #[test]
    fn test_validate_struct_with_truncated_string_returns_overflow_error() {
        // String 声明长度 5 但数据只有 1
        let data = b"\x16\x05\x61";
        let mut scanner = TarsScanner::new(data);
        assert!(matches!(
            scanner.validate_struct(),
            Err(Error::BufferOverflow { .. })
        ));
    }
}

#[cfg(test)]
mod coverage_tests {
    use super::*;
    use crate::codec::writer::TarsWriter;

    // 辅助函数：在 Writer 的缓冲区上运行扫描器
    fn scan(w: &TarsWriter) -> Result<()> {
        let mut scanner = TarsScanner::new(w.get_buffer());
        scanner.validate_struct()
    }

    /// 验证所有标量类型在扫描时的合法性。
    #[test]
    fn test_scan_with_all_scalar_types_returns_ok() {
        let mut w = TarsWriter::new();
        w.write_int(0, 100); // Int1
        w.write_int(1, 1000); // Int2
        w.write_int(2, 100000); // Int4
        w.write_int(3, 10000000000); // Int8
        w.write_float(4, 1.23);
        w.write_double(5, 4.56);
        w.write_int(6, 0); // ZeroTag

        assert!(scan(&w).is_ok());
    }

    /// 验证超长字符串（String4）的扫描路径。
    #[test]
    fn test_scan_with_large_string4_returns_ok() {
        let mut w = TarsWriter::new();
        let s = "a".repeat(300);
        w.write_string(0, &s);
        assert!(scan(&w).is_ok());
    }

    /// 验证各种容器类型（List, Map, SimpleList）的递归扫描。
    #[test]
    fn test_scan_with_nested_containers_returns_ok() {
        let mut w = TarsWriter::new();

        // List<Int>
        w.write_tag(0, TarsType::List);
        w.write_int(0, 2); // Size 2
        w.write_int(0, 1);
        w.write_int(0, 2);

        // Map<Int, String>
        w.write_tag(1, TarsType::Map);
        w.write_int(0, 1); // Size 1
        w.write_int(0, 1); // Key
        w.write_string(0, "val"); // Val

        // SimpleList
        w.write_bytes(2, b"bytes");

        assert!(scan(&w).is_ok());
    }

    /// 验证嵌套结构体的扫描能力。
    #[test]
    fn test_scan_with_nested_struct_returns_ok() {
        let mut w = TarsWriter::new();
        w.write_tag(0, TarsType::StructBegin);
        w.write_int(0, 1);
        w.write_tag(0, TarsType::StructEnd);
        assert!(scan(&w).is_ok());
    }

    /// 验证 SimpleList 在子类型错误时的防御逻辑。
    #[test]
    fn test_scan_with_invalid_simple_list_subtype_returns_custom_error() {
        let bad_simple_list = [
            (TarsType::SimpleList as u8),
            1, // 非法子类型（必须为 0）
            1, // Size
            0, // Data
        ];
        let mut s = TarsScanner::new(&bad_simple_list);
        let res = s.validate_struct();
        assert!(matches!(res, Err(Error::Custom { msg, .. }) if msg.contains("SimpleList")));
    }

    /// 验证结构嵌套深度超过限制时的溢出保护。
    #[test]
    fn test_scan_with_exceeded_recursion_depth_returns_custom_error() {
        let mut w = TarsWriter::new();
        for _ in 0..102 {
            w.write_tag(0, TarsType::StructBegin);
        }
        let mut s = TarsScanner::new(w.get_buffer());
        let res = s.validate_struct();
        assert!(matches!(res, Err(Error::Custom { msg, .. }) if msg.contains("recursion depth")));
    }

    /// 验证容器 Size 字段类型不符合协议规范时的错误处理。
    #[test]
    fn test_read_size_with_invalid_type_marker_returns_custom_error() {
        let data = [
            0x09, // List
            0x06, 0x01, 0x61, // 错误的 Size 类型（使用了 String）
        ];
        let mut s = TarsScanner::new(&data);
        let res = s.validate_struct();
        assert!(matches!(res, Err(Error::Custom { msg, .. }) if msg.contains("Invalid size type")));
    }

    /// 验证双字节扩展 Tag 的扫描处理。
    #[test]
    fn test_read_head_with_large_tag_returns_ok() {
        // Tag 15, Int1
        let data = [0xF0, 0x0F, 0x00];
        let mut s = TarsScanner::new(&data);
        assert!(s.validate_struct().is_ok());
    }
}
