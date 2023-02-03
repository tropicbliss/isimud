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

### Environment variables

`PASSWORD`: Only establishes a connection if the publisher connects with the same password. This is unencrypted data and could be potentially dangerous depending on your threat model.

`IP` (optional): `127.0.0.1` by default.

`PORT` (optional): `3000` by default.

`HOMEPAGE` (optional): Redirects `/` to this GitHub page if `true` (enabled by default)
