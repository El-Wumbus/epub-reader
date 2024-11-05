use epub::doc::EpubDoc;
use std::io::{Cursor, Read};
use tiny_http::{Header, Method, Response, StatusCode};

const INDEX: &'static str = include_str!("index.xhtml");
const INDEX_JAVASCRIPT: &'static str = include_str!("index.js");
const MIME_XHTML: &'static str = "application/xhtml+xml";

fn main() {
    let args = std::env::args();
    let book = args
        .skip(1)
        .next()
        .expect("filename to be provided as the first command-line argument");

    let mut book = std::fs::File::open(book).expect("File to open properly");
    let mut book_buffer = Vec::new();
    book.read_to_end(&mut book_buffer).expect("readable file");
    let mut book = EpubDoc::from_reader(Cursor::new(book_buffer)).expect("valid epub archive");
    dbg!(&book.resources, &book.spine);
    let server = tiny_http::Server::http("localhost:6969").unwrap();

    let mut page_idx = 0usize;
    let page_count = book.get_num_pages();
    loop {
        // blocks until the next request is received
        let request = match server.recv() {
            Ok(rq) => rq,
            Err(e) => {
                println!("error: {}", e);
                break;
            }
        };

        dbg!(&request);
        let response = match (request.method(), request.url()) {
            (&Method::Get, "/") | (&Method::Get, "/index.xhtml") => {
                let mut response = Response::from_string(INDEX);
                response.add_header(
                    Header::from_bytes(b"Content-Type", MIME_XHTML)
                        .expect("sexy header"),
                );
                response
            }
            (&Method::Get, "/index.js") => {
                let mut response = Response::from_string(INDEX_JAVASCRIPT);
                response.add_header(
                    Header::from_bytes(b"Content-Type", "text/javascript").expect("sexy header"),
                );
                response
            }
            (&Method::Post, "/next-page") => {
                if page_idx + 1 < page_count {
                    page_idx += 1;
                }
                dbg!(page_idx, page_count);
                assert!(book.set_current_page(page_idx));
                let path = book.get_current_path().expect("current page");
                let path = path.strip_prefix(book.root_base.as_path()).unwrap();
                Response::from_string(path.to_str().unwrap())
            }
            (&Method::Post, "/prev-page") => {
                if page_idx != 0 {
                    page_idx -= 1;
                }
                dbg!(page_idx, page_count);
                assert!(book.set_current_page(page_idx));
                let path = book.get_current_path().expect("current page");
                let path = path.strip_prefix(book.root_base.as_path()).unwrap();
                Response::from_string(path.to_str().unwrap())
            }
            (&Method::Get, "/page") => {
                let (data, mime) = book.get_current().expect("current page");
                let mut response = Response::from_data(data);
                response.add_header(
                    Header::from_bytes(b"Content-Type", mime.as_bytes()).expect("sexy header"),
                );
                response
            }
            (&Method::Get, req_url) => {
                let req_url = req_url.trim_start_matches('/');
                let abs_url = book.root_base.join(req_url);
                println!("Looking for {}", abs_url.display());
                let (Some(data), Some(mime)) = (
                    book.get_resource_by_path(&abs_url),
                    book.get_resource_mime_by_path(&abs_url),
                ) else {
                    request
                        .respond(Response::from_string("404").with_status_code(StatusCode(404)))
                        .unwrap();
                    continue;
                };
                if let Some(idx) = book.resource_uri_to_chapter(&abs_url) {
                    println!("At idx: {idx}");
                    page_idx = idx;
                }
                println!("got type: {mime}");
                let mut response = Response::from_data(data);
                response.add_header(
                    Header::from_bytes(b"Content-Type", mime.as_bytes()).expect("sexy header"),
                );
                response
            }
            _ => Response::from_string("404").with_status_code(StatusCode(404)),
        };
        request.respond(response).unwrap();
    }
}
