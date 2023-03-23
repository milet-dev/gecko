mod database;
mod model;
mod repository;
mod user;

use crate::model::{Repository, User};
use actix_files::Files;
use actix_identity::{Identity, IdentityMiddleware};
use actix_session::{config::PersistentSession, storage::CookieSessionStore, SessionMiddleware};
use actix_web::{
    cookie::{time::Duration, Key},
    get,
    middleware::NormalizePath,
    web, App, HttpServer, Responder, Result,
};
use askama::Template;
use askama_actix::TemplateToResponse;
use bson::oid::ObjectId;
use database::Database;
use futures::TryStreamExt;
use mongodb::Client;

const DATABASE_NAME: &str = "gecko";

#[derive(Clone)]
pub struct State {
    pub db: mongodb::Database,
    pub database: Database,
}

#[derive(Clone, serde::Deserialize)]
pub struct User_ {
    username: String,
    repositories: Vec<Repository>,
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate<'a> {
    identity: &'a Option<User>,
    users: &'a [User_],
}

#[get("/")]
async fn index(state: web::Data<State>, identity: Option<Identity>) -> Result<impl Responder> {
    let identity = match identity {
        Some(identity) => match identity.id() {
            Ok(id) => state.database.find_user(&id).await,
            Err(_) => todo!(),
        },
        None => None,
    };

    let collection = state.db.collection::<User>("users");

    let output = collection
        .aggregate(
            [bson::doc! {
                "$lookup": {
                    "from": "repositories",
                    "localField": "_id",
                    "foreignField": "user_id",
                    "pipeline": [
                        {
                            "$project": {
                                "user_id": "$user_id",
                                "name": "$name",
                                "description": "$description",
                                "visibility": "$visibility"
                            }
                        },
                    ],
                    "as": "repositories"
                }
            }],
            None,
        )
        .await;

    let mut cursor = output.unwrap();
    let mut users: Vec<User_> = Vec::new();
    while let Some(document) = cursor.try_next().await.unwrap() {
        let Ok(repositories) = document.get_array("repositories") else {
            continue;
        };
        users.push(User_ {
            username: document.get_str("username").unwrap().to_owned(),
            repositories: repositories
                .iter()
                .map(|inner| {
                    let inner = inner.as_document().unwrap();
                    let name = inner.get_str("name");
                    let description = inner.get_str("description");
                    let visibility = inner.get_str("visibility");
                    Repository {
                        user_id: ObjectId::default(),
                        name: name.unwrap().to_string(),
                        description: description.unwrap().to_string(),
                        visibility: visibility.unwrap().to_string(),
                    }
                })
                .collect(),
        });
    }

    Ok(IndexTemplate {
        identity: &identity,
        users: &users,
    }
    .to_response())
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let secret_key = Key::generate();

    let client = Client::with_uri_str("mongodb://localhost:27017")
        .await
        .unwrap();

    let database = Database::new(&client);
    let state = State {
        db: client.database(DATABASE_NAME),
        database,
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
            .service(user::new)
            .service(user::new_internal)
            .service(index)
            .service(user::index)
            .service(
                web::scope("/@{username}")
                    .service(repository::index)
                    .service(
                        web::scope("/{name}")
                            .service(repository::_tree)
                            .service(repository::tree)
                            .service(repository::_commits)
                            .service(repository::commits),
                    ),
            )
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
