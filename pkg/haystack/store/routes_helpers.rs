
use std::io;
use std::io::{Cursor};
use rocket::http::{Status, ContentType};
use rocket::request::{Request};
use rocket::response::{Response, Responder};
use mime_sniffer::MimeTypeSniffer;
use super::super::common::*;
use super::needle::*;

