# isimud

An in-memory pub/sub server.

## Usage

Take note that the socket connection can spontaneously close if the server does not like what you are sending.

It is good courtesy to close a client's websocket connection to the server when not in use.

### Publisher

1. Send the following JSON via a POST request to `/pub`:

```json
{
    "topic": <topic_name>,
    "data": <data_to_send_to_subscribers>
}
```

with the [`Authorization: Basic ...`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Authorization#basic) header. It is absolutely essential that the server is proxied behind a HTTPS reverse proxy (e.g. [Caddy](https://caddyserver.com/)), etc. for this very reason. The password field should match the password given in the environment variable, and the username represents the `publisher` name each subscriber is going to be subscribed to. The `topic` and `publisher` are case-sensitive.

### Subscriber

1. Send the following JSON as text via websocket to `/sub`:

```json
{
    "publisher": <pub_name_to_receive_messages_from>,
    "topic": <topic_to_subscribe_to>
}
```

2. Be ready to receive publisher `data` as text.

#### Authorization

Sometimes, you might want to allow only trusted sources to connect to your server, to prevent unauthorized sources from hogging your websocket connections. Adding `AUTH_URL` as an environment variable allows the server to contact an authorization server at that URL to query whether the connecting user is authorized to connect. This server sends a GET request with an empty body, and if the status code returned by the authorization server is within 200-299, this server allows the client to connect to the `/sub` endpoint. This requires the client to connect to the `/sub` endpoint via an [`Authorization: Bearer <...>`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Authorization) header, which then sends the same header to the authorization server. This can result in an Internal Server Error (500) if the HTTP connection to the authorization server timeouts within 5 seconds.

### Environment variables

`PASSWORD`: Only establishes a connection if the publisher connects with the same password. This is unencrypted data and could be potentially dangerous depending on your threat model.

`IP` (optional): `127.0.0.1` by default.

`PORT` (optional): `3000` by default.

`HOMEPAGE` (optional): Redirects `/` to this GitHub page if `true` (enabled by default)

`AUTH_URL` (optional): Authorization URL for the `/sub` endpoint (subscriber authorization disabled by default)
