# Serve Maio over HTTPS

Use this when Maio is reached from outside your own machine. Maio speaks plain HTTP on port 8080; a reverse proxy in front of it adds HTTPS.

1. Put a reverse proxy in front of port 8080. With [Caddy](https://caddyserver.com), which gets a certificate by itself, a whole `Caddyfile` is:

   ```text
   music.example {
       reverse_proxy localhost:8080
   }
   ```

   The proxy must not buffer responses or time them out: live streams and the browser's event stream stay open for hours.
2. In `maio.toml`, mark the session cookie secure and tell Maio the address people use, since sign-in and changes check it:

   ```toml
   secure_cookies = true
   public_origin = "https://music.example"
   ```

3. Restart Maio: `docker compose up -d --force-recreate maio`.
4. Sign in at `https://music.example`. If sign-in is refused with a bad origin, `public_origin` does not match the address in the browser.

RTSP, SRT and RTMP go to MediaMTX directly and are not encrypted by the proxy. How sign-in and sessions are protected: [arc42 8.1](../architecture/08-crosscutting-concepts.md#81-signing-in-and-security).
