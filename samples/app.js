// app.js

function getUser() {
  // FIXME: This endpoint is deprecated, use /v2/user instead
  fetch('/api/user')
    .then(res => res.json())
    .then(data => console.log(data));
}
