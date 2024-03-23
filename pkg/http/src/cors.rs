use crate::header::Header;
use crate::response::Response;

pub fn allow_all_requests(res: &mut Response) {
    // TODO: In order to support passing of credentials like cookies,, this needs to
    // specify a specific origin.
    //
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
    res.head.headers.raw_headers.push(Header::new(
        "Access-Control-Expose-Headers".into(),
        "*".into(),
    ));
    res.head
        .headers
        .raw_headers
        .push(Header::new("Access-Control-Max-Age".into(), "600".into()));
}
