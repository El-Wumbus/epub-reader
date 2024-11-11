const frame = document.getElementById("pageframe");

window.addEventListener(
    "keydown",
    (event) => {
        if (event.defaultPrevented) {
            return; // Do nothing if event already handled
        }
        console.log(event);
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
            default:
                return;
        }
        event.preventDefault();
    },
    true,
);

frame.addEventListener("load", () => {
    // Replace every link with a corrected version of it.
    for (const link of frame.contentDocument.links) {
        if (link.href.includes("content/")) {
            const betterlink = link.href.replace("content/", "");
            link.href = betterlink;
            link.target = "_top" // HTML is stupid and links don't work unless we do this.
        }
    }
});
