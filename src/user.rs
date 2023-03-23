use crate::{
    model::{Repository, User},
    State,
};
use actix_identity::Identity;
use actix_web::{
    get, http::StatusCode, post, web, HttpMessage, HttpRequest, HttpResponse, Responder,
};
use askama::Template;
use askama_actix::TemplateToResponse;
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
        return web::Redirect::to("/");
    }
    let collection = state.db.collection::<User>("users");
    let user = User {
        _id: bson::oid::ObjectId::default(),
        email: params.email.clone(),
        username: params.username.clone(),
        password: params.password.clone(),
    };
    if collection.insert_one(&user, None).await.is_err() {
        todo!();
    }
    web::Redirect::to("/login")
}

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate<'a> {
    title: &'a str,
}

#[get("/login")]
pub async fn login(identity: Option<Identity>) -> impl Responder {
    if identity.is_some() {
        return HttpResponse::Unauthorized().body("Unauthorized");
    }
    LoginTemplate { title: "login" }.to_response()
}

#[get("/logout")]
pub async fn logout(identity: Option<Identity>) -> impl Responder {
    match identity {
        Some(identity) => {
            identity.logout();
            HttpResponse::Ok().finish()
        }
        None => HttpResponse::Unauthorized().body("Unauthorized"),
    }
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
        return web::Redirect::to("/").using_status_code(StatusCode::NOT_FOUND)
    };

    Identity::login(&req.extensions(), user.username).unwrap();
    web::Redirect::to("/").using_status_code(StatusCode::FOUND)
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

    let user = state.database.find_user(&username).await.unwrap();

    let Some(repositories) = state.database.find_user_repositories(&username).await else {
        return Ok(HttpResponse::NotFound().finish());
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
        todo!()
    };

    let username = identity.id().unwrap();

    let user = state.database.find_user(&username).await;

    let repository_name = form.name.clone();
    state
        .database
        .new_repository(&user, &repository_name, None, &form.visibility)
        .await;

    web::Redirect::to(format!("/@{username}/{}", form.name)).using_status_code(StatusCode::FOUND)
}
