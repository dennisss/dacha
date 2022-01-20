use crate::header::Header;
use crate::response::Response;

pub fn allow_all_requests(res: &mut Response) {
    // TODO: Add "Vary: Origin"
    res.head.headers.raw_headers.push(Header::new(
        "Access-Control-Allow-Origin".into(),
        "*".into(),
    ));
    res.head.headers.raw_headers.push(Header::new(
        "Access-Control-Allow-Methods".into(),
        "*".into(),
    ));
    res.head.headers.raw_headers.push(Header::new(
        "Access-Control-Allow-Headers".into(),
        "*".into(),
    ));
    res.head.headers.raw_headers.push(Header::new(
        "Access-Control-Allow-Credentials".into(),
        "true".into(),
    ));
}
