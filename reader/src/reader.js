const API = {
    NEXT_PAGE: "/api/next-page",
    PREV_PAGE: "/api/prev-page",
    CURRENT_PAGE: "/api/current-page",
    INCREASE_FONT_SIZE: "/api/increase-font-size",
    DECREASE_FONT_SIZE: "/api/decrease-font-size",
    INVERT_TEXT_COLOR: "/api/invert-text-color",
};

async function api_next_page() {
    const response = await fetch(API.NEXT_PAGE, { method: "POST" });
    const text = await response.text();
    console.log("Moving to next page: ", text);
    return text;
}

async function api_prev_page() {
    const response = await fetch(API.PREV_PAGE, { method: "POST" });
    const text = await response.text();
    console.log("Moving to previous page: ", text);
    return text;
}

async function api_current_page(page) {
    const response = await fetch(API.CURRENT_PAGE, {
        method: "POST",
        body: page,
    });
    const text = await response.text();
    return text;
}

async function api_increase_font_size() {
    const response = await fetch(API.INCREASE_FONT_SIZE, { method: "POST" });
    if (response.ok) {
        console.log("Increased font size");
    }
}

async function api_decrease_font_size() {
    const response = await fetch(API.DECREASE_FONT_SIZE, { method: "POST" });
    if (response.ok) {
        console.log("Decreased font size");
    }
}

async function api_invert_text_color() {
    const response = await fetch(API.INVERT_TEXT_COLOR, { method: "POST" });
    if (response.ok) {
        console.log("Inverted text color");
    }
}

// This is called by the reader.xml
async function navigate_to_page() {
    const page = await api_current_page(document.getElementById("pageinput").value);
    location.href = location.origin + "/" + page;
}

async function keybinds(key) {
    switch (key) {
        case "ArrowLeft":
            location.href = "/" + (await api_prev_page());
            break;
        case "ArrowRight":
            location.href = "/" + (await api_next_page());
            break;
        case "=":
            await api_increase_font_size();
            location.reload();
            break;
        case "-":
            await api_decrease_font_size();
            location.reload();
            break;
        case "!":
            await api_invert_text_color();
            location.reload();
            break;
        default:
            return;
    }
}

window.addEventListener(
    "keydown",
    (event) => {
        if (event.defaultPrevented) {
            return;
        }
        keybinds(event.key);
    },
);

//TODO: move this to the server
const frame = document.getElementById("pageframe");
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
