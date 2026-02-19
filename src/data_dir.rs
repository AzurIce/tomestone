use std::path::PathBuf;

/// 获取数据根目录
pub fn data_root() -> PathBuf {
    let root = if cfg!(debug_assertions) {
        PathBuf::from("./.tomestone")
    } else {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".tomestone")
    };
    let _ = std::fs::create_dir_all(&root);
    root
}

/// 获取子目录（自动创建）
pub fn data_subdir(name: &str) -> PathBuf {
    let dir = data_root().join(name);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn glamours_dir() -> PathBuf {
    data_subdir("glamours")
}

pub fn schema_dir() -> PathBuf {
    data_subdir("schema")
}
