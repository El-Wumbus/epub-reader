use epub::doc::EpubDoc;
use log::{debug, error, info};
use rinja::Template;
use std::io::{Cursor, Read};
use tiny_http::{Header, Method, Request, Response, StatusCode};
use util::*;

mod util;

const READER_JS: &str = include_str!("reader.js");
//const STYLES_CSS: &str = include_str!("styles.css");

#[derive(Debug, Template)]
#[template(ext = "xhtml", path = "reader.xml")]
struct Reader<'a> {
    title: &'a str,
    stylesheet: &'a str,
    reader_js: &'a str,
    page_url: &'a str,
    current_page: usize,
    page_count: usize,
}

#[derive(Debug, Clone, Template)]
#[template(path = "content_styles.css", escape = "none")]
struct ContentStyles {
    font_size_px: usize,
    fg_color: String,
    bg_color: String,
}

impl Default for ContentStyles {
    fn default() -> Self {
        Self {
            font_size_px: 20,
            fg_color: "var(--color-primary-a50)".to_string(),
            bg_color: "var(--color-surface-a0)".to_string(),
        }
    }
}

#[derive(Debug, Clone, Template)]
#[template(path = "reader.css", escape = "none")]
struct ReaderStyles {
    fg_color: String,
    bg_color: String,
}

impl Default for ReaderStyles {
    fn default() -> Self {
        Self {
            fg_color: "var(--color-primary-a50)".to_string(),
            bg_color: "var(--color-surface-a0)".to_string(),
        }
    }
}

struct BookState<R: std::io::Read + std::io::Seek> {
    book: EpubDoc<R>,
    content_styles: ContentStyles,
    reader_styles: ReaderStyles,
    current_page: usize,
    page_count: usize,
}

impl<R: std::io::Read + std::io::Seek> BookState<R> {
    fn new(book: EpubDoc<R>) -> Self {
        let page_count = book.get_num_pages();
        Self {
            book,
            content_styles: ContentStyles::default(),
            reader_styles: ReaderStyles::default(),
            current_page: 0,
            page_count,
        }
    }

    fn change_page(
        &mut self,
        pred: impl Fn(usize, usize) -> usize,
    ) -> Result<Response<Cursor<Vec<u8>>>, ()> {
        self.current_page = pred(self.current_page, self.page_count);
        assert!(
            self.book.set_current_page(self.current_page),
            "page index should be valid"
        );
        let path = self.book.get_current_path().ok_or(())?;
        //let path = path
        // .strip_prefix(&self.book.root_base)
        // .expect("page_path to be prefixed with the base path");
        let path = path.to_str().unwrap();
        info!(
            "Moved to page: {}/{} at path \"{}\"",
            self.current_page + 1,
            self.page_count,
            path
        );
        Ok(Response::from_string(path))
    }
}

fn main() {
    let mut builder = env_logger::Builder::new();
    builder.filter_level(log::LevelFilter::Debug).init();

    let args = std::env::args();
    let book = args
        .skip(1)
        .next()
        .expect("filename to be provided as the first command-line argument");

    let mut book = std::fs::File::open(book).expect("File to open properly");
    let mut book_buffer = Vec::new();
    book.read_to_end(&mut book_buffer).expect("readable file");
    let book = EpubDoc::from_reader(Cursor::new(book_buffer)).expect("valid epub archive");
    debug!(
        "Metadata: {:#?}\nSpine: {:#?}\nResources = {:#?}\n",
        book.metadata, book.spine, book.resources
    );

    let book_title = book
        .metadata
        .get("title")
        .into_iter()
        .flatten()
        .map(|x| x.as_str())
        .next()
        .unwrap_or("Missing Title")
        .to_string();
    let mut state = BookState::new(book);
    let server = tiny_http::Server::http("localhost:6969").unwrap();

    loop {
        let mut request = match server.recv() {
            Ok(rq) => rq,
            Err(e) => {
                error!("server: {}", e);
                break;
            }
        };

        let request_method = request.method();
        let request_url = request.url().to_string();
        info!(
            "Recevied request: method = {}, url = {}",
            request_method, request_url
        );

        let response = match (request_method, request_url.as_str()) {
            (&Method::Post, "/api/next-page") => {
                match state.change_page(|page, page_count| {
                    if page + 1 < page_count {
                        page + 1
                    } else {
                        page
                    }
                }) {
                    Ok(r) => r,
                    Err(_) => {
                        respond(request, rcode(500));
                        continue;
                    }
                }
            }
            (&Method::Post, "/api/prev-page") => {
                match state.change_page(|page, _| if page != 0 { page - 1 } else { page }) {
                    Ok(r) => r,
                    Err(_) => {
                        respond(request, rcode(500));
                        continue;
                    }
                }
            }
            (&Method::Post, "/api/current-page") => {
                let mut page = String::new();
                request.as_reader().read_to_string(&mut page).unwrap();
                let Ok(page) = page.parse::<usize>() else {
                    respond(request, rcode(500));
                    continue;
                };
                let Ok(r) = state.change_page(|_, _| page - 1) else {
                    respond(request, rcode(500));
                    continue;
                };
                r
            }
            (Method::Post, "/api/increase-font-size") => {
                state.content_styles.font_size_px += 2;
                info!(
                    "Increasing font size from {} to {}",
                    state.content_styles.font_size_px - 2,
                    state.content_styles.font_size_px
                );
                rcode(200)
            }
            (Method::Post, "/api/decrease-font-size") => {
                if state.content_styles.font_size_px - 2 != 0 {
                    state.content_styles.font_size_px -= 2
                };
                info!(
                    "Decreasing font size from {} to {}",
                    state.content_styles.font_size_px + 2,
                    state.content_styles.font_size_px
                );
                rcode(200)
            }
            (Method::Post, "/api/invert-text-color") => {
                let cbg = state.content_styles.bg_color;
                state.content_styles.bg_color = state.content_styles.fg_color;
                state.content_styles.fg_color = cbg;
                let rbg = state.reader_styles.bg_color;
                state.reader_styles.bg_color = state.reader_styles.fg_color;
                state.reader_styles.fg_color = rbg;
                debug!("Inverted content styles: {:?}", state.content_styles);
                info!("Inverted text color");
                rcode(200)
            }
            (&Method::Get, "/") => {
                assert!(state.book.set_current_page(state.current_page));
                let page_url = state.book.get_current_path().unwrap();
                //let page_url = book.root_base.join(page_url);
                // Redirect to page url
                Response::from_data(&[])
                    .with_status_code(StatusCode(307))
                    .with_header(
                        Header::from_bytes(b"location", page_url.as_os_str().as_encoded_bytes())
                            .unwrap(),
                    )
            }
            (&Method::Get, content) if content.starts_with("/content/") => {
                let content = content.strip_prefix("/content/").unwrap();
                let (Some(data), Some(mime)) = (
                    state.book.get_resource_by_path(&content),
                    state.book.get_resource_mime_by_path(&content),
                ) else {
                    request
                        .respond(Response::from_string("404").with_status_code(StatusCode(404)))
                        .unwrap();
                    continue;
                };

                let data = if mime == mime::XHTML || mime == mime::HTML {
                    let content_styles = state
                        .content_styles
                        .render()
                        .expect("be a good template pls");
                    debug!("rendered content styles: {content_styles}");
                    let data = std::str::from_utf8(&data).unwrap();
                    inject_styles2(&data, &content_styles).into_bytes()
                    //inject_styles(&data, &content_styles)
                } else {
                    data
                };
                Response::from_data(data).with_header(
                    Header::from_bytes(b"Content-Type", mime.as_bytes()).expect("no header?"),
                )
            }
            (&Method::Get, req_url) => {
                let req_url = std::path::PathBuf::from(req_url.trim_start_matches('/'));
                let abs_url = if req_url.starts_with(&state.book.root_base) {
                    req_url
                } else {
                    state.book.root_base.join(req_url)
                };

                println!("{request_url} :: looking for {}", abs_url.display());

                if let Some(idx) = state.book.resource_uri_to_chapter(&abs_url) {
                    if idx != state.current_page {
                        state.current_page = idx;
                        info!(
                            "Set page to {} / {}",
                            state.current_page + 1,
                            state.page_count
                        );
                    }
                    assert!(
                        state.book.set_current_page(state.current_page),
                        "{} should be valid",
                        state.current_page
                    );
                    let Some(page_path) = state.book.get_current_path() else {
                        respond(request, rcode(500));
                        continue;
                    };
                    let page_url = std::path::PathBuf::from("/content").join(page_path);
                    let page_url = page_url.to_str().unwrap();
                    
                    let stylesheet = state.reader_styles.render().unwrap();
                    let rv = Reader {
                        title: &book_title,
                        stylesheet: &stylesheet,
                        reader_js: READER_JS,
                        page_url,

                        current_page: state.current_page + 1,
                        page_count: state.page_count,
                    };
                    Response::from_string(rv.render().expect("thing inside thing"))
                        .with_header(Header::from_bytes(b"Content-Type", mime::XHTML).unwrap())
                } else {
                    let (Some(data), Some(mime)) = (
                        state.book.get_resource_by_path(&abs_url),
                        state.book.get_resource_mime_by_path(&abs_url),
                    ) else {
                        request.respond(rcode(404)).unwrap();
                        continue;
                    };

                    Response::from_data(data).with_header(
                        Header::from_bytes(b"Content-Type", mime.as_bytes()).expect("no header?"),
                    )
                }
            }
            _ => rcode(404),
        };

        respond(request, response);
    }
}

fn inject_styles2(src: &str, stylesheet: &str) -> String {
    use xmlparser::{ElementEnd, Token};
    let mut output = String::with_capacity(src.len() + stylesheet.len());
    for token in xmlparser::Tokenizer::from(src) {
        match token {
            Ok(Token::Attribute { span, .. }) => {
                output.push(' ');
                output.push_str(span.as_str());
            }
            Ok(Token::ElementEnd {
                end: ElementEnd::Close(_, ename),
                span,
                ..
            }) if ename.as_str() == "head" => {
                output.push_str(r#"<style type="text/css">"#);
                output.push('\n');
                output.push_str(stylesheet);
                output.push('\n');
                output.push_str("</style>");
                output.push('\n');

                output.push_str(span.as_str());
            }
            Ok(t) => {
                output.push_str(t.span().as_str());
            }
            Err(e) => panic!("XML parse error: {e}"),
        }
    }
    output
}

fn rcode(status: u16) -> Response<Cursor<Vec<u8>>> {
    Response::from_string(status.to_string()).with_status_code(StatusCode(status))
}

fn respond<R: std::io::Read>(request: Request, response: Response<R>) {
    let request_url = request.url().to_string();

    info!(
        "Responding to {request_url} wtih status: {}",
        response.status_code().0
    );
    if let Err(e) = request.respond(response) {
        error!("Failed to respond to {request_url}: {}", e);
    }
}
