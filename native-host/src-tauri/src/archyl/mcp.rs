use super::{http_error, NETWORK, RECONNECT, RESPONSE};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader};
use ureq::http::{HeaderValue, Response};

const ENDPOINT: &str = "https://api.archyl.com/mcp";
const PROTOCOL: &str = "2025-03-26";

#[derive(Debug)]
pub enum Error {
    Unauthorized,
    Failed(String),
}
impl From<Error> for String {
    fn from(error: Error) -> Self {
        match error {
            Error::Unauthorized => RECONNECT.into(),
            Error::Failed(message) => message,
        }
    }
}

type Result<T> = std::result::Result<T, Error>;

pub struct Mcp<'a> {
    agent: &'a ureq::Agent,
    endpoint: &'a str,
    auth: HeaderValue,
    session: Option<String>,
    version: String,
    next_id: u64,
}

impl<'a> Mcp<'a> {
    pub fn open(agent: &'a ureq::Agent, token: &str) -> Result<Self> {
        Self::open_at(agent, token, ENDPOINT)
    }

    fn open_at(agent: &'a ureq::Agent, token: &str, endpoint: &'a str) -> Result<Self> {
        let mut auth =
            HeaderValue::from_str(&format!("Bearer {token}")).map_err(|_| Error::Unauthorized)?;
        auth.set_sensitive(true);
        let mut client = Self {
            agent,
            endpoint,
            auth,
            session: None,
            version: PROTOCOL.into(),
            next_id: 2,
        };
        let response = client.post(json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL, "capabilities": {},
                "clientInfo": {"name": "Aisurada", "version": env!("CARGO_PKG_VERSION")}
            }
        }))?;
        client.session = response
            .headers()
            .get("mcp-session-id")
            .and_then(|header| header.to_str().ok())
            .map(str::to_owned);
        let hello = rpc_result(response, 1)?;
        if let Some(version) = hello["protocolVersion"].as_str() {
            client.version = version.into();
        }
        client.post(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))?;
        Ok(client)
    }

    fn post(&self, body: Value) -> Result<Response<ureq::Body>> {
        let mut request = self
            .agent
            .post(self.endpoint)
            .header("Authorization", self.auth.clone())
            .header("Accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", &self.version)
            .content_type("application/json");
        if let Some(value) = &self.session {
            request = request.header("Mcp-Session-Id", value);
        }
        let response = request
            .send(body.to_string())
            .map_err(|_| Error::Failed(NETWORK.into()))?;
        if response.status().as_u16() == 401 {
            return Err(Error::Unauthorized);
        }
        if !response.status().is_success() {
            return Err(Error::Failed(http_error(
                response,
                "Archyl에서 MCP 요청을 거절했어요.",
            )));
        }
        Ok(response)
    }

    fn rpc(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        rpc_result(
            self.post(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))?,
            id,
        )
    }

    pub fn tools(&mut self) -> Result<Vec<Value>> {
        let mut cursor = None;
        let mut tools = Vec::new();
        for _ in 0..16 {
            let params = cursor
                .take()
                .map_or_else(|| json!({}), |cursor| json!({ "cursor": cursor }));
            let value = self.rpc("tools/list", params)?;
            let page = value["tools"]
                .as_array()
                .ok_or_else(|| Error::Failed(RESPONSE.into()))?;
            tools.extend(page.iter().cloned());
            cursor = value["nextCursor"]
                .as_str()
                .filter(|token| !token.is_empty())
                .map(str::to_owned);
            if cursor.is_none() {
                return Ok(tools);
            }
        }
        Err(Error::Failed(
            "Archyl 도구 목록을 끝까지 읽지 못했어요.".into(),
        ))
    }

    pub fn call(&mut self, tool: &str, args: Value) -> Result<Value> {
        let result = self.rpc("tools/call", json!({ "name": tool, "arguments": args }))?;
        if result["isError"] == true {
            let reason = result["content"]
                .as_array()
                .and_then(|items| items.iter().find_map(|item| item["text"].as_str()))
                .unwrap_or("");
            let lower = reason.to_ascii_lowercase();
            let message = if lower.contains("permission")
                || lower.contains("forbidden")
                || lower.contains("scope")
            {
                "이 Archyl 프로젝트에서 요청한 작업을 할 권한이 없어요."
            } else {
                "Archyl에서 요청한 작업을 처리하지 못했어요. 모델을 새로고침하고 다시 확인해 주세요."
            };
            return Err(Error::Failed(message.into()));
        }
        if let Some(data) = result
            .get("structuredContent")
            .filter(|data| !data.is_null())
        {
            return Ok(data.clone());
        }
        let content = result["content"]
            .as_array()
            .ok_or_else(|| Error::Failed(RESPONSE.into()))?;
        if let Some(text) = content
            .iter()
            .find(|item| item["type"] == "text")
            .and_then(|item| item["text"].as_str())
        {
            if tool == "list_projects"
                && (text.trim().is_empty()
                    || text.trim() == "No projects found in this organization.")
            {
                return Ok(json!([]));
            }
            return Ok(serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.into())));
        }
        if tool == "list_projects" && content.is_empty() {
            return Ok(json!([]));
        }
        if content.is_empty() {
            return Ok(Value::Null);
        }
        Err(Error::Failed(RESPONSE.into()))
    }
}

fn rpc_result(mut response: Response<ureq::Body>, id: u64) -> Result<Value> {
    let matches = |message: &Value| {
        message["id"].as_u64() == Some(id)
            || message["id"].as_str().and_then(|text| text.parse().ok()) == Some(id)
    };
    let sse = response
        .headers()
        .get("content-type")
        .and_then(|header| header.to_str().ok())
        .is_some_and(|header| header.starts_with("text/event-stream"));
    let body = response.body_mut().with_config().limit(16 * 1024 * 1024);
    let malformed = || Error::Failed(RESPONSE.into());
    let message = if sse {
        let mut data = String::new();
        let mut lines = BufReader::new(body.reader()).lines();
        loop {
            let line = lines
                .next()
                .ok_or_else(malformed)?
                .map_err(|_| malformed())?;
            if let Some(value) = line.strip_prefix("data:") {
                data.push_str(value.strip_prefix(' ').unwrap_or(value));
                data.push('\n');
            } else if line.is_empty() && !data.is_empty() {
                let event = serde_json::from_str(&data).map_err(|_| malformed())?;
                if matches(&event) {
                    break event;
                }
                data.clear();
            }
        }
    } else {
        serde_json::from_slice(&body.read_to_vec().map_err(|_| malformed())?)
            .map_err(|_| malformed())?
    };
    if !matches(&message) || !message["error"].is_null() {
        return Err(malformed());
    }
    message.get("result").cloned().ok_or_else(malformed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
        time::Duration,
    };

    #[test]
    fn archyl_mcp_negotiates_and_reads_structured_and_sse_replies() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            for (index, wanted) in [
                "initialize",
                "notifications/initialized",
                "tools/call",
                "tools/call",
            ]
            .iter()
            .enumerate()
            {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut reader = BufReader::new(&mut socket);
                let mut headers = String::new();
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    headers.push_str(&line.to_ascii_lowercase());
                }
                assert!(headers.contains("authorization: bearer mock-token\r\n"));
                if index > 0 {
                    assert!(headers.contains("mcp-session-id: test-session\r\n"));
                    assert!(headers.contains("mcp-protocol-version: 2025-06-18\r\n"));
                }
                let size: usize = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length: "))
                    .unwrap()
                    .trim()
                    .parse()
                    .unwrap();
                let mut body = vec![0; size];
                reader.read_exact(&mut body).unwrap();
                let request: Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(request["method"], *wanted);
                let (status, content_type, content, extra) = match index {
                    0 => (
                        "200 OK",
                        "application/json",
                        json!({
                            "jsonrpc": "2.0", "id": 1, "result": {"protocolVersion": "2025-06-18"}
                        })
                        .to_string(),
                        "Mcp-Session-Id: test-session\r\n",
                    ),
                    1 => ("202 Accepted", "application/json", String::new(), ""),
                    2 => {
                        assert_eq!(request["params"]["name"], "list_projects");
                        (
                            "200 OK",
                            "text/event-stream",
                            format!(
                                "data: {}\n\ndata: {}\n\n",
                                json!({"method":"notifications/progress"}),
                                json!({
                                    "jsonrpc":"2.0", "id":request["id"], "result":{"content":[{"type":"text","text":"No projects found in this organization."}]}
                                })
                            ),
                            "",
                        )
                    }
                    _ => {
                        assert_eq!(request["params"]["arguments"]["projectId"], "project-1");
                        ("200 OK", "application/json", json!({
                            "jsonrpc":"2.0", "id":request["id"], "result":{"structuredContent":{"systems":[]}}
                        }).to_string(), "")
                    }
                };
                write!(socket, "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{content}", content.len()).unwrap();
            }
        });
        let agent = super::super::agent();
        let mut mcp = Mcp::open_at(&agent, "mock-token", &endpoint).unwrap();
        assert_eq!(mcp.call("list_projects", json!({})).unwrap(), json!([]));
        assert_eq!(
            mcp.call("get_project_c4_model", json!({"projectId":"project-1"}))
                .unwrap(),
            json!({"systems":[]})
        );
        server.join().unwrap();
    }
}
