use actix_web::{web, App, HttpRequest, HttpServer, Responder, HttpResponse, Result};
use reqwest;

const PORT: u16 = 80;

async fn index() -> impl Responder {
    "Hello world!"
}

async fn api_proxy(_req: HttpRequest, path: web::Path<String>) -> Result<HttpResponse> {
    let client = reqwest::Client::new();
    let base_url = "https://ifsc.results.info";
    
    // Step 1: Get initial page and CSRF token
    let response = client.get(base_url).send().await.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to get initial page: {}", e))
    })?;
    
    let html = response.text().await.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to read response: {}", e))
    })?;
    
    // Extract CSRF token
    let csrf_token = html
        .lines()
        .find(|line| line.contains("csrf-token"))
        .and_then(|line| {
            line.split("content=\"")
                .nth(1)
                .and_then(|s| s.split("\"").next())
        })
        .ok_or_else(|| actix_web::error::ErrorInternalServerError("CSRF token not found"))?;
    
    // Step 2: Call appsignal
    client
        .get(&format!("{}/appsignal", base_url))
        .header("X-Csrf-Token", csrf_token)
        .header("Accept", "application/json")
        .header("Referer", "https://ifsc.results.info/")
        .header("Sec-Fetch-Site", "same-origin")
        .header("Sec-Fetch-Mode", "cors")
        .send()
        .await
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("Failed appsignal call: {}", e))
        })?;
    
    // Step 3: Call entrypoint
    client
        .get(&format!("{}/entrypoint", base_url))
        .header("X-Csrf-Token", csrf_token)
        .header("Accept", "application/json")
        .header("Referer", "https://ifsc.results.info/")
        .header("Sec-Fetch-Site", "same-origin")
        .header("Sec-Fetch-Mode", "cors")
        .send()
        .await
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("Failed entrypoint call: {}", e))
        })?;
    
    // Step 4: Make the actual API call
    let api_path = path.into_inner();
    let api_url = format!("{}/api/{}", base_url, api_path);
    
    let api_response = client
        .get(&api_url)
        .header("X-Csrf-Token", csrf_token)
        .header("Accept", "application/json")
        .header("Referer", "https://ifsc.results.info/")
        .header("Sec-Fetch-Site", "same-origin")
        .header("Sec-Fetch-Mode", "cors")
        .send()
        .await
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("Failed API call: {}", e))
        })?;
    
    let status = api_response.status();
    let content_type = api_response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/json")
        .to_string();
    
    let body = api_response.text().await.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to read API response: {}", e))
    })?;
    
    Ok(HttpResponse::build(actix_web::http::StatusCode::from_u16(status.as_u16()).unwrap())
        .content_type(content_type)
        .body(body))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .service(
                web::scope("/app")
                    .route("/index.html", web::get().to(index)),
            )
            .service(
                web::scope("/api")
                    .route("/{path:.*}", web::get().to(api_proxy)),
            )
    })
    .bind(("127.0.0.1", PORT))?
    .run()
    .await
}