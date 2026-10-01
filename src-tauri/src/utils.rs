//! 后端共享通用纯函数工具底座。
//!
//! 包含无状态的日志格式化、路径解析、URL 预处理与本地化判断等辅助函数。

#![allow(dead_code)]

use std::collections::HashMap;
use std::path::Path;

/// 截取错误消息或文本的首行并限制最大长度为 160 个字符。
///
/// 提取第一行（若不存在则为空字符串）并进行首尾空白字符修剪（trim）。
/// 若修剪后字符数超过 160，则截取前 160 个字符，避免日志过长或跨行。
pub fn short_reason(s: &str) -> String {
    const MAX_CHARS: usize = 160;
    let first = s.lines().next().unwrap_or("").trim();
    if first.chars().count() > MAX_CHARS {
        first.chars().take(MAX_CHARS).collect()
    } else {
        first.to_string()
    }
}

/// 提取文件路径中的文件名（基础名称），避免在日志中输出完整绝对路径。
///
/// 若无法提取有效的文件名字符串，则回退返回 `"?"`。
pub fn file_base(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("?")
        .to_string()
}

/// 将失败类别及其出现次数按键名升序格式化为日志后缀片段。
///
/// 若字典为空，返回空字符串。
/// 若包含键值对，则输出形式如 `" fail_kinds={k1:v1,k2:v2}"`（注意前导空格）。
pub fn format_fail_kinds(map: &HashMap<String, usize>) -> String {
    if map.is_empty() {
        return String::new();
    }
    let mut pairs: Vec<(&String, &usize)> = map.iter().collect();
    pairs.sort_by(|a, b| a.0.cmp(b.0));
    let inner: Vec<String> = pairs.into_iter().map(|(k, v)| format!("{k}:{v}")).collect();
    format!(" fail_kinds={{{}}}", inner.join(","))
}

/// 确保 URL 带有协议方案前缀，若不含 `"://"` 则默认补全 `"https://"`。
///
/// 并在补全前进行首尾空白字符修剪（trim）。
pub fn ensure_https_scheme(url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    }
}

/// 检查指定的语言区域代码是否为中文（以 `"zh"` 开头，例如 `"zh"`、`"zh-CN"`、`"zh-TW"`）。
pub fn is_zh_locale(locale: &str) -> bool {
    locale.starts_with("zh")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_short_reason() {
        assert_eq!(short_reason("hello world\nsecond line"), "hello world");
        assert_eq!(short_reason("   trim me   \nnext"), "trim me");
        assert_eq!(short_reason(""), "");
        let long = "a".repeat(200);
        assert_eq!(short_reason(&long).len(), 160);
    }

    #[test]
    fn test_file_base() {
        assert_eq!(file_base("/path/to/settings.json"), "settings.json");
        assert_eq!(file_base("C:\\data\\config.toml"), "config.toml");
        assert_eq!(file_base("simple.txt"), "simple.txt");
        assert_eq!(file_base("/"), "?");
    }

    #[test]
    fn test_format_fail_kinds() {
        let mut map = HashMap::new();
        assert_eq!(format_fail_kinds(&map), "");

        map.insert("timeout".to_string(), 2);
        map.insert("auth".to_string(), 1);
        assert_eq!(format_fail_kinds(&map), " fail_kinds={auth:1,timeout:2}");
    }

    #[test]
    fn test_ensure_https_scheme() {
        assert_eq!(ensure_https_scheme("example.com/rss"), "https://example.com/rss");
        assert_eq!(ensure_https_scheme("  example.com  "), "https://example.com");
        assert_eq!(ensure_https_scheme("http://example.com"), "http://example.com");
        assert_eq!(ensure_https_scheme("https://example.com"), "https://example.com");
        assert_eq!(ensure_https_scheme("custom://feed"), "custom://feed");
    }

    #[test]
    fn test_is_zh_locale() {
        assert!(is_zh_locale("zh"));
        assert!(is_zh_locale("zh-CN"));
        assert!(is_zh_locale("zh-TW"));
        assert!(!is_zh_locale("en"));
        assert!(!is_zh_locale("en-US"));
        assert!(!is_zh_locale("ja"));
    }
}
