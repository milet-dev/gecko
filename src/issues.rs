use actix_identity::Identity;
use actix_web::{http::Method, web, HttpRequest, HttpResponse, Responder};
use askama::Template;
use askama_actix::TemplateToResponse;
use bson::oid::ObjectId;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::{
    model::{Issue, Repository, User},
    time_utils, State,
};

#[derive(Template)]
#[template(path = "repository/issues/index.html")]
struct Issues<'a> {
    title: &'a str,
    identity: &'a Option<User>,
    username: &'a str,
    name: &'a str,
    issues: &'a [Issue],
}
pub async fn index(
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
    let repo = state
        .database
        .find_repository(user.as_ref(), &name)
        .await
        .unwrap();
    Issues {
        title: "issues",
        identity: &identity,
        username: &username,
        name: &name,
        issues: &repo.issues,
    }
    .to_response()
}

#[derive(Template)]
#[template(path = "repository/issues/issue.html")]
struct IssueTemplate<'a> {
    title: &'a str,
    username: &'a str,
    name: &'a str,
    identity: &'a Option<User>,
    issue: &'a Issue,
    user: &'a User,
    comments: &'a [Comment],
}

struct Comment {
    index: i64,
    username: String,
    body: String,
    relative_time: String,
    datetime: String,
}

pub async fn view(
    path: web::Path<(String, String, i64)>,
    state: web::Data<State>,
    identity: Option<Identity>,
) -> impl Responder {
    let (username, name, index) = path.into_inner();

    let identity = match identity {
        Some(identity) => match identity.id() {
            Ok(id) => state.database.find_user_from_id(&id).await,
            Err(_) => todo!(),
        },
        None => None,
    };
    let user = state.database.find_user(&username).await;
    let mut repo = state
        .database
        .find_repository(user.as_ref(), &name)
        .await
        .unwrap();
    let Some(mut issue) = repo.issues.iter_mut().find(|issue| issue.index == index) else {
        todo!()
    };
    let mut comments = Vec::new();
    for comment in &issue.comments {
        let user = state
            .database
            .find_user_from_id(&comment.user_id.to_string())
            .await
            .unwrap_or_default();
        let body = markdown::to_html_with_options(&comment.body, &markdown::Options::gfm())
            .unwrap()
            .to_string();
        let created_at = comment.created_at.unwrap_or(0);
        let relative_time = time_utils::to_relative_time(created_at);
        let datetime = time_utils::to_datetime(
            {
                let offset_dt = OffsetDateTime::from_unix_timestamp(created_at).unwrap();
                offset_dt
            },
            None,
        );
        comments.push(Comment {
            index: comment.index,
            username: user.username,
            body,
            relative_time,
            datetime,
        })
    }

    issue.body =
        markdown::to_html_with_options(&issue.body, &markdown::Options::gfm()).unwrap_or_default();

    let user = state
        .database
        .find_user_from_id(&issue.user_id.to_string())
        .await
        .unwrap();

    let title = &format!("{} - issue #{}", &issue.title, index);

    IssueTemplate {
        title,
        username: &username,
        name: &name,
        identity: &identity,
        issue,
        user: &user,
        comments: &comments,
    }
    .to_response()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CommentForm {
    body: String,
}

pub async fn add_comment(
    path: web::Path<(String, String, i64)>,
    form: web::Form<CommentForm>,
    state: web::Data<State>,
    identity: Option<Identity>,
) -> impl Responder {
    let (username, name, issue_id) = path.into_inner();

    let identity = match identity {
        Some(identity) => match identity.id() {
            Ok(id) => state.database.find_user_from_id(&id).await,
            Err(_) => todo!(),
        },
        None => {
            return HttpResponse::SeeOther()
                .insert_header(("Location", format!("/@{username}/{name}/issues/{issue_id}")))
                .finish()
        }
    };

    if form.body.is_empty() {
        return HttpResponse::SeeOther()
            .insert_header(("Location", format!("/@{username}/{name}/issues/{issue_id}")))
            .finish();
    }

    let user = state.database.find_user(&username).await;

    let repo = state
        .database
        .find_repository(user.as_ref(), &name)
        .await
        .unwrap();
    let Some(issue) = repo.issues.iter().find(|issue| issue.index == issue_id) else {
        todo!()
    };
    let repositories = state.db.collection::<Repository>("repositories");

    let now = time::OffsetDateTime::now_utc();
    let unix_timestamp = now.unix_timestamp();
    let last_index = issue
        .comments
        .last()
        .map(|comment| comment.index)
        .unwrap_or(0);
    let index = last_index + 1;
    let result = repositories
        .update_one(
            bson::doc! { "_id": repo._id, "issues.index": issue_id },
            bson::doc! {
                "$push": {
                    "issues.$.comments": {
                        "_id": ObjectId::new(),
                        "index": index,
                        "user_id": identity.unwrap()._id,
                        "body": &form.body,
                        "created_at": unix_timestamp,
                    }
                }
            },
            None,
        )
        .await;
    if result.is_ok() {
        return HttpResponse::SeeOther()
            .insert_header((
                "Location",
                format!("/@{username}/{name}/issues/{issue_id}#comment-{index}"),
            ))
            .finish();
    }
    HttpResponse::Ok().finish()
}

#[derive(Template)]
#[template(path = "repository/issues/new.html")]
struct NewIssue<'a> {
    username: &'a str,
    name: &'a str,
    identity: &'a Option<User>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct IssueForm {
    title: String,
    body: String,
}

pub async fn new(
    req: HttpRequest,
    state: web::Data<State>,
    identity: Option<Identity>,
    path: web::Path<(String, String)>,
    form: Option<web::Form<IssueForm>>,
) -> impl Responder {
    let (username, name) = path.into_inner();

    let identity = match identity {
        Some(identity) => match identity.id() {
            Ok(id) => state.database.find_user_from_id(&id).await,
            Err(_) => todo!(),
        },
        None => None,
    };

    let repo = {
        let user = state.database.find_user(&username).await;
        state
            .database
            .find_repository(user.as_ref(), &name)
            .await
            .unwrap()
    };

    match *req.method() {
        Method::GET => NewIssue {
            username: &username,
            name: &name,
            identity: &identity,
        }
        .to_response(),
        Method::POST => {
            let Some(user) = identity.as_ref() else {
                return HttpResponse::SeeOther()
                    .insert_header(("Location", "/login"))
                    .finish();
            };

            let form = form.unwrap();

            let repositories = state.db.collection::<Repository>("repositories");
            let now = time::OffsetDateTime::now_utc();
            let unix_timestamp = now.unix_timestamp();

            let col = state.db.collection::<Repository>("repositories");
            let last = col
                .aggregate(
                    [
                        bson::doc! { "$match": { "_id": repo._id } },
                        bson::doc! { "$addFields": { "last_index": { "$last": "$issues.index" } } },
                    ],
                    None,
                )
                .await
                .unwrap();
            let cursor = last.current();
            let last_index = cursor.get_i32("last_index").unwrap_or(0);

            let result = repositories
                .update_one(
                    bson::doc! { "_id": repo._id },
                    bson::doc! {
                        "$push": {
                            "issues": {
                                "user_id": user._id,
                                "index": last_index + 1,
                                "title": form.title.clone(),
                                "body": form.body.clone(),
                                "comments": [],
                                "visibility": true,
                                "created_at": unix_timestamp,
                                "updated_at": unix_timestamp,
                                "status": 0,
                            }
                        }
                    },
                    None,
                )
                .await;
            if result.is_err() {
                todo!();
            }
            HttpResponse::SeeOther()
                .insert_header(("Location", format!("/@{username}/{name}/issues")))
                .finish()
        }
        _ => unimplemented!(),
    }
}
