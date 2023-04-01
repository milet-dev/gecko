use std::collections::HashMap;

use git2::{DiffFormat, DiffLineType, DiffOptions, Oid};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Line {
    pub old_lineno: i32,
    pub new_lineno: i32,
    pub content: String,
    pub origin: u8,
}

impl Line {
    fn new(old_lineno: i32, new_lineno: i32, content: String, origin: u8) -> Self {
        Self {
            old_lineno,
            new_lineno,
            content,
            origin,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Stats {
    pub insertions: usize,
    pub deletions: usize,
}

#[derive(Debug, Clone)]
pub struct File {
    pub name: String,
    pub stats: Stats,
    pub hash: String,
    pub data: Vec<Line>,
}

impl From<(String, (Stats, String, Vec<Line>))> for File {
    fn from(value: (String, (Stats, String, Vec<Line>))) -> Self {
        let (name, (stats, hash, data)) = value;
        File {
            name,
            stats,
            hash,
            data,
        }
    }
}

#[derive(Debug)]
pub struct Diff {
    pub files: Vec<File>,
    pub stats: git2::DiffStats,
}

impl Diff {
    pub fn new(repo: &git2::Repository, id: &str) -> Diff {
        let commit = repo.find_commit(Oid::from_str(id).unwrap()).unwrap();
        let commit_tree = commit.tree().unwrap();

        let parent_tree = commit.parents().next().map(|inner| inner.tree().unwrap());
        let mut opts = DiffOptions::new();
        let diff = repo
            .diff_tree_to_tree(parent_tree.as_ref(), Some(&commit_tree), Some(&mut opts))
            .unwrap();

        let stats = diff.stats().unwrap();

        let mut map: HashMap<String, (Stats, String, Vec<Line>)> = HashMap::new();
        _ = diff.print(DiffFormat::Patch, |delta, _, line| {
            let path = delta
                .new_file()
                .path()
                .map(|inner| inner.to_string_lossy().into_owned())
                .unwrap();
            map.entry(path.clone())
                .and_modify(|(stats, hash, lines)| {
                    let mut hasher = Sha256::new();
                    hasher.update(path.as_bytes());
                    *hash = format!("{:x}", hasher.finalize());
                    let content = String::from_utf8_lossy(line.content());
                    let content = content.trim_matches('\n').to_string();
                    let old_lineno = line.old_lineno().map(|n| n as i32).unwrap_or(-1);
                    let new_lineno = line.new_lineno().map(|n| n as i32).unwrap_or(-1);
                    let origin = match line.origin() {
                        ' ' => 0,
                        '+' => 1,
                        '-' => 2,
                        '=' => 3,
                        '>' => 4,
                        '<' => 5,
                        'F' => 6,
                        'H' => 7,
                        'B' => 8,
                        _ => unreachable!(),
                    };
                    if content.starts_with("@@") {
                        lines.push(Line::new(
                            old_lineno,
                            new_lineno,
                            format!("  {content}"),
                            origin,
                        ));
                    } else {
                        if line.origin_value() == DiffLineType::Addition {
                            stats.insertions += 1;
                        } else if line.origin_value() == DiffLineType::Deletion {
                            stats.deletions += 1;
                        }
                        lines.push(Line::new(old_lineno, new_lineno, content, origin));
                    }
                })
                .or_insert((Stats::default(), String::new(), Vec::new()));
            true
        });

        let mut files: Vec<_> = map.into_iter().map(File::from).collect();
        files.sort_unstable_by_key(|inner| inner.name.clone());

        Self { files, stats }
    }
}
