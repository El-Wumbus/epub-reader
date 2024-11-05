It appears as though the [epub](https://en.wikipedia.org/wiki/EPUB) format is just a zip file containing XHTML web pages and metadata.
I think program could be made to simply serve these container files to the browser.
This program would be desgined to serve to localhost and proabably only a single file.
There would be client side javascript to provide better UI and interactivity.
This would be delivered by the afformentioned server.
This method reduces code repitition and bloat.
The work has already been done to render HTML, and that isn't my goal, so it's a waste of time to focus on that.
To embed a browser in a destktop application is stupid when the user already has a web browser.


# TODO

- On the client (`src/index.js`) we could hijack relative links so that we
  embed the target page instead. This way we keep are controls.
