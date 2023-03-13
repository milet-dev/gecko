mod git;

use actix_web::{
    get, http::header::ContentType, web, App, HttpResponse, HttpServer, Responder, Result,
};

#[get("/")]
async fn index() -> impl Responder {
    "Hello, world!"
}

#[get("/repository/{name}")]
async fn repository(path: web::Path<String>) -> Result<impl Responder> {
    let name = path.into_inner();

    let repository = match git::open(&name) {
        Ok(repository) => repository,
        Err(err) => {
            let body = format!(r#"{{ "message": "{err:?}" }}"#);
            let response = HttpResponse::NotFound()
                .content_type(ContentType::json())
                .body(body);
            return Ok(response);
        }
    };

    let response = HttpResponse::Ok()
        .content_type(ContentType::json())
        .body(serde_json::to_string(&repository)?);
    Ok(response)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| App::new().service(index).service(repository))
        .bind(("127.0.0.1", 8080))?
        .run()
        .await
}
