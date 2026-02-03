/// Tars 协议的数据类型标识符（类型 ID）。
///
/// 定义了 Tars 二进制协议中使用的 4 位类型标记。
/// 这些标记通常存储在标签字节的低 4 位中。
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TarsType {
    /// 1 字节整数 (对应 i8/u8)。
    Int1 = 0,
    /// 2 字节整数 (对应 i16/u16)。
    Int2 = 1,
    /// 4 字节整数 (对应 i32/u32)。
    Int4 = 2,
    /// 8 字节整数 (对应 i64/u64)。
    Int8 = 3,
    /// 4 字节单精度浮点数 (对应 float)。
    Float = 4,
    /// 8 字节双精度浮点数 (对应 double)。
    Double = 5,
    /// 长度小于 256 字节的短字符串 (长度前缀为 1 字节)。
    String1 = 6,
    /// 长度可能超过 255 字节的长字符串 (长度前缀为 4 字节)。
    String4 = 7,
    /// 映射表的开始。
    Map = 8,
    /// 列表的开始。
    List = 9,
    /// 自定义结构体的开始。
    StructBegin = 10,
    /// 自定义结构体的结束。
    StructEnd = 11,
    /// 值为 0 的整数 (用于压缩存储)。
    ZeroTag = 12,
    /// 简单列表 (字节数组专用优化)。
    /// 仅用于存储 `byte` 类型的数组 (`vector<byte>`)。
    SimpleList = 13,
}

impl TryFrom<u8> for TarsType {
    type Error = u8;

    #[inline]
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(TarsType::Int1),
            1 => Ok(TarsType::Int2),
            2 => Ok(TarsType::Int4),
            3 => Ok(TarsType::Int8),
            4 => Ok(TarsType::Float),
            5 => Ok(TarsType::Double),
            6 => Ok(TarsType::String1),
            7 => Ok(TarsType::String4),
            8 => Ok(TarsType::Map),
            9 => Ok(TarsType::List),
            10 => Ok(TarsType::StructBegin),
            11 => Ok(TarsType::StructEnd),
            12 => Ok(TarsType::ZeroTag),
            13 => Ok(TarsType::SimpleList),
            _ => Err(value),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tars_type_with_all_variants_has_expected_values() {
        assert_eq!(TarsType::Int1 as u8, 0);
        assert_eq!(TarsType::Int2 as u8, 1);
        assert_eq!(TarsType::Int4 as u8, 2);
        assert_eq!(TarsType::Int8 as u8, 3);
        assert_eq!(TarsType::Float as u8, 4);
        assert_eq!(TarsType::Double as u8, 5);
        assert_eq!(TarsType::String1 as u8, 6);
        assert_eq!(TarsType::String4 as u8, 7);
        assert_eq!(TarsType::Map as u8, 8);
        assert_eq!(TarsType::List as u8, 9);
        assert_eq!(TarsType::StructBegin as u8, 10);
        assert_eq!(TarsType::StructEnd as u8, 11);
        assert_eq!(TarsType::ZeroTag as u8, 12);
        assert_eq!(TarsType::SimpleList as u8, 13);
    }

    #[test]
    fn test_tars_type_try_from_u8_with_valid_and_invalid_returns_expected() {
        assert_eq!(TarsType::try_from(0), Ok(TarsType::Int1));
        assert_eq!(TarsType::try_from(13), Ok(TarsType::SimpleList));
        assert_eq!(TarsType::try_from(14), Err(14));
    }
}
