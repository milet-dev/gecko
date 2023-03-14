mod git;
mod model;

use actix_web::{
    get, http::header::ContentType, post, web, App, HttpResponse, HttpServer, Responder, Result,
};
use askama::Template;
use askama_actix::TemplateToResponse;
use mongodb::Client;
use serde::{Deserialize, Serialize};

use crate::model::User;

const DATABASE_NAME: &str = "gecko";

#[get("/")]
async fn index() -> impl Responder {
    "Hello, world!"
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
            /* let body = format!(r#"{{ "message": "{err:?}" }}"#);
            let response = HttpResponse::NotFound()
                .content_type(ContentType::json())
                .body(body);
            return Ok(response); */
            return Ok(HttpResponse::NotFound()
                .content_type(ContentType::plaintext())
                .body("Not Found"));
        }
    };

    /* let response = HttpResponse::Ok()
        .content_type(ContentType::json())
        .body(serde_json::to_string(&repository)?);
    Ok(response) */

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
    };
    _ = collection.insert_one(&user, None).await;
    Ok("ok")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let client = Client::with_uri_str("mongodb://localhost:27017")
        .await
        .unwrap();
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(client.clone()))
            .service(signup)
            .service(add_user)
            .service(index)
            .service(repository)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
