//! SGB (Scene Group Binary) 文件解析器
//! 从 SGB 文件中提取引用的 MDL 模型路径

use std::io::{Cursor, Read, Seek, SeekFrom};

/// 从 SGB 文件数据中提取所有 .mdl 路径
pub fn extract_mdl_paths_from_sgb(data: &[u8]) -> Vec<String> {
    let mut paths = Vec::new();

    let mut c = Cursor::new(data);

    // SGB 头部: 跳到字符串区域
    // 参考 TexTools: seek(20), read offset, seek(skip+4), read stringsOffset
    if c.seek(SeekFrom::Start(20)).is_err() {
        return paths;
    }

    let skip = match read_i32(&mut c) {
        Ok(v) => v,
        Err(_) => return paths,
    };

    let target = (skip + 20 + 4) as u64;
    if c.seek(SeekFrom::Start(target)).is_err() {
        return paths;
    }

    let strings_offset = match read_i32(&mut c) {
        Ok(v) => v,
        Err(_) => return paths,
    };

    let strings_start = (skip + 20) as u64 + strings_offset as u64;
    if c.seek(SeekFrom::Start(strings_start)).is_err() {
        return paths;
    }

    // 读取以 null 分隔的字符串
    loop {
        let mut path_bytes = Vec::new();
        loop {
            let mut b = [0u8; 1];
            match c.read_exact(&mut b) {
                Ok(_) => {}
                Err(_) => return paths,
            }
            if b[0] == 0xFF {
                return paths;
            }
            if b[0] == 0 {
                break;
            }
            path_bytes.push(b[0]);
        }

        if path_bytes.is_empty() {
            continue;
        }

        if let Ok(path) = std::str::from_utf8(&path_bytes) {
            let path = path.replace('\0', "");
            if path.ends_with(".mdl") {
                paths.push(path);
            }
        }
    }
}

fn read_i32(c: &mut Cursor<&[u8]>) -> Result<i32, String> {
    let mut b = [0u8; 4];
    c.read_exact(&mut b).map_err(|e| format!("read_i32: {e}"))?;
    Ok(i32::from_le_bytes(b))
}
