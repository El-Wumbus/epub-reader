window.addEventListener("popstate", function (event) {
    console.log(event.state);
});

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
                        document.getElementById("pageframe").src = text;
                    });
                });
                break;

            case "ArrowRight":
                fetch("/next-page", {
                    method: "POST",
                    body: "",
                }).then((response) => {
                    response.text().then((text) => {
                        document.getElementById("pageframe").src = text;
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
