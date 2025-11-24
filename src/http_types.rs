use bytes::Bytes;
use std::collections::HashMap;

/// HTTP Request data (shared type for both runtimes)
#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Option<Bytes>,
}

/// HTTP Response data (shared type for both runtimes)
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Option<Bytes>,
}

// Actix-web conversions (only available in dev/examples)
#[cfg(feature = "actix")]
impl HttpRequest {
    /// Convert from actix_web::HttpRequest + body bytes
    pub fn from_actix(req: &actix_web::HttpRequest, body: Bytes) -> Self {
        let method = req.method().to_string();
        let url = format!(
            "{}://{}{}",
            req.connection_info().scheme(),
            req.connection_info().host(),
            req.uri()
        );

        let mut headers = HashMap::new();
        for (key, value) in req.headers() {
            if let Ok(val_str) = value.to_str() {
                headers.insert(key.to_string(), val_str.to_string());
            }
        }

        HttpRequest {
            method,
            url,
            headers,
            body: if body.is_empty() { None } else { Some(body) },
        }
    }
}

#[cfg(feature = "actix")]
impl From<HttpResponse> for actix_web::HttpResponse {
    fn from(res: HttpResponse) -> Self {
        let mut builder = actix_web::HttpResponse::build(
            actix_web::http::StatusCode::from_u16(res.status)
                .unwrap_or(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR),
        );

        for (key, value) in res.headers {
            builder.insert_header((key.as_str(), value.as_str()));
        }

        match res.body {
            Some(body) => builder.body(body),
            None => builder.finish(),
        }
    }
}
