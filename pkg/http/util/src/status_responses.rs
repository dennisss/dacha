
pub fn not_found() -> http::Response {
    http::ResponseBuilder::new()
        .status(http::status_code::NOT_FOUND)
        .build()
        .unwrap()
}

pub fn bad_request() -> http::Response {
    http::ResponseBuilder::new()
        .status(http::status_code::BAD_REQUEST)
        .build()
        .unwrap()
}

pub fn internal_server_error() -> http::Response {
    http::ResponseBuilder::new()
        .status(http::status_code::INTERNAL_SERVER_ERROR)
        .build()
        .unwrap()
}

pub fn forbidden() -> http::Response {
    http::ResponseBuilder::new()
        .status(http::status_code::FORBIDDEN)
        .build()
        .unwrap()
}
