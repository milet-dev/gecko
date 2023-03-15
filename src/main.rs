mod git;
mod model;

use actix_files::Files;
use actix_identity::{Identity, IdentityMiddleware};
use actix_session::{config::PersistentSession, storage::CookieSessionStore, SessionMiddleware};
use actix_web::{
    cookie::{time::Duration, Key},
    error, get,
    http::{header::ContentType, StatusCode},
    post, web, App, HttpMessage, HttpRequest, HttpResponse, HttpServer, Responder, Result,
};
use askama::Template;
use askama_actix::TemplateToResponse;
use mongodb::Client;
use serde::{Deserialize, Serialize};

use crate::model::User;

const DATABASE_NAME: &str = "gecko";

#[get("/")]
async fn index(identity: Option<Identity>) -> Result<impl Responder> {
    let id = match identity.map(|id| id.id()) {
        None => "None".to_owned(),
        Some(Ok(id)) => id,
        Some(Err(err)) => return Err(error::ErrorInternalServerError(err)),
    };
    Ok(format!("{id}"))
}

#[derive(Template)]
#[template(path = "repository.html")]
struct RepositoryTemplate<'a> {
    repository: &'a git::Repository,
}

#[get("/repository/{name}")]
async fn repository(path: web::Path<String>) -> Result<impl Responder> {
    let name = path.into_inner();

    let repository = match git::open(&name) {
        Ok(repository) => repository,
        Err(_err) => {
            return Ok(HttpResponse::NotFound()
                .content_type(ContentType::plaintext())
                .body("Not Found"));
        }
    };

    Ok(RepositoryTemplate {
        repository: &repository,
    }
    .to_response())
}

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
pub struct SignupFormParams {
    email: String,
    username: String,
    password: String,
}

#[post("/signup")]
async fn add_user(
    client: web::Data<Client>,
    params: web::Form<SignupFormParams>,
) -> Result<impl Responder> {
    let collection = client.database(DATABASE_NAME).collection::<User>("users");
    let user = User {
        email: params.email.clone(),
        username: params.username.clone(),
        password: params.password.clone(),
    };
    _ = collection.insert_one(&user, None).await;
    Ok("ok")
}

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate<'a> {
    title: &'a str,
}

#[get("/login")]
async fn login() -> impl Responder {
    LoginTemplate { title: "login" }.to_response()
}

#[derive(Serialize, Deserialize)]
pub struct LoginFormParams {
    username: String,
    password: String,
}

#[post("/login")]
async fn login_(
    client: web::Data<Client>,
    req: HttpRequest,
    params: web::Form<LoginFormParams>,
) -> impl Responder {
    let collection = client.database(DATABASE_NAME).collection::<User>("users");
    let filter =
        bson::doc! { "username": params.username.clone(), "password": params.password.clone() };
    let Ok(Some(user)) = collection.find_one(filter, None).await else {
        return web::Redirect::to("/").using_status_code(StatusCode::NOT_FOUND)
    };

    Identity::login(&req.extensions(), user.username).unwrap();
    web::Redirect::to("/").using_status_code(StatusCode::FOUND)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let secret_key = Key::generate();

    let client = Client::with_uri_str("mongodb://localhost:27017")
        .await
        .unwrap();

    HttpServer::new(move || {
        App::new()
            .wrap(IdentityMiddleware::default())
            .wrap(
                SessionMiddleware::builder(CookieSessionStore::default(), secret_key.clone())
                    .cookie_secure(false)
                    .session_lifecycle(
                        PersistentSession::default().session_ttl(Duration::minutes(1)),
                    )
                    .build(),
            )
            .app_data(web::Data::new(client.clone()))
            .service(Files::new("/static", "static"))
            .service(signup)
            .service(add_user)
            .service(login)
            .service(login_)
            .service(index)
            .service(repository)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
