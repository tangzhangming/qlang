//! 包解析器
//! 
//! 负责解析导入路径，定位源文件

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use crate::config::{STD_PREFIX, STDLIB_DIR, SOURCE_EXTENSION};
use crate::parser::ast::{ImportDecl, ImportTarget};
use super::project::ProjectConfig;

/// 导入类型
#[derive(Debug, Clone, PartialEq)]
pub enum ImportKind {
    /// 标准库（Rust 内置）
    StdBuiltin,
    /// 标准库（Q语言实现）
    StdSource,
    /// 项目内部包
    Project,
    /// 外部依赖
    External,
}

/// 解析后的导入信息
#[derive(Debug, Clone)]
pub struct ResolvedImport {
    /// 原始导入声明
    pub decl: ImportDecl,
    /// 导入类型
    pub kind: ImportKind,
    /// 源文件路径（如果是源码导入）
    pub source_path: Option<PathBuf>,
    /// 解析出的成员列表
    pub members: Vec<String>,
}

/// 包解析器
pub struct PackageResolver {
    /// 项目配置
    project: Option<ProjectConfig>,
    /// 标准库目录
    stdlib_dir: PathBuf,
    /// 内置标准库模块列表
    builtin_modules: HashMap<String, Vec<String>>,
}

impl PackageResolver {
    /// 创建新的包解析器
    pub fn new(project: Option<ProjectConfig>) -> Self {
        // 获取标准库目录（相对于可执行文件或当前目录）
        let stdlib_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
            .join(STDLIB_DIR);
        
        let mut resolver = Self {
            project,
            stdlib_dir,
            builtin_modules: HashMap::new(),
        };
        
        // 注册内置标准库模块
        resolver.register_builtin_modules();
        
        resolver
    }
    
    /// 设置标准库目录
    pub fn set_stdlib_dir(&mut self, path: PathBuf) {
        self.stdlib_dir = path;
    }
    
    /// 注册内置标准库模块
    fn register_builtin_modules(&mut self) {
        // std.Vmtest - Rust 内置模块，提供测试功能
        self.builtin_modules.insert(
            "std.Vmtest".to_string(),
            vec![
                "assert".to_string(),
                "assertEqual".to_string(),
                "assertTrue".to_string(),
                "assertFalse".to_string(),
                "assertNull".to_string(),
                "assertNotNull".to_string(),
                "fail".to_string(),
            ],
        );
        
        // std.lang - Rust 内置模块，提供异常类和语言基础功能
        self.builtin_modules.insert(
            "std.lang".to_string(),
            vec![
                // 基础类
                "Throwable".to_string(),
                "Error".to_string(),
                "Exception".to_string(),
                // RuntimeException 分支
                "RuntimeException".to_string(),
                "NullPointerException".to_string(),
                "IndexOutOfBoundsException".to_string(),
                "IllegalArgumentException".to_string(),
                "ArithmeticException".to_string(),
                // IOException 分支
                "IOException".to_string(),
                // 工具函数
                "isThrowable".to_string(),
                "isException".to_string(),
                "getExceptionType".to_string(),
                "getExceptionMessage".to_string(),
            ],
        );
    }
    
    /// 解析导入声明
    pub fn resolve(&self, import: &ImportDecl) -> Result<ResolvedImport, String> {
        let full_path = match &import.target {
            ImportTarget::All => import.path.clone(),
            ImportTarget::Single(name) => format!("{}.{}", import.path, name),
            ImportTarget::Multiple(_) => import.path.clone(),
        };
        
        // 判断是否是标准库
        if full_path.starts_with(STD_PREFIX) && 
           (full_path.len() == STD_PREFIX.len() || 
            full_path.chars().nth(STD_PREFIX.len()) == Some('.')) {
            return self.resolve_std_import(import);
        }
        
        // 项目内部包
        if let Some(ref project) = self.project {
            if full_path.starts_with(&project.package) {
                return self.resolve_project_import(import, project);
            }
        }
        
        // 外部依赖
        self.resolve_external_import(import)
    }
    
    /// 解析标准库导入
    fn resolve_std_import(&self, import: &ImportDecl) -> Result<ResolvedImport, String> {
        // 根据导入目标判断意图：
        // - import std.Vmtest -> 导入整个模块（module_path = "std.Vmtest"）
        // - import std.Vmtest.* -> 导入模块所有成员（module_path = "std.Vmtest"）
        // - import std.Vmtest.assert -> 导入模块的特定成员（module_path = "std.Vmtest", member = "assert"）
        
        let (module_path, specific_members) = match &import.target {
            ImportTarget::All => {
                // import path.* - path 是模块路径
                (import.path.clone(), None)
            }
            ImportTarget::Single(name) => {
                // 先尝试将 path.name 作为模块路径
                let full_path = format!("{}.{}", import.path, name);
                if self.builtin_modules.contains_key(&full_path) {
                    // path.name 是一个模块，导入整个模块
                    (full_path, None)
                } else if self.builtin_modules.contains_key(&import.path) {
                    // path 是模块，name 是成员
                    (import.path.clone(), Some(vec![name.clone()]))
                } else {
                    // 假设是模块导入
                    (full_path, None)
                }
            }
            ImportTarget::Multiple(names) => {
                // import path.{a, b, c} - path 是模块路径，导入指定成员
                (import.path.clone(), Some(names.clone()))
            }
        };
        
        // 检查是否是内置模块
        if let Some(exports) = self.builtin_modules.get(&module_path) {
            let members = match specific_members {
                None => exports.clone(),  // 导入所有导出成员
                Some(names) => {
                    for name in &names {
                        if !exports.contains(name) {
                            return Err(format!("模块 {} 没有导出成员 {}", module_path, name));
                        }
                    }
                    names
                }
            };
            
            return Ok(ResolvedImport {
                decl: import.clone(),
                kind: ImportKind::StdBuiltin,
                source_path: None,
                members,
            });
        }
        
        // 检查是否是 Q 语言实现的标准库
        // std.Test -> stdlib/Test.q
        let relative_path = module_path.strip_prefix("std.")
            .ok_or_else(|| format!("无效的标准库路径: {}", module_path))?;
        
        // 将点分隔的路径转换为文件路径
        let file_name = format!("{}.{}", relative_path.replace('.', "/"), SOURCE_EXTENSION);
        let source_path = self.stdlib_dir.join(&file_name);
        
        if source_path.exists() {
            let members = match &import.target {
                ImportTarget::All => vec![], // 需要解析源文件获取
                ImportTarget::Single(name) => vec![name.clone()],
                ImportTarget::Multiple(names) => names.clone(),
            };
            
            return Ok(ResolvedImport {
                decl: import.clone(),
                kind: ImportKind::StdSource,
                source_path: Some(source_path),
                members,
            });
        }
        
        Err(format!("找不到标准库模块: {}", module_path))
    }
    
    /// 解析项目内部导入
    fn resolve_project_import(&self, import: &ImportDecl, project: &ProjectConfig) -> Result<ResolvedImport, String> {
        let module_path = match &import.target {
            ImportTarget::All => import.path.clone(),
            ImportTarget::Single(name) => format!("{}.{}", import.path, name),
            ImportTarget::Multiple(_) => import.path.clone(),
        };
        
        // 去除项目包前缀
        let relative_path = module_path.strip_prefix(&project.package)
            .and_then(|s| s.strip_prefix('.'))
            .unwrap_or(&module_path);
        
        // 构建源文件路径
        let file_path = if relative_path.is_empty() {
            // 导入项目根包
            project.root_dir.join(&project.src_dir)
        } else {
            // 导入子包
            let path_parts: Vec<&str> = relative_path.split('.').collect();
            let mut path = project.root_dir.join(&project.src_dir);
            for part in &path_parts[..path_parts.len()-1] {
                path = path.join(part);
            }
            path
        };
        
        // 确定源文件
        let source_file = match &import.target {
            ImportTarget::All => {
                // 导入目录下所有文件
                file_path
            }
            ImportTarget::Single(name) => {
                file_path.join(format!("{}.{}", name, SOURCE_EXTENSION))
            }
            ImportTarget::Multiple(_) => {
                file_path
            }
        };
        
        let members = match &import.target {
            ImportTarget::All => vec![],
            ImportTarget::Single(name) => vec![name.clone()],
            ImportTarget::Multiple(names) => names.clone(),
        };
        
        Ok(ResolvedImport {
            decl: import.clone(),
            kind: ImportKind::Project,
            source_path: Some(source_file),
            members,
        })
    }
    
    /// 解析外部依赖导入
    fn resolve_external_import(&self, import: &ImportDecl) -> Result<ResolvedImport, String> {
        // 外部依赖目前未实现
        let members = match &import.target {
            ImportTarget::All => vec![],
            ImportTarget::Single(name) => vec![name.clone()],
            ImportTarget::Multiple(names) => names.clone(),
        };
        
        Ok(ResolvedImport {
            decl: import.clone(),
            kind: ImportKind::External,
            source_path: None,
            members,
        })
    }
    
    /// 检查是否是内置模块
    pub fn is_builtin(&self, module_path: &str) -> bool {
        self.builtin_modules.contains_key(module_path)
    }
    
    /// 获取内置模块的导出列表
    pub fn get_builtin_exports(&self, module_path: &str) -> Option<&Vec<String>> {
        self.builtin_modules.get(module_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_resolve_std_builtin() {
        let resolver = PackageResolver::new(None);
        
        let import = ImportDecl {
            path: "std".to_string(),
            target: ImportTarget::Single("Vmtest".to_string()),
        };
        
        let result = resolver.resolve(&import).unwrap();
        assert_eq!(result.kind, ImportKind::StdBuiltin);
    }
}
