const cookies = document.cookie;
const cookie_content_font_size = get_cookie(cookies, "content_font_size");
let content_font_size = (cookie_content_font_size) ? parseInt(cookie_content_font_size) : 16;

const CSS_CONTENT_FONT_SIZE = "--content-font-size";
const frame = document.getElementById("pageframe");
let frame_style_root = undefined;

function get_cookie(cookies, name) {
    const dcookies = decodeURIComponent(cookies);
    const look_for = name + "=";
    for (let cookie of dcookies.split(';')) {
        cookie = cookie.trim();
        if (cookie.indexOf(look_for) == 0) {
            return cookie.substring(look_for.length, cookie.length);
        }
    }
    return null;
}

window.addEventListener("load", () => {
    // Load preferences from cookies;
    const cookies = document.cookie;
    console.log(cookies);
    const cookie_content_font_size = get_cookie(cookies, "content_font_size");
    if (cookie_content_font_size) {
        content_font_size = parseInt(cookie_content_font_size);
        console.log("got font size:", content_font_size);
    }
    
});

function set_font_size(sz) {
    document.cookie = "content_font_size" + '=' + sz;
    frame_style_root.style.setProperty(CSS_CONTENT_FONT_SIZE, sz + "px");
    console.log("Set font size:", sz);
}

frame.addEventListener("load", () => {
    frame_style_root = frame.contentDocument.querySelector(':root');
    //set_font_size(content_font_size);
    
    // Replace every link with a corrected version of it.
    for (const link of frame.contentDocument.links) {
        if (link.href.includes("content/")) {
            const betterlink = link.href.replace("content/", "");
            link.href = betterlink;
            link.target = "_top" // HTML is stupid and links don't work unless we do this.
        }
    }
});



window.addEventListener(
    "keydown",
    (event) => {
        if (event.defaultPrevented) {
            return; // Do nothing if event already handled
        }
        console.log("Key pressed:", event.key);
        switch (event.key) {
            case "ArrowLeft":
                fetch("/prev-page", {
                    method: "POST",
                    body: "",
                }).then((response) => {
                    response.text().then((text) => {
                        console.log("Navigating to the previous page");
                        window.location.href = "/" + text;
                    });
                });
                break;

            case "ArrowRight":
                fetch("/next-page", {
                    method: "POST",
                    body: "",
                }).then((response) => {
                    response.text().then((text) => {
                        console.log("Navigating to the next page");
                        window.location.href = "/" + text;
                    });
                });
                break;
            case "=":
                content_font_size += 2;
                set_font_size(content_font_size);
                break;
            case "-":
                if (content_font_size -2 > 1) {
                    content_font_size -= 2;
                }
                set_font_size(content_font_size);
                break;
            default:
                return;
        }
        //event.preventDefault();
    },
    // true
);

