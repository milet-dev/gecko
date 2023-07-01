use crate::{
    diff::Diff,
    model::{self, User},
    time_utils, State,
};
use actix_identity::Identity;
use actix_web::{get, web, HttpResponse, Responder, Result};
use askama::Template;
use askama_actix::TemplateToResponse;
use git2::Oid;
use std::path::Path;
use time::{OffsetDateTime, UtcOffset};

const MAX_COMMIT_LEN: usize = 20;

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
    relative_time: String,
    datetime: String,
}

#[derive(Debug, Clone)]
struct Entry {
    name: String,
    kind: Kind,
}

#[derive(Template)]
#[template(path = "repository/index.html")]
struct RepositoryTemplate<'a> {
    title: &'a str,
    repository: &'a model::Repository,
    branch: &'a str,
    username: &'a str,
    name: &'a str,
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
            Ok(id) => state.database.find_user_from_id(&id).await,
            Err(_) => todo!(),
        },
        None => None,
    };

    let user = state.database.find_user(&username).await;

    let Some(repository) = state.database.find_repository(user.as_ref(), &name).await else {
        return Ok(HttpResponse::Ok().finish());
    };

    let Ok(repo) = git2::Repository::open(name.clone()) else {
        return Ok(HttpResponse::NotFound().finish());
    };

    let head = repo.head().unwrap();

    let mut branch = String::new();
    if head.is_branch() {
        let name = head
            .name()
            .map(|name| name.split('/').last().unwrap())
            .unwrap();
        branch.push_str(name);
    }
    let commit = head.peel_to_commit().unwrap();

    let message = commit.message().unwrap().to_string();
    let author_name = commit.author().name().unwrap().to_string();
    let author_email = commit.author().email().unwrap().to_string();
    let relative_time = time_utils::to_relative_time(commit.time().seconds());
    let datetime = time_utils::to_datetime(
        OffsetDateTime::from_unix_timestamp(commit.time().seconds()).unwrap(),
        Some(commit.time().offset_minutes()),
    );
    let commit_ = Commit {
        id: commit.id().to_string(),
        message,
        author: Author {
            name: author_name,
            email: author_email,
        },
        relative_time,
        datetime,
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

    let title = &name;

    Ok(RepositoryTemplate {
        title,
        repository: &repository,
        branch: &branch,
        username: &username,
        name: &name,
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
    title: &'a str,
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
    title: &'a str,
    repository: &'a model::Repository,
    username: &'a str,
    name: &'a str,
    branch: &'a str,
    breadcrumb: &'a str,
    user: &'a Option<User>,
    identity: &'a Option<User>,
    blob_name: &'a str,
    content: &'a [&'a str],
    size: &'a str,
}

pub async fn tree(
    path: web::Path<(String, String, String)>,
    state: web::Data<State>,
    identity: Option<Identity>,
) -> Result<impl Responder> {
    let (username, name, branch) = path.into_inner();

    let identity = match identity {
        Some(identity) => match identity.id() {
            Ok(id) => state.database.find_user_from_id(&id).await,
            Err(_) => todo!(),
        },
        None => None,
    };

    let user = state.database.find_user(&username).await;

    let Some(repository) = state.database.find_repository(user.as_ref(), &name).await else {
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
    let relative_time = time_utils::to_relative_time(commit.time().seconds());
    let datetime = time_utils::to_datetime(
        OffsetDateTime::from_unix_timestamp(commit.time().seconds()).unwrap(),
        Some(commit.time().offset_minutes()),
    );
    let commit_ = Commit {
        id: commit.id().to_string(),
        message,
        author: Author {
            name: author_name,
            email: author_email,
        },
        relative_time,
        datetime,
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

    let title = &format!("{name}/{branch}");

    Ok(TreeTemplate {
        title,
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

pub async fn tree_(
    path: web::Path<(String, String, String, String)>,
    query: web::Query<Query>,
    state: web::Data<State>,
    identity: Option<Identity>,
) -> Result<impl Responder> {
    let (username, name, branch, tail) = path.into_inner();

    let identity = match identity {
        Some(identity) => match identity.id() {
            Ok(id) => state.database.find_user_from_id(&id).await,
            Err(_) => todo!(),
        },
        None => None,
    };

    let user = state.database.find_user(&username).await;

    let Some(repository) = state.database.find_repository(user.as_ref(), &name).await else {
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
    let Ok(tree_entry) = commit.tree().unwrap().get_path(Path::new(&tail)) else {
        let file = {
            let rest = tail.split('/');
            rest.last().unwrap()
        };
        let body = format!("the path '{file}' does not exist in the given tree");
        return Ok(HttpResponse::NotFound().body(body));
    };
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

        let content: Vec<&str> = content.lines().collect();

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

        let title = &format!("{name}/{branch}/{tail}");

        return Ok(FileTemplate {
            title,
            repository: &repository,
            username: &username,
            name: &name,
            branch: &branch,
            breadcrumb: &breadcrumb,
            user: &user,
            identity: &identity,
            blob_name,
            content: content.as_slice(),
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
        relative_time: String::new(),
        datetime: String::new(),
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

    let title = &format!("{name}/{branch}");

    Ok(TreeTemplate {
        title,
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
#[template(path = "repository/branches.html")]
struct BranchesTemplate<'a> {
    title: &'a str,
    user: &'a Option<User>,
    identity: &'a Option<User>,
    username: &'a str,
    name: &'a str,
    branches: &'a [String],
}

pub async fn branches(
    path: web::Path<(String, String)>,
    state: web::Data<State>,
    identity: Option<Identity>,
) -> impl Responder {
    let (username, name) = path.into_inner();

    let identity = match identity {
        Some(identity) => match identity.id() {
            Ok(id) => state.database.find_user_from_id(&id).await,
            Err(_) => todo!(),
        },
        None => None,
    };

    let user = state.database.find_user(&username).await;

    let Ok(repo) = git2::Repository::open(name.clone()) else {
        todo!()
    };

    let branches = {
        let mut vec = Vec::new();
        let branches = repo.branches(Some(git2::BranchType::Local)).unwrap();
        for branch in branches {
            let (branch, _) = branch.unwrap();
            let name = branch.name().unwrap().unwrap();
            vec.push(name.to_owned());
        }
        vec
    };

    let title = format!("@{username}/{name}/branches");

    BranchesTemplate {
        title: &title,
        identity: &identity,
        user: &user,
        username: &username,
        name: &name,
        branches: &branches,
    }
    .to_response()
}

#[derive(Template)]
#[template(path = "commits.html")]
struct CommitsTemplate<'a> {
    title: &'a str,
    user: &'a Option<User>,
    identity: &'a Option<User>,
    username: &'a str,
    branch: &'a Option<&'a str>,
    name: &'a str,
    commits: &'a [Commit],
    parent_count: usize,
}

#[derive(serde::Deserialize)]
pub struct CommitsQuery {
    from: Option<String>,
}

pub async fn commits(
    path: web::Path<Vec<String>>,
    state: web::Data<State>,
    identity: Option<Identity>,
    query: web::Query<CommitsQuery>,
) -> Result<impl Responder> {
    let path = path.into_inner();
    let (username, name, branch) = if path.len() == 2 {
        (&path[0], &path[1], None)
    } else {
        (&path[0], &path[1], Some(path[2].as_str()))
    };

    let identity = match identity {
        Some(identity) => match identity.id() {
            Ok(id) => state.database.find_user_from_id(&id).await,
            Err(_) => todo!(),
        },
        None => None,
    };

    let user = state.database.find_user(username).await;

    let repo = git2::Repository::open(name).unwrap();

    let mut commits = Vec::new();

    match branch {
        Some(branch) => {
            let commit = match query.from.as_ref() {
                Some(from) => {
                    let oid = Oid::from_str(from).unwrap();
                    repo.find_commit(oid).unwrap()
                }
                None => {
                    if let Ok(branch) = repo.find_branch(branch, git2::BranchType::Local) {
                        branch.get().peel_to_commit().unwrap()
                    } else {
                        repo.find_commit(Oid::from_str(branch).unwrap()).unwrap()
                    }
                }
            };
            push_log(&commit, &mut commits, Some(MAX_COMMIT_LEN));
        }
        None => {
            if let Some(from) = query.from.as_ref() {
                let commit = repo.find_commit(Oid::from_str(from).unwrap()).unwrap();
                push_log(&commit, &mut commits, Some(MAX_COMMIT_LEN));
            } else {
                let mut revwalk = repo.revwalk().unwrap();
                revwalk.push_head().unwrap();
                for (_, commit) in revwalk.enumerate().take_while(|(i, _)| *i < MAX_COMMIT_LEN) {
                    let oid = commit.unwrap();
                    let commit = repo.find_commit(oid).unwrap();
                    let author = commit.author();
                    let relative_time = time_utils::to_relative_time(commit.time().seconds());
                    let datetime = time_utils::to_datetime(
                        OffsetDateTime::from_unix_timestamp(commit.time().seconds()).unwrap(),
                        Some(commit.time().offset_minutes()),
                    );
                    commits.push(Commit {
                        id: commit.id().to_string(),
                        message: commit.message().unwrap().to_string(),
                        author: Author {
                            name: author.name().unwrap_or_default().to_owned(),
                            email: author.email().unwrap_or_default().to_owned(),
                        },
                        relative_time,
                        datetime,
                    });
                }
            }
        }
    }

    let parent_count = {
        let last_id = commits.last().unwrap();
        let last_commit = repo
            .find_commit(Oid::from_str(&last_id.id).unwrap())
            .unwrap();
        last_commit.parent_count()
    };

    Ok(CommitsTemplate {
        title: "commits",
        name,
        username,
        user: &user,
        branch: &branch,
        identity: &identity,
        commits: &commits,
        parent_count,
    }
    .to_response())
}

pub struct DiffCommit {
    id: String,
    parent_ids: Vec<String>,
    author: Author,
    summary: String,
    relative_time: String,
    datetime: String,
}

#[derive(Template)]
#[template(path = "commit.html")]
pub struct CommitTemplate<'a> {
    username: &'a str,
    name: &'a str,
    commit: DiffCommit,
    diff: &'a Diff,
}

pub async fn diff(path: web::Path<(String, String, String)>) -> Result<impl Responder> {
    let (username, name, id) = path.into_inner();

    let repo = git2::Repository::open(&name).unwrap();
    let commit = repo.find_commit(Oid::from_str(&id).unwrap()).unwrap();
    let summary = commit.summary().unwrap_or_default();
    let time = commit.time();

    let parent_ids: Vec<_> = commit
        .parent_ids()
        .map(|parent_id| parent_id.to_string())
        .collect();

    let offset = UtcOffset::from_whole_seconds(time.offset_minutes() * 60).unwrap();
    let time = OffsetDateTime::from_unix_timestamp(time.seconds())
        .unwrap()
        .to_offset(offset);

    let diff = Diff::new(&repo, &id);

    let author = Author {
        name: commit
            .author()
            .name()
            .map(|inner| inner.to_string())
            .unwrap_or_default(),
        email: commit
            .author()
            .email()
            .map(|inner| inner.to_string())
            .unwrap_or_default(),
    };

    let unix_timestamp = time.unix_timestamp();
    let relative_time = time_utils::to_relative_time(unix_timestamp);
    let datetime = time_utils::to_datetime(time, None);

    Ok(CommitTemplate {
        username: &username,
        name: &name,
        commit: DiffCommit {
            id,
            parent_ids,
            author,
            summary: summary.to_string(),
            relative_time,
            datetime,
        },
        diff: &diff,
    }
    .to_response())
}

fn push_log(commit: &git2::Commit, log: &mut Vec<Commit>, limit: Option<usize>) {
    if let Some(limit) = limit {
        if log.len() == limit {
            return;
        }
    }
    log.push(Commit {
        id: commit.id().to_string(),
        message: commit.summary().unwrap().to_string(),
        author: Author {
            name: commit.author().name().unwrap().to_owned(),
            email: commit.author().email().unwrap().to_owned(),
        },
        relative_time: String::new(),
        datetime: String::new(),
    });
    let Ok(parent) = commit.parent(0) else {
        return;
    };
    push_log(&parent, log, limit);
}

#[get("/delete/{name}")]
pub async fn delete(
    path: web::Path<String>,
    state: web::Data<State>,
    identity: Option<Identity>,
) -> impl Responder {
    let name = path.into_inner();

    let identity = match identity {
        Some(identity) => match identity.id() {
            Ok(id) => state.database.find_user_from_id(&id).await,
            Err(_) => todo!(),
        },
        None => None,
    };

    let result = state.database.delete_repository(&identity, &name).await;
    match result {
        Ok(_) => "Ok",
        Err(e) => match e {
            crate::database::Error::Unauthorized => "Unauthorized",
            crate::database::Error::NotFound => "Not Found",
            _ => unreachable!(),
        },
    }
}
