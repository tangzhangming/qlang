//! 项目配置解析
//! 
//! 解析 project.toml 文件，获取项目配置

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use crate::config::PROJECT_FILE;

/// 项目配置
#[derive(Debug, Clone)]
pub struct ProjectConfig {
    /// 项目名称
    pub name: String,
    /// 项目版本
    pub version: String,
    /// 包名（如 com.example.myapp）
    pub package: String,
    /// 项目根目录
    pub root_dir: PathBuf,
    /// 源码目录（相对于项目根目录）
    pub src_dir: String,
    /// 依赖项
    pub dependencies: HashMap<String, String>,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            version: "0.1.0".to_string(),
            package: String::new(),
            root_dir: PathBuf::new(),
            src_dir: "src".to_string(),
            dependencies: HashMap::new(),
        }
    }
}

impl ProjectConfig {
    /// 从 project.toml 文件加载配置
    pub fn load(project_file: &Path) -> Result<Self, String> {
        let content = fs::read_to_string(project_file)
            .map_err(|e| format!("无法读取项目配置文件: {}", e))?;
        
        Self::parse(&content, project_file.parent().unwrap_or(Path::new(".")))
    }
    
    /// 解析 TOML 内容
    fn parse(content: &str, root_dir: &Path) -> Result<Self, String> {
        let mut config = ProjectConfig::default();
        config.root_dir = root_dir.to_path_buf();
        
        // 简单的 TOML 解析（不依赖外部库）
        let mut current_section = "";
        
        for line in content.lines() {
            let line = line.trim();
            
            // 跳过空行和注释
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            
            // 检查节标题
            if line.starts_with('[') && line.ends_with(']') {
                current_section = &line[1..line.len()-1];
                continue;
            }
            
            // 解析键值对
            if let Some((key, value)) = parse_key_value(line) {
                match current_section {
                    "" | "project" => {
                        match key {
                            "name" => config.name = value,
                            "version" => config.version = value,
                            "package" => config.package = value,
                            "src" => config.src_dir = value,
                            _ => {}
                        }
                    }
                    "dependencies" => {
                        config.dependencies.insert(key.to_string(), value);
                    }
                    _ => {}
                }
            }
        }
        
        // 验证必需字段
        if config.name.is_empty() {
            return Err("项目配置缺少 name 字段".to_string());
        }
        if config.package.is_empty() {
            // 如果没有指定 package，使用 name
            config.package = config.name.clone();
        }
        
        Ok(config)
    }
}

/// 解析键值对 "key = value" 或 "key = \"value\""
fn parse_key_value(line: &str) -> Option<(&str, String)> {
    let parts: Vec<&str> = line.splitn(2, '=').collect();
    if parts.len() != 2 {
        return None;
    }
    
    let key = parts[0].trim();
    let mut value = parts[1].trim();
    
    // 去除引号
    if value.starts_with('"') && value.ends_with('"') {
        value = &value[1..value.len()-1];
    }
    
    Some((key, value.to_string()))
}

/// 从指定路径向上查找项目根目录
/// 
/// 查找逻辑与 Go 相同：从入口文件所在目录向上查找，
/// 直到找到包含 project.toml 的目录或到达文件系统根目录
pub fn find_project_root(start_path: &Path) -> Option<PathBuf> {
    let mut current = if start_path.is_file() {
        start_path.parent()?.to_path_buf()
    } else {
        start_path.to_path_buf()
    };
    
    loop {
        let project_file = current.join(PROJECT_FILE);
        if project_file.exists() {
            return Some(current);
        }
        
        // 向上一级目录
        match current.parent() {
            Some(parent) => {
                if parent == current {
                    // 已到达根目录
                    return None;
                }
                current = parent.to_path_buf();
            }
            None => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_project_config() {
        let content = r#"
[project]
name = "myapp"
version = "1.0.0"
package = "com.example.myapp"
src = "src"

[dependencies]
std = "1.0"
"#;
        
        let config = ProjectConfig::parse(content, Path::new(".")).unwrap();
        assert_eq!(config.name, "myapp");
        assert_eq!(config.version, "1.0.0");
        assert_eq!(config.package, "com.example.myapp");
        assert_eq!(config.src_dir, "src");
        assert_eq!(config.dependencies.get("std"), Some(&"1.0".to_string()));
    }
    
    #[test]
    fn test_parse_minimal_config() {
        let content = r#"
name = "minimal"
"#;
        
        let config = ProjectConfig::parse(content, Path::new(".")).unwrap();
        assert_eq!(config.name, "minimal");
        assert_eq!(config.package, "minimal"); // 默认使用 name
    }
}
