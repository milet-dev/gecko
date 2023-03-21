mod model;
mod repository;
mod user;

use actix_files::Files;
use actix_identity::{Identity, IdentityMiddleware};
use actix_session::{config::PersistentSession, storage::CookieSessionStore, SessionMiddleware};
use actix_web::{
    cookie::{time::Duration, Key},
    error, get,
    middleware::NormalizePath,
    web, App, HttpServer, Responder, Result,
};
use askama::Template;
use askama_actix::TemplateToResponse;
use futures::TryStreamExt;
use mongodb::{options::FindOptions, Client};

use crate::model::User;

const DATABASE_NAME: &str = "gecko";

#[derive(Clone)]
pub struct State {
    pub db: mongodb::Database,
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate<'a> {
    users: &'a [model::User],
}

#[get("/")]
async fn index(state: web::Data<State>, identity: Option<Identity>) -> Result<impl Responder> {
    let id = match identity.map(|id| id.id()) {
        None => "None".to_owned(),
        Some(Ok(id)) => id,
        Some(Err(err)) => return Err(error::ErrorInternalServerError(err)),
    };

    let users_collection = state.db.collection::<User>("users");

    let filter = bson::doc! {};
    let find_options = FindOptions::builder()
        .projection(bson::doc! {
            "username": 1,
            "email": "",
            "password": "",
            "repositories.name": 1,
            "repositories.description": 1
        })
        .build();

    let mut users = Vec::new();
    if let Ok(mut cursor) = users_collection.find(filter, find_options).await {
        while let Some(user) = cursor.try_next().await.unwrap() {
            users.push(user.clone());
        }
    }

    Ok(IndexTemplate { users: &users }.to_response())
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let secret_key = Key::generate();

    let client = Client::with_uri_str("mongodb://localhost:27017")
        .await
        .unwrap();

    let state = State {
        db: client.database(DATABASE_NAME),
    };

    HttpServer::new(move || {
        App::new()
            .wrap(IdentityMiddleware::default())
            .wrap(
                SessionMiddleware::builder(CookieSessionStore::default(), secret_key.clone())
                    .cookie_secure(false)
                    .session_lifecycle(PersistentSession::default().session_ttl(Duration::WEEK))
                    .build(),
            )
            .wrap(NormalizePath::default())
            .app_data(web::Data::new(client.clone()))
            .app_data(web::Data::new(state.clone()))
            .service(Files::new("/static", "static"))
            .service(user::signup)
            .service(user::signup_internal)
            .service(user::login)
            .service(user::login_internal)
            .service(user::logout)
            .service(index)
            .service(user::index)
            .service(
                web::scope("/@{username}")
                    .service(repository::index)
                    .service(repository::_tree)
                    .service(repository::tree)
                    .service(repository::_commits)
                    .service(repository::commits),
            )
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
