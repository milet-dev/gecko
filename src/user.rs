use std::str::FromStr;

use crate::{
    model::{Event, Log, Repository, User},
    State,
};
use actix_identity::Identity;
use actix_web::{get, http::Method, web, HttpMessage, HttpRequest, HttpResponse, Responder};
use askama::Template;
use askama_actix::TemplateToResponse;
use bson::oid::ObjectId;
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use serde::{Deserialize, Serialize};

#[derive(Template)]
#[template(path = "signup.html")]
struct SignupTemplate<'a> {
    title: &'a str,
}

#[derive(Serialize, Deserialize)]
pub struct SignupForm {
    email: String,
    username: String,
    password: String,
}

pub async fn signup(
    req: HttpRequest,
    state: web::Data<State>,
    identity: Option<Identity>,
    params: Option<web::Form<SignupForm>>,
) -> impl Responder {
    if identity.is_some() {
        return HttpResponse::SeeOther()
            .insert_header(("Location", "/"))
            .finish();
    }
    match *req.method() {
        Method::GET => SignupTemplate { title: "sign up" }.to_response(),
        Method::POST => {
            let params = params.unwrap();
            let collection = state.db.collection::<User>("users");

            let (password, salt) = create_password(&params.password);
            let user = User {
                _id: bson::oid::ObjectId::default(),
                email: params.email.clone(),
                username: params.username.clone(),
                password,
                salt,
                created_at: unix_timestamp(),
                updated_at: unix_timestamp(),
                log: Vec::new(),
            };
            if collection.insert_one(&user, None).await.is_err() {
                todo!();
            }
            HttpResponse::SeeOther()
                .insert_header(("Location", "/login"))
                .finish()
        }
        _ => unimplemented!(),
    }
}

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate<'a> {
    title: &'a str,
}

#[derive(Serialize, Deserialize)]
pub struct LoginForm {
    username: String,
    password: String,
}

pub async fn login(
    req: HttpRequest,
    state: web::Data<State>,
    identity: Option<Identity>,
    params: Option<web::Form<LoginForm>>,
) -> impl Responder {
    if identity.is_some() {
        return HttpResponse::SeeOther()
            .insert_header(("Location", "/"))
            .finish();
    }
    match *req.method() {
        Method::GET => LoginTemplate { title: "login" }.to_response(),
        Method::POST => {
            let Some(params) = params else {
                return HttpResponse::SeeOther()
                    .insert_header(("Location", "/login"))
                    .finish()
            };
            let username = params.username.clone();
            let password = params.password.clone();
            let Some(user) = state.database.login(&username, &password).await else {
                return HttpResponse::SeeOther()
                    .insert_header(("Location", "/login"))
                    .finish();
            };
            if Identity::login(&req.extensions(), user._id.to_string()).is_err() {
                return HttpResponse::SeeOther()
                    .insert_header(("Location", "/login"))
                    .finish();
            }
            state
                .database
                .add_user_log(&user, Event::Login, Some(user.username.clone()))
                .await;
            HttpResponse::SeeOther()
                .insert_header(("Location", "/login"))
                .finish()
        }
        _ => unimplemented!(),
    }
}

#[get("/logout")]
pub async fn logout(identity: Option<Identity>, state: web::Data<State>) -> impl Responder {
    if let Some(identity) = identity {
        let id = identity.id().unwrap();
        let user = state.database.find_user_from_id(&id).await.unwrap();
        state
            .database
            .add_user_log(&user, Event::Logout, Some(user.username.clone()))
            .await;
        identity.logout();
    }
    web::Redirect::to("/").see_other()
}

#[derive(Template)]
#[template(path = "user/index.html")]
struct IndexTemplate<'a> {
    title: &'a str,
    user: &'a User,
    identity: &'a Option<User>,
    repositories: &'a [Repository],
}

#[get("/@{username}")]
async fn index(
    path: web::Path<String>,
    state: web::Data<State>,
    identity: Option<Identity>,
) -> actix_web::Result<impl Responder> {
    let username = path.into_inner();

    let identity = match identity {
        Some(identity) => match identity.id() {
            Ok(id) => state.database.find_user_from_id(&id).await,
            Err(_) => todo!(),
        },
        None => None,
    };

    let Some(user) = state.database.find_user(&username).await else {
        return Ok(HttpResponse::NotFound().finish());
    };

    let Some(repositories) = state.database.find_user_repositories(user._id).await else {
        return Ok(HttpResponse::NotFound().finish());
    };

    let repositories: Vec<_> = match identity.as_ref() {
        Some(identity) if identity._id == user._id => repositories.into_iter().collect(),
        _ => repositories
            .into_iter()
            .filter(|inner| inner.visibility == "public")
            .collect(),
    };

    let title = &username;

    Ok(IndexTemplate {
        title,
        user: &user,
        identity: &identity,
        repositories: &repositories,
    }
    .to_response())
}

#[derive(Template)]
#[template(path = "new.html")]
struct NewRepositoryTemplate<'a> {
    title: &'a str,
}

#[derive(Serialize, Deserialize)]
pub struct NewRepositoryForm {
    name: String,
    description: String,
    visibility: String,
}

pub async fn new(
    req: HttpRequest,
    state: web::Data<State>,
    identity: Option<Identity>,
    form: Option<web::Form<NewRepositoryForm>>,
) -> impl Responder {
    let Some(identity) = identity else {
        return HttpResponse::SeeOther()
            .insert_header(("Location", "/"))
            .finish();
    };

    let id = identity.id().unwrap();

    if state.database.find_user_from_id(&id).await.is_none() {
        return HttpResponse::SeeOther()
            .insert_header(("Location", "/"))
            .finish();
    }

    match *req.method() {
        Method::GET => NewRepositoryTemplate { title: "" }.to_response(),
        Method::POST => {
            let form = form.unwrap();

            let id = identity.id().unwrap();
            let user = state.database.find_user_from_id(&id).await.unwrap();
            let username = &user.username;

            let repository_name = form.name.clone();
            let description = if !form.description.is_empty() {
                Some(form.description.clone())
            } else {
                None
            };
            let result = state
                .database
                .new_repository(Some(&user), &repository_name, description, &form.visibility)
                .await;
            if result.eq(&Err(crate::database::Error::Found)) {
                return HttpResponse::SeeOther()
                    .insert_header(("Location", "/new"))
                    .finish();
            }

            if result.is_ok() {
                state
                    .database
                    .add_user_log(&user, Event::RepositoryCreate, Some(repository_name))
                    .await;
            }

            HttpResponse::SeeOther()
                .insert_header(("Location", format!("/@{username}/{}", form.name)))
                .finish()
        }
        _ => HttpResponse::NotFound().finish(),
    }
}

#[derive(Template)]
#[template(path = "settings.html")]
struct SettingsTemplate<'a> {
    title: &'a str,
    user: &'a User,
    identity: Option<User>,
}

pub async fn settings(state: web::Data<State>, identity: Option<Identity>) -> impl Responder {
    let identity = match identity.as_ref() {
        Some(identity) => match identity.id() {
            Ok(id) => state.database.find_user_from_id(&id).await,
            Err(_) => unimplemented!(),
        },
        None => None,
    };

    let Some(user) = identity.clone() else {
        return HttpResponse::SeeOther()
            .insert_header(("Location", "/login"))
            .finish();
    };

    SettingsTemplate {
        title: "settings",
        user: &user,
        identity,
    }
    .to_response()
}

#[derive(Template)]
#[template(path = "user/log.html")]
struct LogTemplate<'a> {
    title: &'a str,
    identity: Option<User>,
    log: &'a [Log],
}

pub async fn log(state: web::Data<State>, identity: Option<Identity>) -> impl Responder {
    let identity = match identity {
        Some(identity) => match identity.id() {
            Ok(id) => state.database.find_user_from_id(&id).await,
            Err(_) => unimplemented!(),
        },
        None => {
            return HttpResponse::TemporaryRedirect()
                .insert_header(("Location", "/login"))
                .finish()
        }
    };

    let user = identity.clone().unwrap();

    let mut log = user.log;
    log.reverse();

    LogTemplate {
        title: "log",
        identity,
        log: log.as_slice(),
    }
    .to_response()
}

#[derive(Template)]
#[template(path = "password.html")]
struct PasswordTemplate<'a> {
    title: &'a str,
    identity: Option<User>,
}

pub async fn password(state: web::Data<State>, identity: Option<Identity>) -> impl Responder {
    let identity = match identity {
        Some(identity) => match identity.id() {
            Ok(id) => state.database.find_user_from_id(&id).await,
            Err(_) => unimplemented!(),
        },
        None => {
            return HttpResponse::TemporaryRedirect()
                .insert_header(("Location", "/login"))
                .finish()
        }
    };

    PasswordTemplate {
        title: "update password",
        identity,
    }
    .to_response()
}

#[derive(Serialize, Deserialize)]
pub struct UpdateForm {
    email: String,
    username: String,
}

pub async fn update(
    state: web::Data<State>,
    identity: Option<Identity>,
    form: web::Form<UpdateForm>,
) -> impl Responder {
    let Some(identity) = identity else {
        return HttpResponse::SeeOther()
            .insert_header(("Location", "/login"))
            .finish();
    };

    let id = identity.id().unwrap();
    let form = form.into_inner();
    let username = form.username;

    let users = state.db.collection::<User>("users");
    let result = users
        .update_one(
            bson::doc! { "_id": ObjectId::from_str(&id).unwrap() },
            bson::doc! {
                "$set": {
                    "email": &form.email,
                    "username": &username,
                    "updated_at": unix_timestamp(),
                },
            },
            None,
        )
        .await;
    match result {
        Ok(update_result) if update_result.modified_count != 0 => HttpResponse::SeeOther()
            .insert_header(("Location", format!("/@{username}")))
            .finish(),
        _ => {
            identity.logout();
            HttpResponse::SeeOther()
                .insert_header(("Location", "/login"))
                .finish()
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct UpdatePasswordForm {
    password0: String,
    password1: String,
    password2: String,
}

pub async fn update_password(
    state: web::Data<State>,
    identity: Option<Identity>,
    form: web::Form<UpdatePasswordForm>,
) -> impl Responder {
    let Some(identity) = identity else {
        return HttpResponse::SeeOther()
            .insert_header(("Location", "/login"))
            .finish();
    };

    let user = match identity.id() {
        Ok(id) => state.database.find_user_from_id(&id).await.unwrap(),
        Err(_) => {
            return HttpResponse::SeeOther()
                .insert_header(("Location", "/login"))
                .finish()
        }
    };
    let form = form.into_inner();

    let password0 = create_password_using_salt(&form.password0, &user.salt);
    if password0 != user.password {
        return HttpResponse::SeeOther()
            .insert_header(("Location", "/settings/password"))
            .finish();
    }
    let (password1, salt) = create_password(&form.password1);
    let password2 = create_password_using_salt(&form.password2, &salt);
    if password1 != password2 {
        return HttpResponse::SeeOther()
            .insert_header(("Location", "/settings/password"))
            .finish();
    }

    let users = state.db.collection::<User>("users");
    let result = users
        .update_one(
            bson::doc! { "_id": user._id },
            bson::doc! {
                "$set": {
                    "password": password1,
                    "salt": salt,
                    "updated_at": unix_timestamp(),
                },
            },
            None,
        )
        .await;
    match result {
        Ok(update_result) if update_result.modified_count != 0 => {
            state
                .database
                .add_user_log(&user, Event::UpdatePassword, None)
                .await;
            HttpResponse::SeeOther()
                .insert_header(("Location", "/settings/password"))
                .finish()
        }
        _ => HttpResponse::SeeOther()
            .insert_header(("Location", "/login"))
            .finish(),
    }
}

pub fn unix_timestamp() -> i64 {
    let now = time::OffsetDateTime::now_utc();
    now.unix_timestamp()
}

fn create_password_using_salt(password: &str, salt: &str) -> String {
    blake3::hash(format!("{}{}", password, salt).as_bytes()).to_string()
}

fn create_password(password: &str) -> (String, String) {
    let random_values: String = thread_rng()
        .sample_iter(&Alphanumeric)
        .take(40)
        .map(char::from)
        .collect();
    let salt = blake3::hash(format!("{random_values}{}", unix_timestamp()).as_bytes()).to_string();
    let password = blake3::hash(format!("{password}{salt}").as_bytes()).to_string();
    (password, salt)
}
