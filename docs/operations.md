# Operations

## Environment

Example `.env`:

```env
APP_USERNAME=admin
APP_PASSWORD=change-me
BIND_ADDRESS=0.0.0.0:8080
FILE_ROOT=/var/lib/receiver
```

## Running

```bash
./receiver
```

Open `http://server:8080`.

## Public Deployment

Receiver serves HTTP. If it is exposed publicly, use a reverse proxy for HTTPS and request-size policy.

Chunk upload reduces per-request size, but the proxy must still allow the selected chunk size.
