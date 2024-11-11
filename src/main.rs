use epub::doc::EpubDoc;
use rinja::Template;
use std::io::{Cursor, Read};
use tiny_http::{Header, Method, Response, StatusCode};
use util::*;

mod util;


const READER_JS: &str = include_str!("reader.js");
const STYLES_CSS: &str = include_str!("styles.css");
const CONTENT_STYLES_CSS: &str = include_str!("content_styles.css");

#[derive(Debug, Template)]
#[template(ext = "xhtml", path = "reader.xml")]
struct Reader<'a> {
    title: &'a str,
    styles: &'a str,
    reader_js: &'a str,
    page_url: &'a str,
    page_number: usize,
    page_count: usize,
}

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
    dbg!(&book.resources, &book.spine, &book.metadata);
    let book_title = book.metadata.get("title").into_iter().flatten().map(|x|x.as_str()).next().unwrap_or("Book").to_string();
    let server = tiny_http::Server::http("localhost:6969").unwrap();
    
    let mut page_idx = 0usize;
    let page_count = book.get_num_pages();

    loop {
        let request = match server.recv() {
            Ok(rq) => rq,
            Err(e) => {
                println!("error: {}", e);
                break;
            }
        };

        let request_url = request.url();
        let response = match (request.method(), request_url) {
            (&Method::Post, "/next-page") => {
                if page_idx + 1 < page_count {
                    page_idx += 1;
                }
                dbg!(page_idx, page_count);
                assert!(book.set_current_page(page_idx));
                let path = book.get_current_path().expect("current page");
                let path = path.strip_prefix(&book.root_base).unwrap();
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
            (&Method::Get, "/") => {
                book.set_current_page(page_idx);
                let page_url = book.get_current_path().unwrap();
                //let page_url = book.root_base.join(page_url);
                // Redirect to page url
                Response::from_data(&[])
                    .with_status_code(StatusCode(307))
                    .with_header(Header::from_bytes(b"location", page_url.as_os_str().as_encoded_bytes()).unwrap())
            }
            (&Method::Get, content) if content.starts_with("/content/") => {
                let content = content.strip_prefix("/content/").unwrap();
                let (Some(data), Some(mime)) = (
                   book.get_resource_by_path(&content),
                   book.get_resource_mime_by_path(&content),
                ) else {
                        request
                            .respond(Response::from_string("404").with_status_code(StatusCode(404)))
                            .unwrap();
                        continue;
                };

                let data = if mime == mime::XHTML || mime == mime::HTML {
                   inject_styles(&data, CONTENT_STYLES_CSS)
                } else { data }; 
                Response::from_data(data).with_header(
                   Header::from_bytes(b"Content-Type", mime.as_bytes()).expect("no header?"),
                )
            }
            (&Method::Get, req_url) => {
                let req_url = std::path::PathBuf::from(req_url.trim_start_matches('/'));
                let abs_url = if req_url.starts_with(&book.root_base) {
                    req_url
                } else {
                    book.root_base.join(req_url)
                };

                println!("{request_url} :: looking for {}", abs_url.display());

                if let Some(idx) = book.resource_uri_to_chapter(&abs_url) {
                    // This is a page.
                    page_idx = idx;
                    println!("At idx: {page_idx}");

                    assert!(book.set_current_page(page_idx));
                    let page_url = std::path::PathBuf::from("/content").join(book.get_current_path().unwrap());
                    let page_url = page_url.to_str().unwrap();
                    let rv = Reader {
                        title: &book_title,
                        styles: STYLES_CSS,
                        reader_js: READER_JS,
                        page_url,
                        page_number: page_idx,
                        page_count,
                    };
                    Response::from_string(rv.render().expect("thing inside thing"))
                        .with_header(Header::from_bytes(b"Content-Type", mime::XHTML).unwrap())
                } else {
                    let (Some(data), Some(mime)) = (
                        book.get_resource_by_path(&abs_url),
                        book.get_resource_mime_by_path(&abs_url),
                    ) else {
                        request
                            .respond(Response::from_string("404").with_status_code(StatusCode(404)))
                            .unwrap();
                        continue;
                    };

                    Response::from_data(data).with_header(
                        Header::from_bytes(b"Content-Type", mime.as_bytes()).expect("no header?"),
                    )
                }
            }
            _ => Response::from_string("404").with_status_code(StatusCode(404)),
        };
        request.respond(response).unwrap();
    }
}

fn inject_styles(src: &[u8], css: &str) -> Vec<u8> {
    use quick_xml::{Writer,Reader};
    use quick_xml::events::{Event, BytesStart, BytesEnd, BytesText};
    let src = std::str::from_utf8(src).expect("please use UTF8");
    let mut reader = Reader::from_str(src);
    let mut writer = Writer::new(Cursor::new(Vec::new()));

    loop {
        match reader.read_event() {
            Ok(Event::End(e)) if e.name().as_ref() == b"head" => {
                let mut elem = BytesStart::new("style");
                elem.push_attribute(("type", mime::CSS));
                writer.write_event(Event::Start(elem)).expect("bruh");

                let css = BytesText::new(css);
                writer.write_event(Event::Text(css)).expect("please");

                writer
                    .write_event(Event::End(BytesEnd::new("style")))
                    .expect("<3");
                
                writer.write_event(Event::End(e)).expect("is okay");
            }
            Ok(Event::Eof) => break,
            Ok(e) => assert!(writer.write_event(e.borrow()).is_ok()),
            Err(e) => panic!("XML parse error at position {}: {:?}", reader.error_position(), e),
        }
    }

    writer.into_inner().into_inner()
}
