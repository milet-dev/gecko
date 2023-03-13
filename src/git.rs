use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
pub enum BranchType {
    Local,
    Remote,
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
pub struct Branch {
    pub name: String,
    pub ty: BranchType,
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
        repository.branches.push(Branch {
            name: branch_name.to_owned(),
            ty: BranchType::from(branch_type),
        });
    }

    Ok(repository)
}
