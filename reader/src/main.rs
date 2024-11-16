#![feature(duration_constructors)]
#![warn(clippy::pedantic)]

use epub::doc::EpubDoc;
use log::{debug, error, info, warn};
use rinja::Template;
use slime::parser::{UnParser as _, ini};
use std::io::{Cursor, Read};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tiny_http::{Header, Method, Request, Response, StatusCode};
//use util::*;
//mod util;
pub const XHTML: &str = "application/xhtml+xml";
pub const HTML: &str = "text/html";
pub const JSON: &str = "application/json";
pub const CSS: &str = "text/css";
const READER_JS: &str = include_str!("reader.js");

struct State<'a> {
    book: EpubDoc<Cursor<Vec<u8>>>,
    css_variables: CSSVariables<'a>,
    current_page: usize,
    page_count: usize,
}

impl State<'_> {
    fn new(book: EpubDoc<Cursor<Vec<u8>>>) -> Self {
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
fn parse_args(config: &mut Config) -> Vec<String> {
    fn expect_next<V>(x: Option<V>) -> V {
        if let Some(v) = x {
            v
        } else {
            error!("Expected a value for this flag but got nothing!");
            std::process::exit(1);
        }
    }

    let mut args = std::env::args().skip(1);
    let mut positional = vec![];
    while let Some(arg) = args.next() {
        if arg.starts_with('-') {
            let arg = arg.to_lowercase();
            let arg = &arg[1..];
            match arg {
                "open-in-browser" => {
                    config.open_in_browser = true;
                }
                "bind" => {
                    config.bind = Box::leak(Box::new(expect_next(args.next())));
                }
                "kill-timeout" => {
                    let kt = expect_next(args.next());
                    match kt.parse::<isize>() {
                        Ok(x) => config.kill_timeout = x,
                        Err(e) => {
                            error!("Invalid value \"{kt}\": {e}");
                            std::process::exit(1);
                        }
                    }
                }
                unrecognized_flag => {
                    if unrecognized_flag.starts_with('-') {
                        let f = unrecognized_flag.trim_start_matches('-');
                        warn!(
                            "Unrecognized flag \"-{arg}\"! I expect flags with a single \"-\", did you mean \"-{f}\"?"
                        );
                    } else {
                        warn!("Unrecognized flag \"-{arg}\"!");
                    }
                }
            }
        } else {
            positional.push(arg);
        }
    }
    positional
}

fn main() -> Result<(), ()> {
    env_logger::Builder::from_env("READER_LOG")
        .filter_level(log::LevelFilter::Info)
        .write_style(env_logger::fmt::WriteStyle::Always)
        .init();

    let config = Config::default();
    let config_home = slime::xdg::Dirs::config_home_dir().expect("home dir");
    let config_file = config_home.join("epub-reader").join("config.ini");

    debug!("Using \"{}\" as config file", config_file.display());
    let mut config: Config = if config_file.exists() {
        let config_s = std::fs::read_to_string(&config_file).unwrap();
        let config_s = Box::leak(Box::new(config_s));
        let i = ini::Parse::from(config_s.as_str());
        i.try_into().expect("valid config")
    } else {
        if let Some(parent) = config_file.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).expect("create parent dir");
            }
        }
        let mut config_f = std::fs::File::create(&config_file).unwrap();
        let ting: [ini::Pair; Config::S_FIELDS] = (&config).into();
        ting.into_iter()
            .serialize(&mut config_f)
            .expect("good time writing to file");
        config
    };

    let mut positionals = parse_args(&mut config).into_iter();
    let Some(book) = positionals.next() else {
        error!(
            "Expected an EPUB file to be provided as the first positional argument"
        );
        return Err(());
    };

    // TODO: report errors instead of unrapping.
    let mut book = std::fs::File::open(book).expect("File to open properly");
    let mut book_buffer = Vec::new();
    book.read_to_end(&mut book_buffer).expect("readable file");
    let book = match EpubDoc::from_reader(Cursor::new(book_buffer)) {
        Ok(b) => b,
        Err(e) => {
            error!("Invalid EPUB archive: {e}");
            return Err(());
        }
    };
    debug!(
        "Metadata: {:#?}\nSpine: {:#?}\nResources = {:#?}\n",
        book.metadata, book.spine, book.resources
    );

    let book_title = book
        .metadata
        .get("title")
        .into_iter()
        .flatten()
        .map(String::as_str)
        .next()
        .unwrap_or("Missing Title")
        .to_string();

    let mut state = State::new(book);
    state.css_variables = config.css_variables;

    let server = tiny_http::Server::http(config.bind).unwrap();

    let (wd_tx, wd_rx) = mpsc::channel();
    let _watchdog = if config.kill_timeout < 0 {
        None
    } else {
        let kill_timeout = config.kill_timeout as usize;
        Some({
            std::thread::spawn(move || {
                let timeout_duration = Duration::from_mins(kill_timeout as u64);
                let mut last_request = Instant::now();
                loop {
                    if let Ok(last) = wd_rx.try_recv() {
                        last_request = last;
                    }
                    let elapsed = last_request.elapsed();
                    if elapsed > timeout_duration {
                        std::process::exit(1);
                    }
                    std::thread::sleep(Duration::from_secs(2));
                }
            })
        })
    };

    if config.open_in_browser {
        // TODO: replace with library to elimiate dependency on xdg-open/xdg-utils.
        if let Err(e) = std::process::Command::new("xdg-open")
            .arg(format!("http://{}", config.bind))
            .output()
        {
            error!("Could't open browser: {e}");
        }
    }
    let mut quit = false;

    while !quit {
        let mut request = match server.recv() {
            Ok(rq) => {
                wd_tx.send(Instant::now()).unwrap();
                rq
            }
            Err(e) => {
                error!("server: {}", e);
                std::process::exit(1);
            }
        };

        let request_url = request.url().to_string();
        debug!(
            "Recevied request: method = {}, url = {}",
            request.method(),
            request_url
        );

        let response = match (request_url.as_str(), request.method()) {
            ("/", &Method::Get) | ("/reader", &Method::Get) => {
                // Redirect to the current page
                assert!(state.book.set_current_page(state.current_page));
                let page_url = state.book.get_current_path().unwrap();
                Response::from_data([])
                    .with_status_code(StatusCode(307))
                    .with_header(
                        Header::from_bytes(
                            b"location",
                            page_url.as_os_str().as_encoded_bytes(),
                        )
                        .unwrap(),
                    )
            }
            ("/api/quit", &Method::Post) => {
                info!("Quitting at the request of the client");
                quit = true;
                rcode(200)
            }
            ("/api/page", &Method::Post) => {
                let mut req_body = String::new();
                // TODO: Stop unwrapping and error handle properly
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
                        Err(()) => rcode(500),
                    },
                    "-" => {
                        match state.change_page(|page, _| {
                            if page != 0 { page - 1 } else { page }
                        }) {
                            Ok(r) => r,
                            Err(()) => rcode(500),
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
            ("/api/font-size", &Method::Post) => {
                let mut req_body = String::new();
                // TODO: Stop unwrapping and error handle properly
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
            ("/api/invert-text-color", &Method::Post) => {
                std::mem::swap(
                    &mut state.css_variables.fg_color,
                    &mut state.css_variables.bg_color,
                );
                debug!("Inverted content styles: {:?}", state.css_variables);
                debug!("Inverted text color");
                rcode(200)
            }
            ("/api/content-width", &Method::Post) => {
                let mut req_body = String::new();
                // TODO: Stop unwrapping and error handle properly
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
            (content, &Method::Get) if content.starts_with("/content/") => {
                let content = content.strip_prefix("/content/").unwrap();
                let (Some(data), Some(mime)) = (
                    state.book.get_resource_by_path(content),
                    state.book.get_resource_mime_by_path(content),
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
                    inject_styles(data, &content_styles).into_bytes()
                } else {
                    data
                };
                Response::from_data(data).with_header(
                    Header::from_bytes(b"Content-Type", mime.as_bytes())
                        .expect("no header?"),
                )
            }
            (req_url, &Method::Get) => {
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
                        javascript: READER_JS,
                        page_url,

                        current_page: state.current_page + 1,
                        page_count: state.page_count,
                    };
                    Response::from_string(
                        rv.render().expect("thing inside thing"),
                    )
                    .with_header(
                        Header::from_bytes(b"Content-Type", XHTML).unwrap(),
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

    Ok(())
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

#[derive(Debug, Template)]
#[template(ext = "xhtml", path = "reader.xml")]
struct Reader<'a> {
    title: &'a str,
    stylesheet: &'a str,
    javascript: &'a str,
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

impl Default for CSSVariables<'_> {
    fn default() -> Self {
        Self {
            fg_color: "var(--color-primary-a50)",
            bg_color: "var(--color-surface-a0)",
            content_font_size_px: 21,
            content_width_em: 36.0,
        }
    }
}

impl<'a> From<CSSVariables<'a>> for [ini::Pair<'a>; 4] {
    fn from(vars: CSSVariables<'a>) -> Self {
        [
            ini::Pair {
                section: "css",
                key: "fg_color",
                value: vars.fg_color,
            },
            ini::Pair {
                section: "css",
                key: "bg_color",
                value: vars.bg_color,
            },
            ini::Pair {
                section: "css",
                key: "content_font_size_px",
                value: Box::leak(Box::new(
                    vars.content_font_size_px.to_string(),
                )),
            },
            ini::Pair {
                section: "css",
                key: "content_width_em",
                value: Box::leak(Box::new(vars.content_width_em.to_string())),
            },
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

#[derive(Debug, Clone)]
struct Config<'a> {
    open_in_browser: bool,
    kill_timeout: isize,
    bind: &'a str,
    css_variables: CSSVariables<'a>,
}

impl Config<'_> {
    pub const S_FIELDS: usize = 7;
}

impl Default for Config<'_> {
    fn default() -> Self {
        Self {
            bind: "localhost:6969",
            open_in_browser: true,
            kill_timeout: -1,
            css_variables: CSSVariables::default(),
        }
    }
}

impl<'a> From<&Config<'a>> for [ini::Pair<'a>; Config::S_FIELDS] {
    fn from(cfg: &Config<'a>) -> Self {
        let css_variables: [ini::Pair<'a>; 4] = cfg.css_variables.into();
        [
            ini::Pair {
                section: "",
                key: "open_in_browser",
                value: if cfg.open_in_browser { "true" } else { "false" },
            },
            ini::Pair {
                section: "",
                key: "kill_timeout",
                value: Box::leak(Box::new(cfg.kill_timeout.to_string())),
            },
            ini::Pair {
                section: "",
                key: "bind",
                value: cfg.bind,
            },
            css_variables[0],
            css_variables[1],
            css_variables[2],
            css_variables[3],
        ]
    }
}

impl<'a> TryFrom<ini::Parse<'a>> for Config<'a> {
    type Error = &'static str;

    fn try_from(ini: ini::Parse<'a>) -> Result<Self, Self::Error> {
        let mut x = Self::default();
        for ini::Pair {
            section,
            key,
            value,
        } in ini
        {
            match (section, key) {
                ("", "bind") => {
                    x.bind = value;
                }
                ("", "open_in_browser") => {
                    x.open_in_browser = value.parse::<bool>().map_err(
                        |_| "Invalid boolean value for 'open_in_browser'",
                    )?;
                }
                ("css", "fg_color") => x.css_variables.fg_color = value,
                ("css", "bg_color") => x.css_variables.bg_color = value,
                ("css", "content_font_size_px") => {
                    x.css_variables.content_font_size_px = value
                        .parse()
                        .map_err(|_| "Invalid content_font_size_px")?;
                }
                ("css", "content_width_em") => {
                    x.css_variables.content_width_em = value
                        .parse()
                        .map_err(|_| "Invalid content_width_em")?;
                }
                _ => {}
            }
        }

        Ok(x)
    }
}
