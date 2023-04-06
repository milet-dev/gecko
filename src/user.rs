use crate::{
    model::{Repository, User},
    State,
};
use actix_identity::Identity;
use actix_web::{get, http::Method, web, HttpMessage, HttpRequest, HttpResponse, Responder};
use askama::Template;
use askama_actix::TemplateToResponse;
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
            let now = time::OffsetDateTime::now_utc();
            let unix_timestamp = now.unix_timestamp();

            let output: String = thread_rng()
                .sample_iter(&Alphanumeric)
                .take(40)
                .map(char::from)
                .collect();
            let salt = blake3::hash(format!("{output}{}", unix_timestamp).as_bytes()).to_string();
            let password =
                blake3::hash(format!("{}{}", params.password, salt).as_bytes()).to_string();
            let user = User {
                _id: bson::oid::ObjectId::default(),
                email: params.email.clone(),
                username: params.username.clone(),
                password,
                salt,
                created_at: unix_timestamp,
                updated_at: unix_timestamp,
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
            let params = params.unwrap();
            let username = params.username.clone();
            let password = params.password.clone();
            let Some(user) = state.database.login(&username, &password).await else {
                return HttpResponse::SeeOther()
                    .insert_header(("Location", "/login"))
                    .finish();
            };
            Identity::login(&req.extensions(), user.username).unwrap();
            HttpResponse::SeeOther()
                .insert_header(("Location", "/login"))
                .finish()
        }
        _ => unimplemented!(),
    }
}

#[get("/logout")]
pub async fn logout(identity: Option<Identity>) -> impl Responder {
    if let Some(identity) = identity {
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
            Ok(id) => state.database.find_user(&id).await,
            Err(_) => todo!(),
        },
        None => None,
    };

    let Some(user) = state.database.find_user(&username).await else {
        return Ok(HttpResponse::NotFound().finish());
    };

    let Some(repositories) = state.database.find_user_repositories(&username).await else {
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

    let username = identity.id().unwrap();

    if state.database.find_user(&username).await.is_none() {
        return HttpResponse::SeeOther()
            .insert_header(("Location", "/"))
            .finish();
    }

    match *req.method() {
        Method::GET => NewRepositoryTemplate { title: "" }.to_response(),
        Method::POST => {
            let form = form.unwrap();

            let username = identity.id().unwrap();

            let user = state.database.find_user(&username).await;

            let repository_name = form.name.clone();
            let description = if !form.description.is_empty() {
                Some(form.description.clone())
            } else {
                None
            };
            let result = state
                .database
                .new_repository(&user, &repository_name, description, &form.visibility)
                .await;
            if result.eq(&Err(crate::database::Error::Found)) {
                return HttpResponse::SeeOther()
                    .insert_header(("Location", "/new"))
                    .finish();
            }

            HttpResponse::SeeOther()
                .insert_header(("Location", format!("/@{username}/{}", form.name)))
                .finish()
        }
        _ => unimplemented!(),
    }
}
