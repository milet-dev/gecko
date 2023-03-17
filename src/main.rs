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
use mongodb::Client;

const DATABASE_NAME: &str = "gecko";

#[derive(Clone)]
pub struct State {
    pub db: mongodb::Database,
}

#[get("/")]
async fn index(identity: Option<Identity>) -> Result<impl Responder> {
    let id = match identity.map(|id| id.id()) {
        None => "None".to_owned(),
        Some(Ok(id)) => id,
        Some(Err(err)) => return Err(error::ErrorInternalServerError(err)),
    };
    Ok(id)
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
            .service(repository::index)
            .service(
                web::scope("/repository/{name}")
                    .service(repository::tree)
                    .service(repository::commits),
            )
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
