/// Tars 协议的数据类型标识符 (Type ID)。
///
/// 定义了 Tars 二进制协议中使用的 4 位类型标记。
/// 这些标记通常存储在 Tag 字节的低 4 位中。
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
    /// 映射表 (Map) 的开始。
    Map = 8,
    /// 列表 (List) 的开始。
    List = 9,
    /// 自定义结构体 (Struct) 的开始。
    StructBegin = 10,
    /// 自定义结构体的结束。
    StructEnd = 11,
    /// 值为 0 的整数 (用于压缩存储)。
    ZeroTag = 12,
    /// 简单列表 (字节数组 specific optimization)。
    /// 仅用于存储 `byte` 类型的数组 (`vector<byte>`)。
    SimpleList = 13,
}

/// 泛型标记 (非 Tars 规范，仅供内部使用)
pub const TARS_TYPE_GENERIC: u8 = 255;

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

// 常量定义，与 Python 端保持一致
pub const TARS_INT1: u8 = 0;
pub const TARS_INT2: u8 = 1;
pub const TARS_INT4: u8 = 2;
pub const TARS_INT8: u8 = 3;
pub const TARS_FLOAT: u8 = 4;
pub const TARS_DOUBLE: u8 = 5;
pub const TARS_STRING1: u8 = 6;
pub const TARS_STRING4: u8 = 7;
pub const TARS_MAP: u8 = 8;
pub const TARS_LIST: u8 = 9;
pub const TARS_STRUCT_BEGIN: u8 = 10;
pub const TARS_STRUCT_END: u8 = 11;
pub const TARS_ZERO_TAG: u8 = 12;
pub const TARS_SIMPLE_LIST: u8 = 13;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tars_type_values() {
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
    fn test_try_from_u8() {
        assert_eq!(TarsType::try_from(0), Ok(TarsType::Int1));
        assert_eq!(TarsType::try_from(13), Ok(TarsType::SimpleList));
        assert_eq!(TarsType::try_from(14), Err(14));
    }
}
