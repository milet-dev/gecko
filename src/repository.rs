use crate::{model::User, State};
use actix_identity::Identity;
use actix_web::{get, web, Responder, Result};
use askama::Template;
use askama_actix::TemplateToResponse;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Tree,
    File,
    Submodule,
}

impl Kind {
    fn as_str(&self) -> &str {
        match self {
            Kind::Tree => "Tree",
            Kind::File => "Kind",
            Kind::Submodule => "Submodule",
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
    readme: Option<(String, String)>,
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

    let mut readme: Option<(String, String)> = None;
    let mut entries = vec![];
    for entry in commit_tree.iter() {
        let entry_name = entry.name().unwrap();
        if entry.kind() == Some(git2::ObjectType::Blob) {
            if entry_name.starts_with("README") {
                let blob = repo.find_blob(entry.id()).unwrap();
                let content = String::from_utf8_lossy(blob.content());
                readme = Some((
                    entry_name.to_owned(),
                    markdown::to_html_with_options(&content, &markdown::Options::gfm()).unwrap(),
                ));
            }
        }

        let mut entry_kind = match entry.kind().unwrap() {
            git2::ObjectType::Tree => Kind::Tree,
            _ => Kind::File,
        };

        if repo.find_submodule(entry_name).is_ok() {
            entry_kind = Kind::Submodule;
        }

        entries.push(Entry {
            name: entry_name.to_string(),
            kind: entry_kind,
        });
    }

    entries.sort_by_key(|e| e.kind == Kind::File);

    Ok(RepositoryTemplate {
        name: &name,
        branch: "main",
        user: &user,
        entries: &entries,
        commit: commit_,
        readme,
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
    readme: Option<(String, String)>,
}

#[derive(Template)]
#[template(path = "file.html")]
struct FileTemplate<'a> {
    name: &'a str,
    branch: &'a str,
    tail: &'a str,
    user: &'a Option<User>,
    blob_name: &'a str,
    content: &'a str,
    size: &'a str,
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
        let blob_name = tail.split('/').last().unwrap();

        let content = String::from_utf8_lossy(blob.content());
        let size = humansize::format_size(blob.size(), humansize::DECIMAL.decimal_places(0));

        let content = if blob_name.ends_with(".md") || blob_name.ends_with(".markdown") {
            markdown::to_html_with_options(&content, &markdown::Options::gfm()).unwrap()
        } else {
            let mut buffer = String::new();
            buffer.push_str("<pre>");
            buffer.push_str(&content);
            buffer.push_str("</pre>");
            buffer
        };

        return Ok(FileTemplate {
            name: &name,
            branch: &branch,
            tail: &tail,
            user: &user,
            blob_name,
            content: &content,
            size: &size,
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
    let mut readme: Option<(String, String)> = None;
    let mut entries = vec![];
    for entry in tree.iter() {
        let entry_name = entry.name().unwrap();

        if entry.kind() == Some(git2::ObjectType::Blob) {
            let blob_path = Path::new(entry_name);

            if let Some(stem) = blob_path.file_stem() {
                let stem = stem.to_string_lossy();

                if let Some(ext) = blob_path.extension() {
                    if stem == "README" && (ext == "md" || ext == "markdown") {
                        let blob = repo.find_blob(entry.id()).unwrap();
                        let content = String::from_utf8_lossy(blob.content());
                        let output =
                            markdown::to_html_with_options(&content, &markdown::Options::gfm())
                                .unwrap();
                        readme = Some((stem.into_owned(), output));
                    }
                }
            }
        }

        let entry_kind = match entry.kind().unwrap() {
            git2::ObjectType::Tree => Kind::Tree,
            _ => Kind::File,
        };

        entries.push(Entry {
            name: entry_name.to_string(),
            kind: entry_kind,
        });
    }

    entries.sort_by_key(|e| e.kind == Kind::File);

    Ok(TreeTemplate {
        user: &user,
        entries: &entries,
        commit: commit_,
        name: &name,
        branch: &branch,
        tail: &tail,
        readme,
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
