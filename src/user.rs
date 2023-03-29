use crate::{
    model::{Repository, User},
    State,
};
use actix_identity::Identity;
use actix_web::{get, post, web, HttpMessage, HttpRequest, HttpResponse, Responder};
use askama::Template;
use askama_actix::TemplateToResponse;
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use serde::{Deserialize, Serialize};

#[derive(Template)]
#[template(path = "signup.html")]
struct SignupTemplate<'a> {
    title: &'a str,
}

#[get("/signup")]
async fn signup() -> impl Responder {
    SignupTemplate { title: "sign up" }.to_response()
}

#[derive(Serialize, Deserialize)]
pub struct SignupForm {
    email: String,
    username: String,
    password: String,
}

#[post("/signup")]
pub async fn signup_internal(
    state: web::Data<State>,
    identity: Option<Identity>,
    params: web::Form<SignupForm>,
) -> impl Responder {
    if identity.is_some() {
        return web::Redirect::to("/").see_other();
    }
    let collection = state.db.collection::<User>("users");
    let now = time::OffsetDateTime::now_utc();
    let unix_timestamp = now.unix_timestamp();

    let output: String = thread_rng()
        .sample_iter(&Alphanumeric)
        .take(40)
        .map(char::from)
        .collect();
    let salt = blake3::hash(format!("{output}{}", unix_timestamp).as_bytes()).to_string();
    let password = blake3::hash(format!("{}{}", params.password, salt).as_bytes()).to_string();
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
    web::Redirect::to("/login").see_other()
}

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate<'a> {
    title: &'a str,
}

#[get("/login")]
pub async fn login(identity: Option<Identity>) -> impl Responder {
    if identity.is_some() {
        return HttpResponse::SeeOther()
            .insert_header(("Location", "/"))
            .finish();
    }
    LoginTemplate { title: "login" }.to_response()
}

#[derive(Serialize, Deserialize)]
pub struct LoginForm {
    username: String,
    password: String,
}

#[post("/login")]
pub async fn login_internal(
    state: web::Data<State>,
    req: HttpRequest,
    params: web::Form<LoginForm>,
) -> impl Responder {
    let username = params.username.clone();
    let password = params.password.clone();

    let Some(user) = state.database.login(&username, &password).await else {
        return web::Redirect::to("/login").see_other();
    };

    Identity::login(&req.extensions(), user.username).unwrap();
    web::Redirect::to("/").see_other()
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

    Ok(IndexTemplate {
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

#[get("/new")]
async fn new(state: web::Data<State>, identity: Option<Identity>) -> impl Responder {
    let Some(identity) = identity else {
        return HttpResponse::Unauthorized().body("Unauthorized");
    };

    let username = identity.id().unwrap();

    if state.database.find_user(&username).await.is_none() {
        return HttpResponse::Unauthorized().body("Unauthorized");
    }

    NewRepositoryTemplate { title: "" }.to_response()
}

#[derive(Serialize, Deserialize)]
pub struct NewRepositoryForm {
    name: String,
    description: String,
    visibility: String,
}

#[post("/new")]
async fn new_internal(
    state: web::Data<State>,
    identity: Option<Identity>,
    form: web::Form<NewRepositoryForm>,
) -> impl Responder {
    let Some(identity) = identity else {
        return web::Redirect::to("/").see_other();
    };

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
        eprintln!("Found");
        return web::Redirect::to("/new").see_other();
    }

    web::Redirect::to(format!("/@{username}/{}", form.name)).see_other()
}
