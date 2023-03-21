use crate::{
    model::{self, User},
    State,
};
use actix_identity::Identity;
use actix_web::{
    get, http::StatusCode, post, web, HttpMessage, HttpRequest, HttpResponse, Responder,
};
use askama::Template;
use askama_actix::TemplateToResponse;
use mongodb::options::FindOneOptions;
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
        email: params.email.clone(),
        username: params.username.clone(),
        password: params.password.clone(),
        repositories: Vec::new(),
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
            HttpResponse::Ok().body("")
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

    let users_collection = state.db.collection::<User>("users");
    let filter = bson::doc! { "username": username, "password": password };
    let Ok(Some(user)) = users_collection.find_one(filter, None).await else {
        return web::Redirect::to("/").using_status_code(StatusCode::NOT_FOUND)
    };

    Identity::login(&req.extensions(), user.username).unwrap();
    web::Redirect::to("/").using_status_code(StatusCode::FOUND)
}

#[derive(Template)]
#[template(path = "user/index.html")]
struct IndexTemplate<'a> {
    user: &'a model::User,
}

#[get("/@{username}")]
async fn index(
    path: web::Path<String>,
    state: web::Data<State>,
    identity: Option<Identity>,
) -> actix_web::Result<impl Responder> {
    let username = path.into_inner();

    let collection = state.db.collection::<User>("users");

    let filter = bson::doc! { "username": &username };
    let find_options = FindOneOptions::builder()
        .projection(bson::doc! {
            "username": 1,
            "email": "",
            "password": "",
            "repositories.name": 1,
            "repositories.description": 1
        })
        .build();
    let Ok(Some(user)) = collection.find_one(filter, find_options).await else {
        return Ok(HttpResponse::NotFound().body(""));
    };

    Ok(IndexTemplate { user: &user }.to_response())
}
