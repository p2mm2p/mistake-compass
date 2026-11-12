//! 页面层复用的领域类型与测试期目录载入。
//!
//! 生产环境由 WASM 宿主提供题库；目录载入只在测试中使用。

pub use mc_core::{
    Category, Corpus, DirNode, FIELDS, FileNode, Level, Number, Question, QuestionType,
    build_level, build_tree,
};

/// 从目录载入测试题库。
///
/// 递归读取 `.yaml` 文件，并按相对路径生成分类路径。
///
/// # Errors
///
/// 目录读取、文件读取或题库校验失败时返回错误。
#[cfg(test)]
pub fn load(dir: &std::path::Path) -> Result<Corpus, String> {
    let mut paths = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in std::fs::read_dir(&current).map_err(|error| error.to_string())? {
            let path = entry.map_err(|error| error.to_string())?.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "yaml")
            {
                paths.push(path);
            }
        }
    }
    paths.sort();
    let categories = paths
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
            let relative = path
                .strip_prefix(dir)
                .map_err(|error| error.to_string())?
                .with_extension("");
            let category = relative
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            mc_core::parse_category(&category, &text)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Corpus::new(categories)
}
