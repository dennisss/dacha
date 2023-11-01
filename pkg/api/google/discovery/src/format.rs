use std::collections::HashMap;

use common::errors::*;

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
pub struct DirectoryList {
    pub kind: String,
    pub items: Vec<DirectoryItem>,
}

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
pub struct DirectoryItem {
    pub id: String,
    pub name: String,
    pub version: String,
    pub title: String,
    pub description: String,
    pub discoveryRestUrl: String,
    pub documentationLink: Option<String>,
    pub preferred: bool,
}

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
pub struct RestDescription {
    pub baseUrl: String,
    pub name: Option<String>,
    pub canonicalName: Option<String>,
    pub parameters: HashMap<String, JsonSchema>,
    pub resources: HashMap<String, RestResource>,
    pub schemas: HashMap<String, JsonSchema>,
}

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
pub struct RestResource {
    pub methods: Option<HashMap<String, RestMethod>>,
}

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
pub struct RestMethod {
    pub id: Option<String>,
    pub path: String,
    pub flatPath: Option<String>,
    pub httpMethod: String,
    pub parameters: Option<HashMap<String, JsonSchema>>,
    pub parameterOrder: Option<Vec<String>>,
    pub request: Option<ReferencedType>,
    pub response: Option<ReferencedType>,
    pub etagRequired: Option<bool>,

    #[parse(sparse = true)]
    pub supportsMediaDownload: bool,

    #[parse(sparse = true)]
    pub supportsMediaUpload: bool,

    pub mediaUpload: Option<MediaUpload>,
}

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
pub struct ReferencedType {
    // Only might be present in 'request' types
    pub parameterName: Option<String>,

    #[parse(name = "$ref")]
    pub typ_reference: String,
}

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
pub struct JsonSchema {
    pub id: Option<String>,
    pub required: Option<bool>,
    pub properties: Option<HashMap<String, JsonSchema>>,

    pub description: Option<String>,

    #[parse(name = "type")]
    pub typ: Option<String>,
    #[parse(name = "$ref")]
    pub typ_reference: Option<String>,
    pub items: Option<Box<JsonSchema>>,

    pub default: Option<String>,
    pub format: Option<String>,
    pub location: Option<String>,
}

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
pub struct MediaUpload {
    pub protocols: MediaUploadProtocols,
}

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
pub struct MediaUploadProtocols {
    pub simple: MediaUploadProtocol,
    pub resumable: MediaUploadProtocol,
}

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
pub struct MediaUploadProtocol {
    pub path: String,

    #[parse(sparse = true)]
    pub multipart: bool,
}
