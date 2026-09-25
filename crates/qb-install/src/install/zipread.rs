//! 读 zip —— 先列目录、挑好了再解，挑剩的一个字节都不落盘（2026-09-25，Claude 汉化插件用）。
//!
//! # 为什么不用 [`super::managed::unzip`]
//!
//! 那个是 .NET `ZipFile.ExtractToDirectory`：整包解开。汉化插件只要上游
//! `javaht/claude-desktop-zh-cn` 归档里 Windows 安全模式用得到的那一小部分
//! （安装脚本与翻译 JSON），而同一个归档里还有 Frida 内存补丁、绕过 Claude 调试开关闸门的脚本 ——
//! 整包解到磁盘上，就等于把面板**永远不会调用**的那些东西也落到了使用者机器上
//! （杀软看到的是「一个没签名的程序把一堆注入脚本写进了自己的目录」）。
//!
//! 顺带拿到 GitHub 写在源码归档**注释**里的提交号：不用为了「这一版是哪个提交」
//! 再去问一次 `api.github.com`（未登录一个出口 IP 一小时 60 次，0.25.3 self_update 同一个理由）。
//!
//! # 只认最简单的那一种
//!
//! GitHub 的源码归档：不加密、不分卷、不用 zip64，条目要么「存储」要么 deflate。
//! 超出这些的一律报错，**不猜** —— 认不出来的压缩包不该被半懂不懂地解出一半。
//! 每个条目解完都核 CRC-32 与声明的大小，对不上就报错。
use crate::error::{GateError, Result};

/// 目录里的一个条目。偏移、压缩方式这些只给 [`extract`] 用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// 压缩包里的路径（`/` 分隔）。目录条目以 `/` 结尾。
    pub name: String,
    /// 解压后的字节数（中央目录里声明的）。
    pub size: u64,
    method: u16,
    compressed: u64,
    crc: u32,
    offset: u64,
}

impl Entry {
    pub fn is_dir(&self) -> bool {
        self.name.ends_with('/')
    }
}

/// 整个压缩包的目录。
#[derive(Debug, Clone)]
pub struct Archive {
    pub entries: Vec<Entry>,
    /// 归档注释。GitHub 的源码归档在这里写提交号（40 位十六进制）。
    pub comment: String,
}

impl Archive {
    /// 注释是 40 位十六进制时，那就是 GitHub 写的提交号。
    pub fn commit(&self) -> Option<&str> {
        let c = self.comment.trim();
        (c.len() == 40 && c.bytes().all(|b| b.is_ascii_hexdigit())).then_some(c)
    }
}

const EOCD: u32 = 0x0605_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const LOCAL: u32 = 0x0403_4b50;

fn u16_at(b: &[u8], at: usize) -> Result<u16> {
    b.get(at..at + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or_else(truncated)
}

fn u32_at(b: &[u8], at: usize) -> Result<u32> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(truncated)
}

fn truncated() -> GateError {
    GateError::Other("压缩包不完整（读到一半就没了）".into())
}

/// 读中央目录。**纯函数**：只看给它的字节。
pub fn read_index(bytes: &[u8]) -> Result<Archive> {
    // 目录结尾记录在文件最后 22 字节 + 最长 65535 字节的注释之内。
    let floor = bytes.len().saturating_sub(22 + 0xFFFF);
    let eocd = (floor..=bytes.len().saturating_sub(22))
        .rev()
        .find(|&i| u32_at(bytes, i).ok() == Some(EOCD))
        .ok_or_else(|| GateError::Other("不是 zip 压缩包（找不到目录结尾）".into()))?;
    let count = u16_at(bytes, eocd + 10)? as usize;
    let cd_size = u32_at(bytes, eocd + 12)?;
    let cd_offset = u32_at(bytes, eocd + 16)?;
    let comment_len = u16_at(bytes, eocd + 20)? as usize;
    if count == 0xFFFF || cd_size == u32::MAX || cd_offset == u32::MAX {
        return Err(GateError::Other("zip64 压缩包，不认".into()));
    }
    let comment = bytes
        .get(eocd + 22..eocd + 22 + comment_len)
        .ok_or_else(truncated)?;
    let comment = String::from_utf8_lossy(comment).into_owned();

    let mut entries = Vec::with_capacity(count);
    let mut at = cd_offset as usize;
    for _ in 0..count {
        if u32_at(bytes, at)? != CENTRAL {
            return Err(GateError::Other("压缩包目录损坏".into()));
        }
        let flags = u16_at(bytes, at + 8)?;
        let method = u16_at(bytes, at + 10)?;
        let crc = u32_at(bytes, at + 16)?;
        let compressed = u32_at(bytes, at + 20)?;
        let size = u32_at(bytes, at + 24)?;
        let name_len = u16_at(bytes, at + 28)? as usize;
        let extra_len = u16_at(bytes, at + 30)? as usize;
        let note_len = u16_at(bytes, at + 32)? as usize;
        let offset = u32_at(bytes, at + 42)?;
        let name = bytes
            .get(at + 46..at + 46 + name_len)
            .ok_or_else(truncated)?;
        let name = String::from_utf8_lossy(name).into_owned();
        if flags & 1 != 0 {
            return Err(GateError::Other(format!("「{name}」是加密条目，不认")));
        }
        if compressed == u32::MAX || size == u32::MAX || offset == u32::MAX {
            return Err(GateError::Other("zip64 条目，不认".into()));
        }
        entries.push(Entry {
            name,
            size: size as u64,
            method,
            compressed: compressed as u64,
            crc,
            offset: offset as u64,
        });
        at += 46 + name_len + extra_len + note_len;
    }
    Ok(Archive { entries, comment })
}

/// 解出一个条目的内容。**纯函数**。核 CRC-32 与大小，对不上就报错。
///
/// `max` 是这一个条目最多接受多少字节 —— 声明的大小超过它直接拒绝，
/// 解压时也只多读一个字节用来发现「声明的比实际的小」（防压缩炸弹）。
pub fn extract(bytes: &[u8], entry: &Entry, max: u64) -> Result<Vec<u8>> {
    use std::io::Read;
    if entry.size > max {
        return Err(GateError::Other(format!(
            "「{}」有 {} 字节，超过 {} 字节的上限",
            entry.name, entry.size, max
        )));
    }
    let at = entry.offset as usize;
    if u32_at(bytes, at)? != LOCAL {
        return Err(GateError::Other(format!("「{}」的本地头损坏", entry.name)));
    }
    let name_len = u16_at(bytes, at + 26)? as usize;
    let extra_len = u16_at(bytes, at + 28)? as usize;
    let start = at + 30 + name_len + extra_len;
    let data = bytes
        .get(start..start + entry.compressed as usize)
        .ok_or_else(truncated)?;
    let out = match entry.method {
        0 => data.to_vec(),
        8 => {
            let mut out = Vec::with_capacity(entry.size as usize);
            flate2::read::DeflateDecoder::new(data)
                .take(entry.size + 1)
                .read_to_end(&mut out)
                .map_err(|e| GateError::Other(format!("「{}」解压失败：{e}", entry.name)))?;
            out
        }
        m => {
            return Err(GateError::Other(format!(
                "「{}」用的压缩方式 {m} 不认（只认存储与 deflate）",
                entry.name
            )))
        }
    };
    if out.len() as u64 != entry.size {
        return Err(GateError::Other(format!(
            "「{}」解出来 {} 字节，目录里写的是 {} 字节",
            entry.name,
            out.len(),
            entry.size
        )));
    }
    let mut crc = flate2::Crc::new();
    crc.update(&out);
    if crc.sum() != entry.crc {
        return Err(GateError::Other(format!(
            "「{}」的 CRC-32 对不上，压缩包损坏",
            entry.name
        )));
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    /// 拼一个最小的 zip：`(名字, 内容, 要不要 deflate)`，外加归档注释。
    /// 跟 GitHub 源码归档同一种形状（没有数据描述符、没有 zip64）。
    pub(crate) fn build(files: &[(&str, &[u8], bool)], comment: &str) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, body, deflate) in files {
            let mut crc = flate2::Crc::new();
            crc.update(body);
            let data = if *deflate {
                let mut enc =
                    flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
                enc.write_all(body).unwrap();
                enc.finish().unwrap()
            } else {
                body.to_vec()
            };
            let method: u16 = if *deflate { 8 } else { 0 };
            let offset = out.len() as u32;
            out.extend_from_slice(&LOCAL.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes()); // version
            out.extend_from_slice(&0u16.to_le_bytes()); // flags
            out.extend_from_slice(&method.to_le_bytes());
            out.extend_from_slice(&[0; 4]); // time/date
            out.extend_from_slice(&crc.sum().to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(body.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // extra
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&data);

            central.extend_from_slice(&CENTRAL.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes()); // made by
            central.extend_from_slice(&20u16.to_le_bytes()); // needed
            central.extend_from_slice(&0u16.to_le_bytes()); // flags
            central.extend_from_slice(&method.to_le_bytes());
            central.extend_from_slice(&[0; 4]);
            central.extend_from_slice(&crc.sum().to_le_bytes());
            central.extend_from_slice(&(data.len() as u32).to_le_bytes());
            central.extend_from_slice(&(body.len() as u32).to_le_bytes());
            central.extend_from_slice(&(name.len() as u16).to_le_bytes());
            central.extend_from_slice(&[0; 4]); // extra + comment len
            central.extend_from_slice(&[0; 4]); // disk + internal attrs
            central.extend_from_slice(&[0; 4]); // external attrs
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
        }
        let cd_offset = out.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&EOCD.to_le_bytes());
        out.extend_from_slice(&[0; 4]); // disk numbers
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&(comment.len() as u16).to_le_bytes());
        out.extend_from_slice(comment.as_bytes());
        out
    }

    const SHA: &str = "4bb466d9fedcdd87fc5338f243bd53e28daf211d";

    #[test]
    fn it_lists_and_extracts_stored_and_deflated_entries() {
        let big = "安装脚本第一行\n".repeat(400);
        let zip = build(
            &[
                ("repo-1.0/", b"", false),
                ("repo-1.0/LICENSE", b"MIT License\n", false),
                ("repo-1.0/scripts/install.ps1", big.as_bytes(), true),
            ],
            SHA,
        );
        let a = read_index(&zip).unwrap();
        assert_eq!(a.commit(), Some(SHA));
        assert_eq!(a.entries.len(), 3);
        assert!(a.entries[0].is_dir());
        assert_eq!(
            extract(&zip, &a.entries[1], 1 << 20).unwrap(),
            b"MIT License\n"
        );
        assert_eq!(
            extract(&zip, &a.entries[2], 1 << 20).unwrap(),
            big.as_bytes()
        );
    }

    #[test]
    fn a_comment_that_is_not_a_commit_is_not_reported_as_one() {
        let zip = build(&[("a/x", b"x", false)], "made by someone");
        assert_eq!(read_index(&zip).unwrap().commit(), None);
    }

    #[test]
    fn a_corrupted_entry_is_an_error_not_garbage() {
        let mut zip = build(&[("a/x.json", b"{\"k\":\"v\"}", false)], "");
        // 改掉内容里的一个字节：CRC 必须对不上。
        let at = zip.windows(3).position(|w| w == b"\"k\"").unwrap();
        zip[at + 1] = b'K';
        let a = read_index(&zip).unwrap();
        let err = extract(&zip, &a.entries[0], 1 << 20).unwrap_err();
        assert!(err.to_string().contains("CRC"), "{err}");
    }

    #[test]
    fn oversized_entries_are_refused_before_decompressing() {
        let zip = build(&[("a/big", &[b'x'; 4096], true)], "");
        let a = read_index(&zip).unwrap();
        assert!(extract(&zip, &a.entries[0], 1024).is_err());
    }

    #[test]
    fn not_a_zip_and_truncated_zips_are_errors() {
        assert!(read_index(b"<html>rate limited</html>").is_err());
        let zip = build(&[("a/x", b"hello", false)], SHA);
        assert!(read_index(&zip[..zip.len() / 2]).is_err());
    }
}
