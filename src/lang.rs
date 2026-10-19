//! Language registry backed by tree-sitter grammars.

use anyhow::{Result, anyhow};
use tree_sitter::{Language, Parser, Tree};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum Lang {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Tsx,
    Go,
    Java,
    CSharp,
    Ruby,
}

pub const ALL: [Lang; 9] = [
    Lang::Rust,
    Lang::Python,
    Lang::JavaScript,
    Lang::TypeScript,
    Lang::Tsx,
    Lang::Go,
    Lang::Java,
    Lang::CSharp,
    Lang::Ruby,
];

impl Lang {
    pub fn from_name(s: &str) -> Option<Lang> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "rust" | "rs" => Lang::Rust,
            "python" | "py" => Lang::Python,
            "javascript" | "js" | "jsx" => Lang::JavaScript,
            "typescript" | "ts" => Lang::TypeScript,
            "tsx" => Lang::Tsx,
            "go" | "golang" => Lang::Go,
            "java" => Lang::Java,
            "csharp" | "c#" | "cs" => Lang::CSharp,
            "ruby" | "rb" => Lang::Ruby,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Lang::Rust => "rust",
            Lang::Python => "python",
            Lang::JavaScript => "javascript",
            Lang::TypeScript => "typescript",
            Lang::Tsx => "tsx",
            Lang::Go => "go",
            Lang::Java => "java",
            Lang::CSharp => "csharp",
            Lang::Ruby => "ruby",
        }
    }

    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Lang::Rust => &["rs"],
            Lang::Python => &["py", "pyi"],
            Lang::JavaScript => &["js", "mjs", "cjs", "jsx"],
            Lang::TypeScript => &["ts", "mts", "cts"],
            Lang::Tsx => &["tsx"],
            Lang::Go => &["go"],
            Lang::Java => &["java"],
            Lang::CSharp => &["cs"],
            Lang::Ruby => &["rb", "rake", "gemspec", "ru"],
        }
    }

    pub fn from_path(path: &str) -> Option<Lang> {
        let file = path.rsplit('/').next()?;
        if matches!(
            file,
            "Gemfile" | "Rakefile" | "Guardfile" | "Vagrantfile" | "Podfile" | "Fastfile"
        ) {
            return Some(Lang::Ruby);
        }
        let (_, ext) = file.rsplit_once('.')?;
        let ext = ext.to_ascii_lowercase();
        ALL.into_iter().find(|l| l.extensions().contains(&ext.as_str()))
    }

    /// Separator used when qualifying nested symbol names.
    pub fn sep(self) -> &'static str {
        match self {
            Lang::Rust | Lang::Ruby => "::",
            _ => ".",
        }
    }

    pub fn ts_language(self) -> Language {
        match self {
            Lang::Rust => tree_sitter_rust::LANGUAGE.into(),
            Lang::Python => tree_sitter_python::LANGUAGE.into(),
            Lang::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Lang::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Lang::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Lang::Go => tree_sitter_go::LANGUAGE.into(),
            Lang::Java => tree_sitter_java::LANGUAGE.into(),
            Lang::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
            Lang::Ruby => tree_sitter_ruby::LANGUAGE.into(),
        }
    }
}

pub fn parse(lang: Lang, src: &[u8]) -> Result<Tree> {
    let mut p = Parser::new();
    p.set_language(&lang.ts_language())
        .map_err(|e| anyhow!("loading {} grammar: {e}", lang.name()))?;
    p.parse(src, None)
        .ok_or_else(|| anyhow!("tree-sitter failed to parse {} source", lang.name()))
}
