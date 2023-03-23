use crate::{
    model::{self, User},
    State,
};
use actix_identity::Identity;
use actix_web::{get, web, HttpResponse, Responder, Result};
use askama::Template;
use askama_actix::TemplateToResponse;
use git2::Oid;
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

#[derive(Debug, Clone, Default)]
struct Author {
    name: String,
    email: String,
}

#[derive(Debug, Clone, Default)]
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
    repository: &'a model::Repository,
    username: &'a str,
    name: &'a str,
    branch: &'a str,
    user: &'a Option<User>,
    identity: &'a Option<User>,
    entries: &'a [Entry],
    commit: Commit,
    readme: Option<(String, String)>,
}

#[get("/{name}")]
pub async fn index(
    path: web::Path<(String, String)>,
    state: web::Data<State>,
    identity: Option<Identity>,
) -> Result<impl Responder> {
    let (username, name) = path.into_inner();

    let identity = match identity {
        Some(identity) => match identity.id() {
            Ok(id) => state.database.find_user(&id).await,
            Err(_) => todo!(),
        },
        None => None,
    };

    let user = state.database.find_user(&username).await;

    let Some(repository) = state.database.find_repository(&user, &name).await else {
        return Ok(HttpResponse::Ok().finish());
    };

    let Ok(repo) = git2::Repository::open(name.clone()) else {
        return Ok(HttpResponse::NotFound().finish());
    };
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
        repository: &repository,
        username: &username,
        name: &name,
        branch: "main",
        user: &user,
        identity: &identity,
        entries: &entries,
        commit: commit_,
        readme,
    }
    .to_response())
}

#[derive(Template)]
#[template(path = "tree.html")]
struct TreeTemplate<'a> {
    repository: &'a model::Repository,
    username: &'a str,
    user: &'a Option<User>,
    identity: &'a Option<User>,
    entries: &'a [Entry],
    commit: Commit,
    name: &'a str,
    branch: &'a str,
    tail: &'a str,
    breadcrumb: &'a str,
    readme: Option<(String, String)>,
}

#[derive(Template)]
#[template(path = "file.html")]
struct FileTemplate<'a> {
    repository: &'a model::Repository,
    username: &'a str,
    name: &'a str,
    branch: &'a str,
    breadcrumb: &'a str,
    user: &'a Option<User>,
    identity: &'a Option<User>,
    blob_name: &'a str,
    content: &'a str,
    size: &'a str,
}

#[get("/{name}/tree/{branch}")]
pub async fn _tree(
    path: web::Path<(String, String, String)>,
    state: web::Data<State>,
    identity: Option<Identity>,
) -> Result<impl Responder> {
    let (username, name, branch) = path.into_inner();

    let identity = match identity {
        Some(identity) => match identity.id() {
            Ok(id) => state.database.find_user(&id).await,
            Err(_) => todo!(),
        },
        None => None,
    };

    let user = state.database.find_user(&username).await;

    let Some(repository) = state.database.find_repository(&user, &name).await else {
        return Ok(HttpResponse::Ok().finish());
    };

    let repo = git2::Repository::open(name.clone()).unwrap();
    let commit = {
        if let Ok(inner) = repo.find_branch(&branch, git2::BranchType::Local) {
            inner.get().peel_to_commit().unwrap()
        } else {
            repo.find_commit(Oid::from_str(&branch).unwrap()).unwrap()
        }
    };
    let commit_tree = commit.tree().unwrap();

    let message = commit.summary().unwrap().to_string();
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
    for entry in commit_tree.iter() {
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

    Ok(TreeTemplate {
        repository: &repository,
        username: &username,
        user: &user,
        identity: &identity,
        entries: &entries,
        commit: commit_,
        name: &name,
        branch: &branch,
        tail: "",
        breadcrumb: "",
        readme,
    }
    .to_response())
}

#[derive(serde::Deserialize)]
pub struct Query {
    raw: Option<bool>,
}

#[get("/{name}/tree/{branch}/{tail}*")]
pub async fn tree(
    path: web::Path<(String, String, String, String)>,
    query: web::Query<Query>,
    state: web::Data<State>,
    identity: Option<Identity>,
) -> Result<impl Responder> {
    let (username, name, branch, tail) = path.into_inner();

    let identity = match identity {
        Some(identity) => match identity.id() {
            Ok(id) => state.database.find_user(&id).await,
            Err(_) => todo!(),
        },
        None => None,
    };

    let user = state.database.find_user(&username).await;

    let Some(repository) = state.database.find_repository(&user, &name).await else {
        return Ok(HttpResponse::Ok().finish());
    };

    let repo = git2::Repository::open(name.clone()).unwrap();
    let commit = {
        if let Ok(inner) = repo.find_branch(&branch, git2::BranchType::Local) {
            inner.get().peel_to_commit().unwrap()
        } else {
            repo.find_commit(Oid::from_str(&branch).unwrap()).unwrap()
        }
    };
    let tree_entry = commit.tree().unwrap().get_path(Path::new(&tail)).unwrap();
    let object = tree_entry.to_object(&repo).unwrap();

    if let Some(blob) = object.as_blob() {
        let blob_name = tail.split('/').last().unwrap();
        let size = humansize::format_size(blob.size(), humansize::DECIMAL.decimal_places(0));

        let content = String::from_utf8_lossy(blob.content());

        if blob.is_binary() {
            if let Some(raw) = query.raw {
                if raw {
                    return Ok(HttpResponse::Ok()
                        .content_type("application/octet-stream")
                        .insert_header((
                            "Content-Disposition",
                            format!("attachment; filename=\"{blob_name}\""),
                        ))
                        .body(content.into_owned()));
                }
            }
            return Ok(HttpResponse::Ok()
                .content_type("text/html")
                .body(format!("{blob_name} {size}\n<a href=\"/@{username}/{name}/tree/{branch}/{tail}/?raw=true\">view raw</a>")));
        }

        let content = if blob_name.ends_with(".md") || blob_name.ends_with(".markdown") {
            markdown::to_html_with_options(&content, &markdown::Options::gfm()).unwrap()
        } else {
            let mut buffer = String::new();
            buffer.push_str("<pre>");
            for (i, line) in content.lines().enumerate() {
                let url = format!("<a class=\"line\" id=\"L{i}\" href=\"/@{username}/{name}/tree/{branch}/{tail}#L{i}\">{i}</a>\t{line}\n");
                buffer.push_str(&url);
            }
            buffer.push_str("</pre>");
            buffer
        };

        let mut breadcrumb = String::new();
        let mut buffer = String::new();
        let segments = tail.split('/');
        breadcrumb.push_str(&format!("<a href=\"/@{username}\">@{username}</a>/"));
        breadcrumb.push_str(&format!(
            "<a href=\"/@{username}/{name}/tree/{branch}\">{name}</a>/"
        ));
        for segment in segments {
            buffer.push_str(segment);
            buffer.push('/');
            let url = if segment == blob_name {
                segment.to_owned()
            } else {
                format!("<a href=\"/@{username}/{name}/tree/{branch}/{buffer}\">{segment}</a>/")
            };
            breadcrumb.push_str(&url);
        }

        return Ok(FileTemplate {
            repository: &repository,
            username: &username,
            name: &name,
            branch: &branch,
            breadcrumb: &breadcrumb,
            user: &user,
            identity: &identity,
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

    let mut breadcrumb = String::new();
    let mut buffer = String::new();
    let segments = tail.split('/');
    let last = segments.clone().last().unwrap().to_string();
    breadcrumb.push_str(&format!("<a href=\"/@{username}\">@{username}</a>/"));
    breadcrumb.push_str(&format!(
        "<a href=\"/@{username}/{name}/tree/{branch}\">{name}</a>/"
    ));
    for segment in segments {
        buffer.push_str(segment);
        buffer.push('/');
        let url = if segment == last {
            segment.to_owned()
        } else {
            format!("<a href=\"/@{username}/{name}/tree/{branch}/{buffer}\">{segment}</a>/")
        };
        breadcrumb.push_str(&url);
    }

    Ok(TreeTemplate {
        repository: &repository,
        username: &username,
        user: &user,
        identity: &identity,
        entries: &entries,
        commit: commit_,
        name: &name,
        branch: &branch,
        tail: &tail,
        breadcrumb: breadcrumb.trim_end_matches('/'),
        readme,
    }
    .to_response())
}

#[derive(Template)]
#[template(path = "commits.html")]
struct CommitsTemplate<'a> {
    user: &'a Option<User>,
    identity: &'a Option<User>,
    username: &'a str,
    name: &'a str,
    commits: &'a [Commit],
}

#[get("/{name}/commits")]
pub async fn commits(
    path: web::Path<(String, String)>,
    state: web::Data<State>,
    identity: Option<Identity>,
) -> Result<impl Responder> {
    let (username, name) = path.into_inner();

    let identity = match identity {
        Some(identity) => match identity.id() {
            Ok(id) => state.database.find_user(&id).await,
            Err(_) => todo!(),
        },
        None => None,
    };

    let user = state.database.find_user(&username).await;

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
        username: &username,
        user: &user,
        identity: &identity,
        commits: &commits,
    }
    .to_response())
}

#[get("/{name}/commits/{branch}")]
pub async fn _commits(
    path: web::Path<(String, String, String)>,
    state: web::Data<State>,
    identity: Option<Identity>,
) -> Result<impl Responder> {
    let _identity = match identity {
        Some(identity) => {
            let Ok(id) = identity.id() else {
                todo!()
            };
            state.database.find_user(&id).await
        }
        None => return Ok(HttpResponse::Ok().finish()),
    };

    let (username, name, branch) = path.into_inner();
    let user = state.database.find_user(&username).await;

    let repo = git2::Repository::open(name.clone()).unwrap();

    let mut log = vec![];

    let commit = {
        if let Ok(inner) = repo.find_branch(&branch, git2::BranchType::Local) {
            inner.get().peel_to_commit().unwrap()
        } else {
            repo.find_commit(Oid::from_str(&branch).unwrap()).unwrap()
        }
    };

    push_log(&commit, &mut log);

    Ok(CommitsTemplate {
        name: &name,
        username: &username,
        user: &user,
        identity: &_identity,
        commits: &log,
    }
    .to_response())
}

fn push_log(commit: &git2::Commit, log: &mut Vec<Commit>) {
    log.push(Commit {
        id: commit.id().to_string(),
        message: commit.summary().unwrap().to_string(),
        author: Author {
            name: commit.author().name().unwrap().to_owned(),
            email: commit.author().email().unwrap().to_owned(),
        },
    });
    let Ok(parent) = commit.parent(0) else {
        return;
    };
    push_log(&parent, log);
}
