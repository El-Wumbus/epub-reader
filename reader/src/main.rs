#![feature(duration_constructors)]
#![warn(clippy::pedantic)]

use epub::doc::EpubDoc;
use log::{debug, error, info, warn};
use rinja::Template;
use slime::parser::{UnParser as _, ini};
use std::io::{Cursor, Read, Write};
use std::process::exit;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tiny_http::{Header, Method, Request, Response, StatusCode};
pub const XHTML: &str = "application/xhtml+xml";
pub const HTML: &str = "text/html";
pub const JSON: &str = "application/json";
pub const CSS: &str = "text/css";
const READER_JS: &str = include_str!("reader.js");

/// Application state.
struct State<'a> {
    book: EpubDoc<Cursor<Vec<u8>>>,
    socket_addr: std::net::SocketAddr,
    css_variables: CSSVariables<'a>,
    current_page: usize,
    page_count: usize,
}

impl State<'_> {
    fn new(book: EpubDoc<Cursor<Vec<u8>>>) -> Self {
        let page_count = book.get_num_pages();
        Self {
            book,
            socket_addr: std::net::SocketAddr::new(
                std::net::Ipv4Addr::LOCALHOST.into(),
                0,
            ),
            css_variables: CSSVariables::default(),
            current_page: 0,
            page_count,
        }
    }

    /// Change the current page based on some predicate `pred`.
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
        let path = path.to_str().unwrap(); // TODO: Does epub-rs convert utf16 paths to utf8? Epubs
        // are garuenteed to be one of these.
        debug!(
            "Moved to page: {}/{} at path \"{}\"",
            self.current_page + 1,
            self.page_count,
            path
        );
        Ok(Response::from_string(path))
    }
}

#[derive(Debug, Clone)]
struct Config<'a> {
    open_in_browser: bool,
    kill_timeout: isize,
    bind_addr: &'a str,
    bind_port: u16,
    css_variables: CSSVariables<'a>,
}

impl Config<'_> {
    /// The number of INI fields when serialized.
    pub const S_FIELDS: usize = 9;
    pub const DEFAULT_BIND_ADDR: &'static str = "localhost";
    pub const DEFAULT_BIND_PORT: u16 = 0;
}

impl Default for Config<'_> {
    fn default() -> Self {
        Self {
            bind_addr: Self::DEFAULT_BIND_ADDR,
            bind_port: Self::DEFAULT_BIND_PORT,
            open_in_browser: false,
            kill_timeout: -1,
            css_variables: CSSVariables::default(),
        }
    }
}

impl<'a> From<&Config<'a>> for [ini::Pair<'a>; Config::S_FIELDS] {
    /// Create serializeable INI key-value pairs for [`Config`].
    fn from(cfg: &Config<'a>) -> Self {
        let css_variables: [ini::Pair<'a>; 5] = cfg.css_variables.into();
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
                key: "bind_addr",
                value: cfg.bind_addr,
            },
            ini::Pair {
                section: "",
                key: "bind_port",
                value: Box::leak(Box::new(cfg.bind_port.to_string())),
            },
            css_variables[0],
            css_variables[1],
            css_variables[2],
            css_variables[3],
            css_variables[4],
        ]
    }
}

impl<'a> TryFrom<ini::Parse<'a>> for Config<'a> {
    type Error = &'static str;

    /// Deserialize from a parsed INI.
    fn try_from(ini: ini::Parse<'a>) -> Result<Self, Self::Error> {
        let mut x = Self::default();
        for ini::Pair {
            section,
            key,
            value,
        } in ini
        {
            match (section, key) {
                ("", "bind_addr") => {
                    x.bind_addr = value;
                }
                ("", "bind_port") => {
                    x.bind_port =
                        value.parse().map_err(|_| "Invalid bind_port")?;
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
                ("css", "content_width") => {
                    x.css_variables.content_width = value
                        .parse()
                        .map_err(|_| "Invalid content_width")?;
                }
                ("css", "font") => x.css_variables.font = value,
                _ => {}
            }
        }

        Ok(x)
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

#[derive(Debug, Clone, Copy)]
struct CSSVariables<'a> {
    font: &'a str,
    fg_color: &'a str,
    bg_color: &'a str,
    content_width: f32,
    content_font_size_px: u32,
}

impl Default for CSSVariables<'_> {
    fn default() -> Self {
        Self {
            font: "'Iosevka', sans-serif",
            fg_color: "var(--color-primary-a50)",
            bg_color: "var(--color-surface-a0)",
            content_font_size_px: 21,
            content_width: 76.0,
        }
    }
}

impl<'a> From<CSSVariables<'a>> for [ini::Pair<'a>; 5] {
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
                key: "content_width",
                value: Box::leak(Box::new(vars.content_width.to_string())),
            },
            ini::Pair {
                section: "css",
                key: "font",
                value: Box::leak(Box::new(vars.font.to_string())),
            },
        ]
    }
}

/// Parse all the command-line flags and set the [`Config`] accordingly.
fn parse_args(config: &mut Config) -> Vec<String> {
    fn expect_next<V>(x: Option<V>) -> V {
        if let Some(v) = x {
            v
        } else {
            error!("FATAL: Expected a value for this flag but got nothing!");
            exit(1);
        }
    }

    let mut args = std::env::args().skip(1);
    let mut positional = vec![];
    let mut was_error = false;
    while let Some(arg) = args.next() {
        if arg.starts_with('-') {
            let arg = arg.to_lowercase();
            let arg = &arg[1..];
            match arg {
                "usage" => {
                    print_usage();
                    exit(0);
                }
                "open-in-browser" => {
                    let oib = expect_next(args.next());
                    match oib.parse::<bool>() {
                        Ok(b) => config.open_in_browser = b,
                        Err(e) => {
                            error!(
                                "FATAL: Invalid value for flag -{arg} \"{oib}\": {e}"
                            );
                            was_error = true;
                        }
                    }
                }
                "bind-addr" => {
                    config.bind_addr =
                        Box::leak(Box::new(expect_next(args.next())));
                }
                "bind-port" => {
                    let x = expect_next(args.next());
                    match x.parse() {
                        Ok(x) => config.bind_port = x,
                        Err(e) => {
                            error!(
                                "FATAL: Invalid value for flag -{arg} \"{x}\": {e}"
                            );
                            was_error = true;
                        }
                    }
                }
                "kill-timeout" => {
                    let kt = expect_next(args.next());
                    match kt.parse::<isize>() {
                        Ok(x) => config.kill_timeout = x,
                        Err(e) => {
                            error!(
                                "FATAL: Invalid value for flag -{arg} \"{kt}\": {e}"
                            );
                            was_error = true;
                        }
                    }
                }
                unrecognized_flag => {
                    was_error = true;
                    if unrecognized_flag.starts_with('-') {
                        let f = unrecognized_flag.trim_start_matches('-');
                        error!(
                            "FATAL: Unrecognized flag \"-{arg}\"! I expect flags with a single \"-\", did you mean \"-{f}\"?"
                        );
                    } else {
                        error!("FATAL: Unrecognized flag \"-{arg}\"!");
                    }
                }
            }
        } else {
            positional.push(arg);
        }
    }

    if was_error {
        print_usage();
        exit(1);
    }
    positional
}

fn print_usage() {
    let program_name = std::env::args()
        .nth(0)
        .unwrap_or_else(|| String::from("epub-reader"));
    let program_name = std::path::PathBuf::from(program_name);
    let program_name = program_name
        .file_name()
        .unwrap()
        .to_str()
        .expect("We made this from a utf8 string");
    println!("Usage: {program_name} [flags] <epub>
    -usage              Display this message
    -open-in-browser    Opens the the bind url in the default application (web browser)
                        default: false
    -bind-addr          Set the bind address
                        default: '{default_bind_addr}:bind_port'
    -bind-port          Set the bind port
                        default: '{default_bind_port}'
    -kill-timeout       Set the inactivity timeout, after which the server quits.
                        default: -1 (disabled).",
                        default_bind_addr = Config::DEFAULT_BIND_ADDR,
                        default_bind_port = Config::DEFAULT_BIND_PORT);
}

fn main() -> Result<(), ()> {
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
    fn response_invalid_utf8() -> Response<Cursor<Vec<u8>>> {
        Response::from_string("400\nInvalid request body: Expected valid UTF-8")
            .with_status_code(StatusCode(400))
    }
    fn rcode(status: u16) -> Response<Cursor<Vec<u8>>> {
        Response::from_string(status.to_string())
            .with_status_code(StatusCode(status))
    }

    env_logger::Builder::from_env("READER_LOG")
        .filter_level(log::LevelFilter::Info)
        .write_style(env_logger::fmt::WriteStyle::Always)
        .init();

    let config = Config::default();
    let config_home = match slime::xdg::Dirs::config_home_dir() {
        Some(h) => h,
        None => {
            error!("FATAL: Couldn't find config directory!");
            exit(1);
        }
    };
    let config_file = config_home.join("epub-reader").join("config.ini");
    debug!("Using \"{}\" as config file", config_file.display());

    let mut config: Config = if config_file.exists() {
        match std::fs::read_to_string(&config_file) {
            Ok(contents) => {
                let contents = Box::leak(Box::new(contents));
                match ini::Parse::from(contents.as_str()).try_into() {
                    Ok(c) => c,
                    Err(e) => {
                        error!("FATAL: Invalid configuration file: {e}");
                        exit(1);
                    }
                }
            }
            Err(e) => {
                error!(
                    "Couldn't read configuration file \"{}\": {e}. Using default configurations.",
                    config_file.display()
                );
                config
            }
        }
    } else {
        let mut c = true;
        let cs: [ini::Pair; Config::S_FIELDS] = (&config).into();
        let contents = cs
            .into_iter()
            .serialize_to_bytes()
            .expect("This valid config shouldn't fail to serialize");
        if let Some(parent) = config_file.parent() {
            if !parent.exists() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    c = false;
                    error!(
                        "Couldn't create configuration file's missing parent directory: \"{}\": {e}",
                        parent.display()
                    );
                }
            }
        }

        if c {
            if let Err(e) = std::fs::File::create(&config_file)
                .and_then(|mut f| f.write_all(&contents))
            {
                error!(
                    "Failed to save config to file \"{}\": {e}",
                    config_file.display()
                );
            }
        }
        config
    };

    let mut positionals = parse_args(&mut config).into_iter();
    let Some(book_arg) = positionals.next() else {
        error!(
            "FATAL: Expected an EPUB file to be provided as the first positional argument"
        );
        print_usage();
        exit(1);
    };

    let book = match std::fs::File::open(&book_arg).and_then(|mut f| {
        let mut bookbuf = vec![];
        f.read_to_end(&mut bookbuf)?;
        Ok(bookbuf)
    }) {
        Ok(book) => match EpubDoc::from_reader(Cursor::new(book)) {
            Ok(b) => b,
            Err(e) => {
                error!(
                    "FATAL: Failed to read provided file \"{book_arg}\": {e}"
                );
                exit(1);
            }
        },
        Err(e) => {
            error!(
                "FATAL: Failed to open provided book file \"{book_arg}\": {e}"
            );
            exit(1);
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
    let server =
        match tiny_http::Server::http((config.bind_addr, config.bind_port)) {
            Ok(s) => s,
            Err(e) => {
                error!(
                    "FATAL: failed to bind to {}:{}: {e}!",
                    config.bind_addr, config.bind_port
                );
                exit(1);
            }
        };
    state.socket_addr = server.server_addr().to_ip().unwrap();
    info!(
        "Bound server to {}:{}",
        config.bind_addr,
        state.socket_addr.port()
    );

    // Create a "watchdog" thread to kill the server after some time of
    // inactivity. TODO: This could be reimplemented to use a single thread
    // by not blocking the server thread and instead sleeping
    // between requests.
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
                        exit(0);
                    }
                    std::thread::sleep(Duration::from_secs(2));
                }
            })
        })
    };

    if config.open_in_browser {
        let cmd = std::process::Command::new("xdg-open")
            .arg(format!(
                "http://{}:{}",
                config.bind_addr,
                state.socket_addr.port()
            ))
            .output();
        match cmd {
            Ok(output) if output.status.success() => {}
            Ok(output) => error!(
                "Failed to open in browser: xdg-open: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
            Err(e) => {
                error!("Failed to open in browser: failed to spawn child: {e}");
            }
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
                error!("FATAL: {e}");
                exit(1);
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
            ("/api/keepalive", &Method::Post) => {
                debug!("Got keepalive signal from client");
                rcode(200)
            }
            ("/api/page", &Method::Post) => {
                let mut req_body = String::new();
                // TODO: Stop unwrapping and error handle properly
                if let Err(_e) =
                    request.as_reader().read_to_string(&mut req_body)
                {
                    respond(request, response_invalid_utf8());
                    continue;
                }

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
                if let Err(_e) =
                    request.as_reader().read_to_string(&mut req_body)
                {
                    respond(request, response_invalid_utf8());
                    continue;
                }
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
                if let Err(_e) =
                    request.as_reader().read_to_string(&mut req_body)
                {
                    respond(request, response_invalid_utf8());
                    continue;
                }

                match req_body.trim() {
                    "+" => {
                        state.css_variables.content_width += 1.0;
                        debug!(
                            "Increased content width to {}",
                            state.css_variables.content_width
                        );
                        rcode(200)
                    }
                    "-" => {
                        if state.css_variables.content_width > 20.0 {
                            state.css_variables.content_width -= 1.0;
                        }
                        debug!(
                            "Increased content width to {}",
                            state.css_variables.content_width
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

/// Add the `stylesheet` to the end of the XHTML Header found in `src`. This
/// does nothing if `src` doesn't have an HTML header.
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
            Err(e) => {
                error!("FATAL: XML parsing error: {e}");
                exit(1);
            }
        }
    }
    output
}
