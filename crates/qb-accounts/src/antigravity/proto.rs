//! 最小的 protobuf 线格式读取器 —— 只读字段、不认 schema。**纯函数，无 I/O。**
//!
//! # 为什么自己写而不是引 `prost`
//!
//! 反重力落盘的东西（用户状态、生成记录）没有公开的 `.proto`。我们只知道**几个字段号**
//! 长什么样（实机对着 12,410 条记录数出来的，见 [`super::usage`] 与 [`super::status`] 文件头），
//! 而且下一版可能就多几个字段。按线格式逐个 tag 走、遇到不认识的跳过，比按一份自己猜的
//! schema 生成代码更稳：schema 猜错一处，整条消息就解不出来；这里猜错只是少一个字段。
//!
//! # 只做四种线类型
//!
//! varint（0）、64 位（1）、长度前缀（2）、32 位（5）。已废弃的 group（3 / 4）一律当作
//! 损坏 —— Google 现役的 proto3 不产生它们，遇到就说明这不是一条我们认识的消息。
//!
//! # ⛔ 「解不出来」必须是错误，不是空
//!
//! 截断、越界、未知线类型都返回 `Err`。调用方拿 `Err` 去计「没读成的文件」，
//! 拿 `Ok(空)` 就会把损坏的记录当成「零 token」静默吞掉（§7.17 那一族）。

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtoError(pub &'static str);

impl fmt::Display for ProtoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "protobuf 解析失败：{}", self.0)
    }
}

impl std::error::Error for ProtoError {}

/// 一个字段的值，按线类型分。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Wire<'a> {
    Varint(u64),
    Fixed64(u64),
    Bytes(&'a [u8]),
    Fixed32(u32),
}

/// 一个已解出的字段：字段号 + 值。重复字段按出现顺序各占一条。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Field<'a> {
    pub number: u32,
    pub value: Wire<'a>,
}

/// 一条消息的全部字段（顺序保留）。
#[derive(Debug, Clone, PartialEq)]
pub struct Message<'a> {
    fields: Vec<Field<'a>>,
}

fn varint(buf: &[u8], pos: &mut usize) -> Result<u64, ProtoError> {
    let mut out: u64 = 0;
    let mut shift = 0u32;
    loop {
        let byte = *buf.get(*pos).ok_or(ProtoError("varint 被截断"))?;
        *pos += 1;
        if shift >= 64 {
            return Err(ProtoError("varint 超过 64 位"));
        }
        out |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(out);
        }
        shift += 7;
    }
}

impl<'a> Message<'a> {
    /// 按线格式把整段字节解成字段表。任何损坏都是 `Err`。
    pub fn parse(buf: &'a [u8]) -> Result<Self, ProtoError> {
        let mut fields = Vec::new();
        let mut pos = 0usize;
        while pos < buf.len() {
            let tag = varint(buf, &mut pos)?;
            let number = u32::try_from(tag >> 3).map_err(|_| ProtoError("字段号超出范围"))?;
            if number == 0 {
                return Err(ProtoError("字段号为 0"));
            }
            let value = match tag & 0x7 {
                0 => Wire::Varint(varint(buf, &mut pos)?),
                1 => {
                    let end = pos.checked_add(8).ok_or(ProtoError("64 位字段越界"))?;
                    let bytes = buf.get(pos..end).ok_or(ProtoError("64 位字段被截断"))?;
                    pos = end;
                    Wire::Fixed64(u64::from_le_bytes(bytes.try_into().unwrap()))
                }
                2 => {
                    let len = usize::try_from(varint(buf, &mut pos)?)
                        .map_err(|_| ProtoError("长度前缀超出范围"))?;
                    let end = pos.checked_add(len).ok_or(ProtoError("长度前缀越界"))?;
                    let bytes = buf.get(pos..end).ok_or(ProtoError("长度前缀字段被截断"))?;
                    pos = end;
                    Wire::Bytes(bytes)
                }
                5 => {
                    let end = pos.checked_add(4).ok_or(ProtoError("32 位字段越界"))?;
                    let bytes = buf.get(pos..end).ok_or(ProtoError("32 位字段被截断"))?;
                    pos = end;
                    Wire::Fixed32(u32::from_le_bytes(bytes.try_into().unwrap()))
                }
                _ => return Err(ProtoError("不支持的线类型（group）")),
            };
            fields.push(Field { number, value });
        }
        Ok(Self { fields })
    }

    pub fn fields(&self) -> &[Field<'a>] {
        &self.fields
    }

    /// 第一个叫这个号的 varint。
    pub fn varint(&self, number: u32) -> Option<u64> {
        self.fields.iter().find_map(|f| match f {
            Field {
                number: n,
                value: Wire::Varint(v),
            } if *n == number => Some(*v),
            _ => None,
        })
    }

    /// 第一个叫这个号的 32 位定长字段，按 `float` 读。
    pub fn f32(&self, number: u32) -> Option<f32> {
        self.fields.iter().find_map(|f| match f {
            Field {
                number: n,
                value: Wire::Fixed32(v),
            } if *n == number => Some(f32::from_bits(*v)),
            _ => None,
        })
    }

    /// 第一个叫这个号的长度前缀字段的原始字节。
    pub fn bytes(&self, number: u32) -> Option<&'a [u8]> {
        self.all_bytes(number).next()
    }

    /// 所有叫这个号的长度前缀字段（repeated）。
    pub fn all_bytes(&self, number: u32) -> impl Iterator<Item = &'a [u8]> + '_ {
        self.fields.iter().filter_map(move |f| match f {
            Field {
                number: n,
                value: Wire::Bytes(b),
            } if *n == number => Some(*b),
            _ => None,
        })
    }

    /// 第一个叫这个号的字段按 UTF-8 字符串读；不是合法 UTF-8 就当没有。
    pub fn str(&self, number: u32) -> Option<&'a str> {
        self.bytes(number).and_then(|b| std::str::from_utf8(b).ok())
    }

    /// 第一个叫这个号的字段按子消息解。
    pub fn sub(&self, number: u32) -> Option<Message<'a>> {
        self.bytes(number).and_then(|b| Message::parse(b).ok())
    }

    /// 所有叫这个号的字段各按子消息解；解不出的跳过。
    pub fn all_sub(&self, number: u32) -> impl Iterator<Item = Message<'a>> + '_ {
        self.all_bytes(number)
            .filter_map(|b| Message::parse(b).ok())
    }
}

/// `google.protobuf.Timestamp`：`1` = 秒，`2` = 纳秒。只要秒。
pub fn timestamp_secs(buf: &[u8]) -> Option<i64> {
    let m = Message::parse(buf).ok()?;
    let secs = m.varint(1)?;
    i64::try_from(secs).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 手工拼一条：1: varint 150，2: "hi"，3: fixed32 1.0f，4: fixed64 7，5: 子消息 {1: 3}。
    fn sample() -> Vec<u8> {
        let mut b = vec![0x08, 0x96, 0x01];
        b.extend([0x12, 0x02, b'h', b'i']);
        b.push(0x1d);
        b.extend(1.0f32.to_bits().to_le_bytes());
        b.push(0x21);
        b.extend(7u64.to_le_bytes());
        b.extend([0x2a, 0x02, 0x08, 0x03]);
        b
    }

    #[test]
    fn reads_every_wire_type_by_number() {
        let bytes = sample();
        let m = Message::parse(&bytes).unwrap();
        assert_eq!(m.varint(1), Some(150));
        assert_eq!(m.str(2), Some("hi"));
        assert_eq!(m.f32(3), Some(1.0));
        assert_eq!(
            m.fields().iter().find(|f| f.number == 4).map(|f| f.value),
            Some(Wire::Fixed64(7))
        );
        assert_eq!(m.sub(5).and_then(|s| s.varint(1)), Some(3));
        assert_eq!(m.varint(9), None, "没有的字段是 None，不是 0");
    }

    #[test]
    fn repeated_fields_keep_every_occurrence_in_order() {
        let bytes = [0x0a, 0x01, b'a', 0x0a, 0x01, b'b', 0x0a, 0x01, b'c'];
        let m = Message::parse(&bytes).unwrap();
        let all: Vec<&[u8]> = m.all_bytes(1).collect();
        assert_eq!(all, vec![b"a".as_slice(), b"b", b"c"]);
        assert_eq!(m.str(1), Some("a"), "单值取第一个");
    }

    /// ⛔ 截断是错误，不是「空消息」。
    #[test]
    fn truncation_and_groups_are_errors_not_empty_messages() {
        assert!(Message::parse(&[0x12, 0x05, b'h']).is_err(), "长度前缀越界");
        assert!(Message::parse(&[0x08]).is_err(), "varint 没写完");
        assert!(
            Message::parse(&[0x0d, 0x01]).is_err(),
            "fixed32 只有一个字节"
        );
        assert!(Message::parse(&[0x0b]).is_err(), "group 开始（线类型 3）");
        assert!(Message::parse(&[0x00, 0x01]).is_err(), "字段号 0");
        assert_eq!(
            Message::parse(&[]).unwrap().fields().len(),
            0,
            "真空的消息是 Ok(空)"
        );
    }

    #[test]
    fn timestamp_takes_only_the_seconds() {
        // {1: 1786793933, 2: 5}
        let mut b = vec![0x08];
        let mut v = 1_786_793_933u64;
        while v >= 0x80 {
            b.push((v as u8 & 0x7f) | 0x80);
            v >>= 7;
        }
        b.push(v as u8);
        b.extend([0x10, 0x05]);
        assert_eq!(timestamp_secs(&b), Some(1_786_793_933));
        assert_eq!(timestamp_secs(&[0x10, 0x05]), None, "没有秒就是没有");
        assert_eq!(timestamp_secs(&[0xff]), None);
    }
}
