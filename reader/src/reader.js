const API = {
    PAGE: "/api/page",
    FONT_SIZE: "/api/font-size",
    INVERT_TEXT_COLOR: "/api/invert-text-color",
    CONTENT_WIDTH: "/api/content-width",
};

async function api_page(action) {
    const response = await fetch(API.PAGE, {
        method: "POST",
        body: action,
    });
    const text = await response.text();
    return text;
}

async function api_font_size(action) {
    const response = await fetch(API.FONT_SIZE, {
        method: "POST",
        body: action,
    });
    if (response.ok) {
        console.log("Adjusted font size");
    }
}

async function api_invert_text_color() {
    const response = await fetch(API.INVERT_TEXT_COLOR, { method: "POST" });
    if (response.ok) {
        console.log("Inverted text color");
    }
}

async function api_content_width(action) {
    const response = await fetch(API.CONTENT_WIDTH, {
        method: "POST",
        body: action,
    });
    if (response.ok) {
        console.log("Adjusted content width");
    }
}

// This is called by the reader.xml
async function navigate_to_page() {
    const page = await api_page(document.getElementById("pageinput").value);
    location.href = location.origin + "/" + page;
}

async function keybinds(key) {
    switch (key) {
        case "ArrowLeft":
            location.href = "/" + (await api_page("-"));
            break;
        case "ArrowRight":
            location.href = "/" + (await api_page("+"));
            break;
        case "=":
            await api_font_size("+");
            location.reload();
            break;
        case "-":
            await api_font_size("-");
            location.reload();
            break;
        case "!":
            await api_invert_text_color();
            location.reload();
            break;
        case "[":
            await api_content_width("-");
            location.reload();
            break;
        case "]":
            await api_content_width("+");
            location.reload();
            break;
        default:
            return;
    }
}

const frame = document.getElementById("pageframe");

window.addEventListener("keydown", (event) => {
    if (event.defaultPrevented) {
        return;
    }
    keybinds(event.key);
});

frame.addEventListener("keydown", (event) => {
    if (event.defaultPrevented) {
        return;
    }
    keybinds(event.key);
});
//TODO: move this to the server
frame.addEventListener("load", () => {
    // Replace every link with a corrected version of it.
    for (const link of frame.contentDocument.links) {
        if (link.href.includes("content/")) {
            const betterlink = link.href.replace("content/", "");
            link.href = betterlink;
            link.target = "_top"; // HTML is stupid and links don't work unless we do this.
        }
    }
});

/*window.addEventListener("load", () => {
    // Load preferences from cookies;
});*/

