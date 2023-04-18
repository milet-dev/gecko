mod database;
mod diff;
mod issues;
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
    title: &'a str,
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
                                "visibility": "$visibility",
                                "created_at": "$created_at",
                                "updated_at": "$updated_at"
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
        let username = document.get_str("username").unwrap().to_owned();

        let user = state.database.find_user(&username).await.unwrap();

        let Ok(repositories) = document.get_array("repositories") else {
            continue;
        };

        let repositories: Vec<_> = repositories
            .iter()
            .map(|inner| {
                let inner = inner.as_document().unwrap();
                let name = inner.get_str("name");
                let description = inner.get_str("description");
                let visibility = inner.get_str("visibility");
                let created_at = inner.get_i64("created_at");
                let updated_at = inner.get_i64("updated_at");
                Repository {
                    _id: ObjectId::new(),
                    user_id: ObjectId::default(),
                    name: name.unwrap().to_string(),
                    description: description.unwrap().to_string(),
                    visibility: visibility.unwrap().to_string(),
                    created_at: created_at.unwrap(),
                    updated_at: updated_at.unwrap(),
                    issues: vec![],
                }
            })
            .collect();

        let repositories: Vec<_> = match identity.as_ref() {
            Some(identity) if identity._id == user._id => repositories.into_iter().collect(),
            _ => repositories
                .into_iter()
                .filter(|inner| inner.visibility == "public")
                .collect(),
        };

        users.push(User_ {
            username,
            repositories,
        });
    }

    Ok(IndexTemplate {
        title: "gecko",
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
            .service(
                web::resource("/signup")
                    .route(web::get().to(user::signup))
                    .route(web::post().to(user::signup)),
            )
            .service(
                web::resource("/login")
                    .route(web::get().to(user::login))
                    .route(web::post().to(user::login)),
            )
            .service(
                web::resource("/new")
                    .route(web::get().to(user::new))
                    .route(web::post().to(user::new)),
            )
            .service(user::logout)
            .service(user::index)
            .service(index)
            .service(repository::delete)
            .service(
                web::scope("/settings")
                    .default_service(web::get().to(user::settings))
                    .route("/update", web::post().to(user::update))
                    .route("/password", web::get().to(user::password))
                    .route("/update_password", web::post().to(user::update_password)),
            )
            .service(
                web::scope("/@{username}")
                    .service(repository::index)
                    .service(
                        web::scope("/{name}")
                            .route("/branches", web::get().to(repository::branches))
                            .route("/commit/{id}", web::get().to(repository::diff))
                            .service(
                                web::scope("/tree/{branch}")
                                    .default_service(web::get().to(repository::tree))
                                    .route("/{tail}*", web::get().to(repository::tree_)),
                            )
                            .route("/blob/{branch}/{tail}*", web::get().to(repository::tree_))
                            .service(
                                web::scope("/commits")
                                    .default_service(web::get().to(repository::commits))
                                    .route("/{branch}", web::get().to(repository::commits)),
                            )
                            .service(
                                web::scope("/issues")
                                    .default_service(web::get().to(issues::index))
                                    .service(
                                        web::resource("/new")
                                            .route(web::get().to(issues::new))
                                            .route(web::post().to(issues::new)),
                                    )
                                    .service(
                                        web::scope("/{issue_id}")
                                            .default_service(web::get().to(issues::view))
                                            .route("/add", web::post().to(issues::add_comment)),
                                    ),
                            ),
                    ),
            )
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
