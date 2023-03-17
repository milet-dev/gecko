use crate::{model::User, State};
use actix_identity::Identity;
use actix_web::{get, web, Responder, Result};
use askama::Template;
use askama_actix::TemplateToResponse;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Dir,
    File,
}

impl Kind {
    fn as_str(&self) -> &str {
        match self {
            Kind::Dir => "Dir",
            Kind::File => "Kind",
        }
    }
}

#[derive(Debug, Clone)]
struct Author {
    name: String,
    email: String,
}

#[derive(Debug, Clone)]
struct Commit {
    id: String,
    message: String,
    author: Author,
}

#[derive(Debug, Clone)]
struct Entry {
    name: String,
    kind: Kind,
}

#[derive(Template)]
#[template(path = "repository.html")]
struct RepositoryTemplate<'a> {
    name: &'a str,
    branch: &'a str,
    user: &'a Option<User>,
    entries: &'a [Entry],
    commit: Commit,
}

#[get("/repository/{name}")]
pub async fn index(
    path: web::Path<String>,
    state: web::Data<State>,
    identity: Option<Identity>,
) -> Result<impl Responder> {
    let user = match identity {
        Some(identity) => {
            let users_collection = state.db.collection::<User>("users");
            let filter = bson::doc! { "username": identity.id().unwrap() };
            let Ok(Some(user)) = users_collection.find_one(filter, None).await else {
                unreachable!();
            };
            Some(user)
        }
        None => None,
    };

    let name = path.into_inner();
    let repo = git2::Repository::open(name.clone()).unwrap();
    let head = repo.head().unwrap();
    let commit = head.peel_to_commit().unwrap();

    let message = commit.message().unwrap().to_string();
    let author_name = commit.author().name().unwrap().to_string();
    let author_email = commit.author().email().unwrap().to_string();
    let commit_ = Commit {
        id: commit.id().to_string(),
        message,
        author: Author {
            name: author_name,
            email: author_email,
        },
    };
    let commit_tree = commit.tree().unwrap();
    let mut entries = vec![];
    for entry in commit_tree.iter() {
        let entry_name = entry.name().unwrap();

        let entry_kind = match entry.kind().unwrap() {
            git2::ObjectType::Tree => Kind::Dir,
            _ => Kind::File,
        };

        entries.push(Entry {
            name: entry_name.to_string(),
            kind: entry_kind,
        });
    }

    entries.sort_by_key(|k| k.kind == Kind::File);

    Ok(RepositoryTemplate {
        name: &name,
        branch: "main",
        user: &user,
        entries: &entries,
        commit: commit_,
    }
    .to_response())
}

#[derive(Template)]
#[template(path = "tree.html")]
struct TreeTemplate<'a> {
    user: &'a Option<User>,
    entries: &'a [Entry],
    commit: Commit,
    name: &'a str,
    branch: &'a str,
    tail: &'a str,
}

#[derive(Template)]
#[template(path = "file.html")]
struct FileTemplate<'a> {
    name: &'a str,
    branch: &'a str,
    tail: &'a str,
    user: &'a Option<User>,
    content: &'a str,
    size: String,
}

#[get("tree/{branch}/{tail}*")]
pub async fn tree(
    path: web::Path<(String, String, String)>,
    state: web::Data<State>,
    identity: Option<Identity>,
) -> Result<impl Responder> {
    let user = match identity {
        Some(identity) => {
            let users_collection = state.db.collection::<User>("users");
            let filter = bson::doc! { "username": identity.id().unwrap() };
            let Ok(Some(user)) = users_collection.find_one(filter, None).await else {
                unreachable!();
            };
            Some(user)
        }
        None => None,
    };
    let (name, branch, tail) = path.into_inner();

    let repo = git2::Repository::open(name.clone()).unwrap();
    let head = repo.head().unwrap();
    let commit = head.peel_to_commit().unwrap();
    let tree_entry = commit.tree().unwrap().get_path(Path::new(&tail)).unwrap();
    let object = tree_entry.to_object(&repo).unwrap();
    if let Some(blob) = object.as_blob() {
        let content = String::from_utf8_lossy(blob.content());
        let size = humansize::format_size(blob.size(), humansize::DECIMAL.decimal_places(0));

        return Ok(FileTemplate {
            name: &name,
            branch: &branch,
            tail: &tail,
            user: &user,
            content: &content,
            size,
        }
        .to_response());
    }

    let tree = object.into_tree().unwrap();

    let message = commit.message().unwrap().to_string();
    let author_name = commit.author().name().unwrap().to_string();
    let author_email = commit.author().email().unwrap().to_string();
    let commit_ = Commit {
        id: commit.id().to_string(),
        message,
        author: Author {
            name: author_name,
            email: author_email,
        },
    };

    let mut entries = vec![];
    for entry in tree.iter() {
        let entry_name = entry.name().unwrap();
        let entry_kind = match entry.kind().unwrap() {
            git2::ObjectType::Tree => Kind::Dir,
            _ => Kind::File,
        };

        entries.push(Entry {
            name: entry_name.to_string(),
            kind: entry_kind,
        });
    }

    entries.sort_by_key(|k| k.kind == Kind::File);

    Ok(TreeTemplate {
        user: &user,
        entries: &entries,
        commit: commit_,
        name: &name,
        branch: &branch,
        tail: &tail,
    }
    .to_response())
}

#[derive(Template)]
#[template(path = "commits.html")]
struct CommitsTemplate<'a> {
    user: &'a Option<User>,
    name: &'a str,
    commits: &'a [Commit],
}

#[get("/commits")]
pub async fn commits(
    path: web::Path<String>,
    state: web::Data<State>,
    identity: Option<Identity>,
) -> Result<impl Responder> {
    let user = match identity {
        Some(identity) => {
            let users_collection = state.db.collection::<User>("users");
            let filter = bson::doc! { "username": identity.id().unwrap() };
            let Ok(Some(user)) = users_collection.find_one(filter, None).await else {
                unreachable!();
            };
            Some(user)
        }
        None => None,
    };
    let name = path.into_inner();

    let repo = git2::Repository::open(name.clone()).unwrap();
    let mut revwalk = repo.revwalk().unwrap();
    revwalk.push_head().unwrap();
    let mut commits = Vec::new();
    for commit in revwalk {
        let oid = commit.unwrap();
        let commit = repo.find_commit(oid).unwrap();

        commits.push(Commit {
            id: commit.id().to_string(),
            message: commit.message().unwrap().to_string(),
            author: Author {
                name: commit.author().name().unwrap().to_owned(),
                email: commit.author().email().unwrap().to_owned(),
            },
        });
    }

    Ok(CommitsTemplate {
        name: &name,
        user: &user,
        commits: &commits,
    }
    .to_response())
}
