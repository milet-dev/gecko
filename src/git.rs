use std::fmt::Display;

use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
pub enum BranchType {
    Local,
    Remote,
}

impl Display for BranchType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl BranchType {
    pub fn as_str(&self) -> &str {
        match self {
            BranchType::Local => "Local",
            BranchType::Remote => "Remote",
        }
    }
}

impl From<git2::BranchType> for BranchType {
    fn from(value: git2::BranchType) -> Self {
        match value {
            git2::BranchType::Local => BranchType::Local,
            git2::BranchType::Remote => BranchType::Remote,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub filemode: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Branch {
    pub name: String,
    pub ty: BranchType,
    pub entries: Vec<Entry>,
}

#[derive(Clone, Serialize)]
pub struct Repository {
    pub name: String,
    pub branches: Vec<Branch>,
}

pub fn open(name: &str) -> anyhow::Result<Repository> {
    let repo = git2::Repository::open(name)?;

    let mut repository = Repository {
        name: name.to_owned(),
        branches: Vec::new(),
    };

    let mut branches = repo.branches(None)?;
    while let Some(Ok((branch, branch_type))) = branches.next() {
        let Ok(Some(branch_name)) = branch.name() else {
            panic!()
        };

        let mut entries = vec![];
        let tree = repo.find_tree(branch.get().peel(git2::ObjectType::Tree)?.id())?;
        tree.walk(git2::TreeWalkMode::PostOrder, |_, entry| {
            entries.push(Entry {
                id: entry.id().to_string(),
                name: entry.name().unwrap().to_string(),
                filemode: entry.filemode().to_string(),
            });
            git2::TreeWalkResult::Ok
        })?;

        repository.branches.push(Branch {
            name: branch_name.to_owned(),
            ty: BranchType::from(branch_type),
            entries,
        });
    }

    Ok(repository)
}
