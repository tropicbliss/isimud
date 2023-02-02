# isimud

An in-memory pub/sub server.

## Usage

Communicating with this server is done through websockets at the websocket URL path (`/ws`).

Take note that the socket connection can spontaneously close if the server does not like what you are sending.

### Publisher

1. `pub auth <password>`
2. `pub name <name>` (give yourself a name)
3. Send the following JSON as text:

```json
{
    "topic": <topic_name>,
    "data": <data_to_send_to_subscribers>
}
```

### Subscriber

1. Send the following JSON as text:

```json
{
    "publisher": <pub_name_to_receive_messages_from>,
    "topic": <topic_to_subscribe_to>
}
```

2. Be ready to receive publisher `data` as text.

### Environment variables

`PASSWORD`: Only establishes a connection if the publisher connects with the same password.

`IP` (optional): `127.0.0.1` by default.

`PORT` (optional): `3000` by default.

`HOMEPAGE` (optional): Redirects `/` to this GitHub page if `true` (enabled by default)
