use serde::Deserialize;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::{mpsc, Arc};

use crate::ui::components::{ProgressTracker, ProgressUnit};

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
                    for i in 0..count {
                        result.push(format!("{}[{}]", name, i));
                    }
                } else if nested.len() == 1 && nested[0].name.is_none() {
                    for i in 0..count {
                        result.push(format!("{}[{}]", name, i));
                    }
                } else {
                    for i in 0..count {
                        let arr_prefix = format!("{}[{}]", name, i);
                        let sub = flatten_schema_fields(nested, &arr_prefix);
                        result.extend(sub);
                    }
                }
            }
            _ => {
                result.push(name);
            }
        }
    }
    result
}

fn normalize_name_for_file(name: &str) -> String {
    name.replace('/', "__")
}

fn normalize_name_for_url(name: &str) -> String {
    name.replace('/', "%2F")
}

fn schema_path(name: &str) -> PathBuf {
    let safe_name = normalize_name_for_file(name);
    crate::config::schema_dir().join(format!("{}.yml", safe_name))
}

fn schema_url(name: &str) -> String {
    let encoded = normalize_name_for_url(name);
    format!(
        "https://raw.githubusercontent.com/xivdev/EXDSchema/refs/heads/latest/{}.yml",
        encoded
    )
}

fn parse_schema_yml(content: &str) -> Option<Vec<String>> {
    let schema: SchemaFile = serde_yml::from_str(content).ok()?;
    Some(flatten_schema_fields(&schema.fields, ""))
}

fn fetch_schema_http_with_progress(
    name: &str,
    tracker: &ProgressTracker,
) -> Result<String, String> {
    let url = schema_url(name);

    tracker.set_indeterminate();

    let response = ureq::get(&url)
        .call()
        .map_err(|e| format!("HTTP 请求失败: {}", e))?;

    let content_length = response
        .headers()
        .get("Content-Length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    if content_length > 0 {
        tracker.set_length(content_length);
        tracker.set_position(0);
    }

    let mut reader = response.into_body().into_reader();
    let mut buffer = vec![0u8; 8192];
    let mut total_read = 0u64;
    let mut result = Vec::new();

    loop {
        let n = reader
            .read(&mut buffer)
            .map_err(|e| format!("读取响应失败: {}", e))?;
        if n == 0 {
            break;
        }
        result.extend_from_slice(&buffer[..n]);
        total_read += n as u64;
        if content_length > 0 {
            tracker.set_position(total_read);
        }
    }

    if content_length == 0 {
        tracker.set_length(total_read);
        tracker.set_position(total_read);
    }

    String::from_utf8(result).map_err(|e| format!("UTF-8 解码失败: {}", e))
}

pub fn load_schema_from_cache(name: &str) -> Option<Vec<String>> {
    let path = schema_path(name);
    let content = fs::read_to_string(&path).ok()?;
    parse_schema_yml(&content)
}

pub struct SchemaTaskRunner {
    tracker: Arc<ProgressTracker>,
}

impl SchemaTaskRunner {
    pub fn new() -> Self {
        Self {
            tracker: Arc::new(ProgressTracker::new()),
        }
    }

    pub fn tracker(&self) -> ProgressTracker {
        (*self.tracker).clone()
    }

    pub fn spawn_fetch(&self, name: String) -> mpsc::Receiver<Result<Vec<String>, String>> {
        let (result_tx, result_rx) = mpsc::channel();
        let tracker = self.tracker.clone();

        std::thread::spawn(move || {
            tracker.clear();
            tracker.set_message(format!("正在下载 {}...", name));

            match fetch_schema_http_with_progress(&name, &tracker) {
                Ok(content) => match parse_schema_yml(&content) {
                    Some(columns) => {
                        let path = schema_path(&name);
                        let _ = fs::write(&path, &content);
                        tracker.set_message("下载完成");
                        tracker.set_completed();
                        let _ = result_tx.send(Ok(columns));
                    }
                    None => {
                        tracker.set_failed("解析 YAML 失败");
                        let _ = result_tx.send(Err("解析 YAML 失败".to_string()));
                    }
                },
                Err(e) => {
                    tracker.set_failed(format!("下载失败: {}", e));
                    let _ = result_tx.send(Err(e));
                }
            }
        });

        result_rx
    }

    pub fn spawn_fetch_all(&self, names: Vec<String>) -> mpsc::Receiver<usize> {
        let (result_tx, result_rx) = mpsc::channel();
        let tracker = self.tracker.clone();

        std::thread::spawn(move || {
            tracker.clear();
            let total = names.len() as u64;
            tracker.set_unit(ProgressUnit::Count);
            tracker.set_length(total);

            let mut count = 0;
            for (i, name) in names.iter().enumerate() {
                tracker.set_message(format!("正在下载 {}/{}: {}", i + 1, total, name));

                match fetch_schema_http_with_progress(name, &tracker) {
                    Ok(content) => {
                        if parse_schema_yml(&content).is_some() {
                            let path = schema_path(name);
                            let _ = fs::write(&path, &content);
                            count += 1;
                        }
                    }
                    Err(_) => {}
                }
                tracker.set_position((i + 1) as u64);
            }

            tracker.set_message(format!("已更新 {} / {} 个 Schema", count, total));
            tracker.set_completed();
            let _ = result_tx.send(count);
        });

        result_rx
    }
}

impl Default for SchemaTaskRunner {
    fn default() -> Self {
        Self::new()
    }
}

pub fn cached_schema_names() -> Vec<String> {
    let dir = crate::config::schema_dir();
    let mut names = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "yml") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    let original_name = name.replace("__", "/");
                    names.push(original_name);
                }
            }
        }
    }
    names.sort();
    names
}
