use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

use crate::data_dir;

#[derive(Deserialize)]
struct SchemaFile {
    #[serde(default)]
    fields: Vec<SchemaField>,
}

#[derive(Deserialize)]
struct SchemaField {
    name: Option<String>,
    #[serde(rename = "type")]
    field_type: Option<String>,
    count: Option<usize>,
    fields: Option<Vec<SchemaField>>,
}

/// 扁平化 schema fields 为列名数组
fn flatten_schema_fields(fields: &[SchemaField], prefix: &str) -> Vec<String> {
    let mut result = Vec::new();
    for field in fields {
        let name = match &field.name {
            Some(n) => {
                if prefix.is_empty() {
                    n.clone()
                } else {
                    format!("{}.{}", prefix, n)
                }
            }
            None => prefix.to_string(),
        };

        match field.field_type.as_deref() {
            Some("array") => {
                let count = field.count.unwrap_or(1);
                let nested = field.fields.as_deref().unwrap_or(&[]);

                if nested.is_empty() {
                    // 简单数组: Name[0] .. Name[N-1]
                    for i in 0..count {
                        result.push(format!("{}[{}]", name, i));
                    }
                } else if nested.len() == 1 && nested[0].name.is_none() {
                    // 单个无名嵌套: Name[0] .. Name[N-1]
                    for i in 0..count {
                        result.push(format!("{}[{}]", name, i));
                    }
                } else {
                    // 多嵌套字段: Name[i].Sub
                    for i in 0..count {
                        let arr_prefix = format!("{}[{}]", name, i);
                        let sub = flatten_schema_fields(nested, &arr_prefix);
                        result.extend(sub);
                    }
                }
            }
            // 标量/link/icon/color/modelId → 1 列
            _ => {
                result.push(name);
            }
        }
    }
    result
}

fn schema_path(name: &str) -> PathBuf {
    data_dir::schema_dir().join(format!("{}.yml", name))
}

fn schema_url(name: &str) -> String {
    format!(
        "https://raw.githubusercontent.com/xivdev/EXDSchema/refs/heads/latest/{}.yml",
        name
    )
}

fn parse_schema_yml(content: &str) -> Option<Vec<String>> {
    let schema: SchemaFile = serde_yml::from_str(content).ok()?;
    Some(flatten_schema_fields(&schema.fields, ""))
}

fn fetch_schema_http(name: &str) -> Result<String, String> {
    let url = schema_url(name);
    let body = ureq::get(&url)
        .call()
        .map_err(|e| format!("HTTP 请求失败: {}", e))?
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("读取响应失败: {}", e))?;
    Ok(body)
}

/// 获取 schema 列名（磁盘缓存优先，miss 时从 HTTP 拉取并保存）
pub fn load_schema(name: &str) -> Option<Vec<String>> {
    let path = schema_path(name);

    // 尝试从磁盘缓存读取
    if let Ok(content) = fs::read_to_string(&path) {
        if let Some(columns) = parse_schema_yml(&content) {
            return Some(columns);
        }
    }

    // 从 HTTP 拉取
    let content = fetch_schema_http(name).ok()?;
    let columns = parse_schema_yml(&content)?;

    // 写入缓存
    let _ = fs::write(&path, &content);

    Some(columns)
}

/// 强制从 HTTP 更新指定表的 schema
pub fn update_schema(name: &str) -> Result<Vec<String>, String> {
    let content = fetch_schema_http(name)?;
    let columns = parse_schema_yml(&content).ok_or_else(|| "解析 YAML 失败".to_string())?;
    let path = schema_path(name);
    fs::write(&path, &content).map_err(|e| format!("写入缓存失败: {}", e))?;
    Ok(columns)
}

/// 强制从 HTTP 更新所有已缓存的 schema
pub fn update_all_schemas() -> usize {
    let dir = data_dir::schema_dir();
    let mut count = 0;
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "yml") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    if update_schema(name).is_ok() {
                        count += 1;
                    }
                }
            }
        }
    }
    count
}
