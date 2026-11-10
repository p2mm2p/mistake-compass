pub use mc_core::{
    Category, Corpus, DirNode, FIELDS, FileNode, Level, Number, Question, QuestionType,
    build_level, build_tree,
};

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
