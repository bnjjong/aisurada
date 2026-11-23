use super::{http_error, json_body, Result, NETWORK, RECONNECT, RESPONSE};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ring::{
    digest,
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{Ipv4Addr, TcpListener},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Url};
use tauri_plugin_opener::OpenerExt;

const ISSUER: &str = "https://api.archyl.com";
const READ: &str = "mcp:read";
const WRITE: &str = "mcp:write";

#[derive(Serialize, Deserialize)]
pub struct Client {
    pub client_id: String,
    client_secret: String,
}

#[derive(Serialize, Deserialize)]
pub struct Session {
    pub client: Client,
    pub access_token: String,
    refresh_token: Option<String>,
    scope: String,
    expires_at: Option<u64>,
}

#[derive(Deserialize)]
struct TokenReply {
    access_token: String,
    refresh_token: Option<String>,
    scope: Option<String>,
    expires_in: Option<u64>,
}

impl Session {
    pub fn writable(&self) -> bool {
        self.scope.split_whitespace().any(|scope| scope == WRITE)
    }

    pub fn expiring(&self) -> bool {
        self.expires_at
            .is_some_and(|expiry| expiry <= now().saturating_add(30))
    }

    pub fn refresh(&mut self, agent: &ureq::Agent) -> Result<()> {
        let refresh = self.refresh_token.as_deref().ok_or(RECONNECT)?;
        let reply = token(
            agent,
            &self.client,
            [
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh),
                ("resource", ISSUER),
            ],
        )?;
        let scope = checked_scope(reply.scope.as_deref().unwrap_or(&self.scope))?;
        self.access_token = reply.access_token;
        self.refresh_token = reply.refresh_token.or(self.refresh_token.take());
        self.scope = scope;
        self.expires_at = reply
            .expires_in
            .map(|seconds| now().saturating_add(seconds));
        Ok(())
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn random() -> Result<String> {
    let mut bytes = [0; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| "로그인 요청을 준비하지 못했어요.")?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn checked_scope(scope: &str) -> Result<String> {
    if !scope.split_whitespace().any(|scope| scope == READ) {
        return Err("Archyl에서 조회 권한을 받지 못했어요.".into());
    }
    Ok(scope.to_owned())
}

fn token<const N: usize>(
    agent: &ureq::Agent,
    client: &Client,
    grant: [(&str, &str); N],
) -> Result<TokenReply> {
    let response = agent
        .post(format!("{ISSUER}/oauth/token"))
        .send_form(
            [
                ("client_id", client.client_id.as_str()),
                ("client_secret", client.client_secret.as_str()),
            ]
            .into_iter()
            .chain(grant),
        )
        .map_err(|_| NETWORK)?;
    if !response.status().is_success() {
        return Err(http_error(response, RECONNECT));
    }
    let reply: TokenReply = json_body(response)?;
    if reply.access_token.trim().is_empty() {
        return Err(RESPONSE.into());
    }
    Ok(reply)
}

pub fn login(
    app: &AppHandle,
    agent: &ureq::Agent,
    read_only: bool,
    generation: &AtomicU64,
    attempt: u64,
) -> Result<Session> {
    let scope = if read_only {
        READ
    } else {
        "mcp:read mcp:write"
    };
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .map_err(|_| "로그인 응답을 받을 포트를 열지 못했어요.")?;
    listener.set_nonblocking(true).map_err(|_| RESPONSE)?;
    let port = listener.local_addr().map_err(|_| RESPONSE)?.port();
    let redirect = format!("http://127.0.0.1:{port}/callback");
    let response = agent
        .post(format!("{ISSUER}/register"))
        .content_type("application/json")
        .send(
            json!({
                "client_name": "Aisurada",
                "redirect_uris": [redirect],
                "grant_types": ["authorization_code", "refresh_token"],
                "response_types": ["code"],
                "token_endpoint_auth_method": "client_secret_post",
                "scope": scope,
            })
            .to_string(),
        )
        .map_err(|_| NETWORK)?;
    if !response.status().is_success() {
        return Err(http_error(
            response,
            "Archyl에서 로그인 요청을 등록하지 못했어요.",
        ));
    }
    let client: Client = json_body(response)?;
    if client.client_id.is_empty() || client.client_secret.is_empty() {
        return Err(RESPONSE.into());
    }
    let verifier = random()?;
    let challenge = URL_SAFE_NO_PAD.encode(digest::digest(&digest::SHA256, verifier.as_bytes()));
    let nonce = random()?;
    let mut authorize = Url::parse(&format!("{ISSUER}/oauth/authorize")).map_err(|_| RESPONSE)?;
    authorize.query_pairs_mut().extend_pairs([
        ("response_type", "code"),
        ("client_id", &client.client_id),
        ("redirect_uri", &redirect),
        ("scope", scope),
        ("state", &nonce),
        ("resource", ISSUER),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
    ]);
    if generation.load(Ordering::SeqCst) != attempt {
        return Err("Archyl 로그인을 취소했어요.".into());
    }
    app.opener()
        .open_url(authorize.as_str(), None::<&str>)
        .map_err(|_| "기본 브라우저를 열지 못했어요.")?;
    let code = callback(listener, &redirect, &nonce, generation, attempt)?;
    let reply = token(
        agent,
        &client,
        [
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", &redirect),
            ("code_verifier", &verifier),
            ("resource", ISSUER),
        ],
    )?;
    Ok(Session {
        client,
        access_token: reply.access_token,
        refresh_token: reply.refresh_token,
        scope: checked_scope(reply.scope.as_deref().unwrap_or(scope))?,
        expires_at: reply
            .expires_in
            .map(|seconds| now().saturating_add(seconds)),
    })
}

fn callback(
    listener: TcpListener,
    redirect: &str,
    nonce: &str,
    generation: &AtomicU64,
    attempt: u64,
) -> Result<String> {
    let deadline = Instant::now() + Duration::from_secs(180);
    let base = Url::parse(redirect).map_err(|_| RESPONSE)?;
    while Instant::now() < deadline && generation.load(Ordering::SeqCst) == attempt {
        let (mut stream, _) = match listener.accept() {
            Ok(connection) => connection,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(75));
                continue;
            }
            Err(_) => return Err("브라우저 로그인 응답을 받지 못했어요.".into()),
        };
        let _ = stream.set_read_timeout(Some(Duration::from_millis(750)));
        let _ = stream.set_write_timeout(Some(Duration::from_millis(750)));
        let mut line = String::new();
        let _ = BufReader::new(&stream).take(8192).read_line(&mut line);
        let mut words = line.split_whitespace();
        let target = words
            .next()
            .filter(|method| *method == "GET")
            .and_then(|_| words.next());
        let response = target.filter(|path| path.starts_with("/callback?"))
            .and_then(|path| base.join(path).ok())
            .and_then(|url| {
                let query: Vec<_> = url.query_pairs().collect();
                let states: Vec<_> = query.iter().filter(|(key, _)| key == "state").collect();
                if states.len() != 1 || states[0].1 != nonce { return None; }
                if query.iter().any(|(key, _)| key == "error") {
                    return Some(Err("Archyl에서 로그인을 승인하지 않았어요. 권한을 확인하거나 조회 전용으로 다시 연결해 주세요.".into()));
                }
                let codes: Vec<_> = query.iter().filter(|(key, value)| key == "code" && !value.is_empty()).collect();
                Some(if codes.len() == 1 { Ok(codes[0].1.to_string()) } else { Err(RESPONSE.into()) })
            });
        let (status, body) = match response.as_ref() {
            Some(Ok(_)) => ("200 OK", "Aisurada로 돌아가 연결 결과를 확인해 주세요."),
            Some(Err(_)) => ("400 Bad Request", "Archyl 로그인을 완료하지 못했어요."),
            None => ("404 Not Found", "로그인 응답 주소가 아니에요."),
        };
        let _ = write!(stream, "HTTP/1.0 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nContent-Length: {}\r\n\r\n{body}", body.len());
        if let Some(result) = response {
            return result;
        }
    }
    Err(if generation.load(Ordering::SeqCst) == attempt {
        "로그인 시간이 지났어요. 다시 연결해 주세요."
    } else {
        "Archyl 로그인을 취소했어요."
    }
    .into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpStream;

    #[test]
    fn archyl_callback_ignores_foreign_state_and_receives_the_matching_code() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let browser = thread::spawn(move || {
            for (target, status) in [
                ("/callback?state=foreign&code=wrong", "404"),
                ("/callback?state=expected&code=ok%2Bcode", "200"),
            ] {
                let mut stream = TcpStream::connect(address).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                write!(stream, "GET {target} HTTP/1.0\r\n\r\n").unwrap();
                let mut reply = String::new();
                stream.read_to_string(&mut reply).unwrap();
                assert!(reply.starts_with(&format!("HTTP/1.0 {status}")));
            }
        });
        let result = callback(
            listener,
            &format!("http://{address}/callback"),
            "expected",
            &AtomicU64::new(1),
            1,
        );
        assert_eq!(result.unwrap(), "ok+code");
        browser.join().unwrap();
    }
}
