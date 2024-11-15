use epub::doc::EpubDoc;
use log::{debug, error};
use rinja::Template;
use std::io::{Cursor, Read};
use tiny_http::{Header, Method, Request, Response, StatusCode};
use slime::parser::{ini, UnParser as _};
//use util::*;
//mod util;
pub const XHTML: &str = "application/xhtml+xml";
    pub const HTML: &str = "text/html";
    pub const JSON: &str = "application/json";
    pub const CSS: &str = "text/css";
const READER_JS: &str = include_str!("reader.js");

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

#[derive(Debug, Clone, Copy)]
struct CSSVariables<'a> {
    fg_color: &'a str,
    bg_color: &'a str,
    content_width_em: f32,
    content_font_size_px: u32,
}

impl<'a> Default for CSSVariables<'a> {
    fn default() -> Self {
        Self {
            fg_color: "var(--color-primary-a50)",
            bg_color: "var(--color-surface-a0)",
            content_font_size_px: 21,
            content_width_em: 36.0,
        }
    }
}

// parse from an INI file
impl<'a> TryFrom<ini::Parse<'a>> for CSSVariables<'a> {
    type Error = &'static str;

    fn try_from(ini: ini::Parse<'a>) -> Result<CSSVariables<'a>, Self::Error> {
        let mut vars = Self::default();
        for ini::Pair { key, value, .. } in ini.filter(|x|x.section == "css") {
            match key {
                "fg_color" => vars.fg_color = value,
                "bg_color" => vars.bg_color = value,
                "content_font_size_px" => vars.content_font_size_px = value.parse().map_err(|_| "Invalid content_font_size_px")?,
                "content_width_em" => vars.content_width_em = value.parse().map_err(|_| "Invalid content_width_em")?,
                _ => {},
            }
        }

        Ok(vars)
    }
}

impl<'a> From<CSSVariables<'a>> for [ini::Pair<'a>;4] {
    fn from(vars: CSSVariables<'a>) -> Self {
        [
            ini::Pair { section: "css", key: "fg_color", value: vars.fg_color},
            ini::Pair { section: "css", key: "bg_color", value: vars.bg_color},
            ini::Pair { section: "css", key: "content_font_size_px", value: Box::leak(Box::new(vars.content_font_size_px.to_string()))},
            ini::Pair { section: "css", key: "content_width_em", value: Box::leak(Box::new(vars.content_width_em.to_string()))},
        ]
    }

}

#[derive(Debug, Clone, Default, Template)]
#[template(path = "content_styles.css", escape = "none")]
struct ContentStyles<'a> {
    variables: CSSVariables<'a>,
}

#[derive(Debug, Clone, Default, Template)]
#[template(path = "reader.css", escape = "none")]
struct ReaderStyles<'a> {
    variables: CSSVariables<'a>,
}

struct BookState<'a, R: std::io::Read + std::io::Seek> {
    book: EpubDoc<R>,
    css_variables: CSSVariables<'a>,
    current_page: usize,
    page_count: usize,
}

impl<'a, R: std::io::Read + std::io::Seek> BookState<'a, R> {
    fn new(book: EpubDoc<R>) -> Self {
        let page_count = book.get_num_pages();
        Self {
            book,
            css_variables: CSSVariables::default(),
            current_page: 0,
            page_count,
        }
    }

    fn change_page(
        &mut self,
        pred: impl Fn(usize, usize) -> usize,
    ) -> Result<Response<Cursor<Vec<u8>>>, ()> {
        self.current_page = pred(self.current_page, self.page_count)
            .clamp(0, self.page_count - 1);

        assert!(
            self.book.set_current_page(self.current_page),
            "page index should be valid"
        );
        let path = self.book.get_current_path().ok_or(())?;
        let path = path.to_str().unwrap();
        debug!(
            "Moved to page: {}/{} at path \"{}\"",
            self.current_page + 1,
            self.page_count,
            path
        );
        Ok(Response::from_string(path))
    }
}

#[derive(Debug, Clone, Default)]
struct Config<'a> {
    css_variables: CSSVariables<'a>,
}

fn main() {
    env_logger::Builder::from_env("READER_LOG").filter_level(log::LevelFilter::Info).write_style(env_logger::fmt::WriteStyle::Always).init();
    
    let config_home = slime::xdg::Dirs::config_home_dir().expect("home dir");
    let config_file = config_home.join("epub-reader").join("config.ini");

    debug!("Using \"{}\" as config file", config_file.display());
    let config = if !config_file.exists() {
        let config = Config::default();
        if let Some(parent) = config_file.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).expect("create parent dir");
            }
        }
        let mut config_f = std::fs::File::create(&config_file).unwrap();
        let ting: [ini::Pair;4] = config.css_variables.into();
        ting.into_iter().serialize(&mut config_f).expect("good time writing to file");
        config
    } else {
        let config_s = std::fs::read_to_string(&config_file).unwrap();
        let config_s = Box::leak(Box::new(config_s));
        let i = ini::Parse::from(config_s.as_str());
        Config { css_variables: i.try_into().expect("valid config") }
    };

    let args = std::env::args();
    let book = args
        .skip(1)
        .next()
        .expect("filename to be provided as the first command-line argument");

    let mut book = std::fs::File::open(book).expect("File to open properly");
    let mut book_buffer = Vec::new();
    book.read_to_end(&mut book_buffer).expect("readable file");
    let book = EpubDoc::from_reader(Cursor::new(book_buffer))
        .expect("valid epub archive");
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
    state.css_variables = config.css_variables;

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
        debug!(
            "Recevied request: method = {}, url = {}",
            request_method, request_url
        );

        let response = match (request_method, request_url.as_str()) {
            (&Method::Post, "/api/page") => {
                let mut req_body = String::new();
                request.as_reader().read_to_string(&mut req_body).unwrap();

                match req_body.trim() {
                    "+" => match state.change_page(|page, page_count| {
                        if page + 1 < page_count {
                            page + 1
                        } else {
                            page
                        }
                    }) {
                        Ok(r) => r,
                        Err(_) => rcode(500),
                    },
                    "-" => {
                        match state.change_page(|page, _| {
                            if page != 0 { page - 1 } else { page }
                        }) {
                            Ok(r) => r,
                            Err(_) => rcode(500),
                        }
                    }
                    _ => {
                        let Ok(page) = req_body.parse::<usize>() else {
                            respond(request, rcode(400));
                            continue;
                        };
                        let Ok(r) = state.change_page(|_, _| page.max(1) - 1)
                        else {
                            respond(request, rcode(500));
                            continue;
                        };
                        r
                    }
                }
            }
            (Method::Post, "/api/font-size") => {
                let mut req_body = String::new();
                request.as_reader().read_to_string(&mut req_body).unwrap();
                match req_body.trim() {
                    "+" => {
                        state.css_variables.content_font_size_px += 2;
                        debug!(
                            "Increasing font size from {} to {}",
                            state.css_variables.content_font_size_px - 2,
                            state.css_variables.content_font_size_px
                        );
                        rcode(200)
                    }
                    "-" => {
                        if state.css_variables.content_font_size_px - 2 != 0 {
                            state.css_variables.content_font_size_px -= 2;
                        };
                        debug!(
                            "Decreasing font size from {} to {}",
                            state.css_variables.content_font_size_px + 2,
                            state.css_variables.content_font_size_px
                        );
                        rcode(200)
                    }
                    _ => rcode(400),
                }
            }
            (Method::Post, "/api/invert-text-color") => {
                let bg = state.css_variables.bg_color;
                state.css_variables.bg_color = state.css_variables.fg_color;
                state.css_variables.fg_color = bg;
                debug!("Inverted content styles: {:?}", state.css_variables);
                debug!("Inverted text color");
                rcode(200)
            }
            (Method::Post, "/api/content-width") => {
                let mut req_body = String::new();
                request
                    .as_reader()
                    .read_to_string(&mut req_body)
                    .expect("the client should send a string");

                match req_body.trim() {
                    "+" => {
                        state.css_variables.content_width_em += 1.0;
                        debug!(
                            "Increased content width to {}",
                            state.css_variables.content_width_em
                        );
                        rcode(200)
                    }
                    "-" => {
                        if state.css_variables.content_width_em > 20.0 {
                            state.css_variables.content_width_em -= 1.0;
                        }
                        debug!(
                            "Increased content width to {}",
                            state.css_variables.content_width_em
                        );
                        rcode(200)
                    }
                    _ => rcode(400),
                }
            }
            (&Method::Get, "/") => {
                assert!(state.book.set_current_page(state.current_page));
                let page_url = state.book.get_current_path().unwrap();
                //let page_url = book.root_base.join(page_url);
                // Redirect to page url
                Response::from_data(&[])
                    .with_status_code(StatusCode(307))
                    .with_header(
                        Header::from_bytes(
                            b"location",
                            page_url.as_os_str().as_encoded_bytes(),
                        )
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
                        .respond(
                            Response::from_string("404")
                                .with_status_code(StatusCode(404)),
                        )
                        .unwrap();
                    continue;
                };

                let data = if mime == XHTML || mime == HTML {
                    let content_styles = ContentStyles {
                        variables: state.css_variables,
                    }
                    .render()
                    .unwrap();
                    debug!("rendered content styles: {content_styles}");
                    let data = std::str::from_utf8(&data).unwrap();
                    inject_styles(&data, &content_styles).into_bytes()
                } else {
                    data
                };
                Response::from_data(data).with_header(
                    Header::from_bytes(b"Content-Type", mime.as_bytes())
                        .expect("no header?"),
                )
            }
            (&Method::Get, req_url) => {
                let req_url =
                    std::path::PathBuf::from(req_url.trim_start_matches('/'));
                let abs_url = if req_url.starts_with(&state.book.root_base) {
                    req_url
                } else {
                    state.book.root_base.join(req_url)
                };


                if let Some(idx) = state.book.resource_uri_to_chapter(&abs_url)
                {
                    if idx != state.current_page {
                        state.current_page = idx;
                        debug!(
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
                    let page_url =
                        std::path::PathBuf::from("/content").join(page_path);
                    let page_url = page_url.to_str().unwrap();

                    let stylesheet = ReaderStyles {
                        variables: state.css_variables,
                    }
                    .render()
                    .unwrap();
                    let rv = Reader {
                        title: &book_title,
                        stylesheet: &stylesheet,
                        reader_js: READER_JS,
                        page_url,

                        current_page: state.current_page + 1,
                        page_count: state.page_count,
                    };
                    Response::from_string(
                        rv.render().expect("thing inside thing"),
                    )
                    .with_header(
                        Header::from_bytes(b"Content-Type", XHTML)
                            .unwrap(),
                    )
                } else {
                    let (Some(data), Some(mime)) = (
                        state.book.get_resource_by_path(&abs_url),
                        state.book.get_resource_mime_by_path(&abs_url),
                    ) else {
                        request.respond(rcode(404)).unwrap();
                        continue;
                    };

                    Response::from_data(data).with_header(
                        Header::from_bytes(b"Content-Type", mime.as_bytes())
                            .expect("no header?"),
                    )
                }
            }
            _ => rcode(404),
        };

        respond(request, response);
    }
}

fn inject_styles(src: &str, stylesheet: &str) -> String {
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
    Response::from_string(status.to_string())
        .with_status_code(StatusCode(status))
}

fn respond<R: std::io::Read>(request: Request, response: Response<R>) {
    let request_url = request.url().to_string();

    debug!(
        "Responding to {request_url} wtih status: {}",
        response.status_code().0
    );
    if let Err(e) = request.respond(response) {
        error!("Failed to respond to {request_url}: {}", e);
    }
}
