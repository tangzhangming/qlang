//! HTTP标准库实现
//!
//! 提供HttpClient和HttpServer类，支持HTTP/1.1协议
//! HttpServer支持回调方式处理请求

use std::collections::HashMap;
use std::io::{Read, Write, BufRead, BufReader};
use std::net::{TcpStream, TcpListener, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use std::thread;
use parking_lot::Mutex;
use crate::vm::value::{Value, ClassInstance};
use crate::stdlib::CallbackChannel;

// ============================================================================
// 常量定义
// ============================================================================

/// HttpClient类名
pub const CLASS_HTTP_CLIENT: &str = "std.net.http.HttpClient";
/// HttpServer类名
pub const CLASS_HTTP_SERVER: &str = "std.net.http.HttpServer";
/// HttpRequest类名
pub const CLASS_HTTP_REQUEST: &str = "std.net.http.HttpRequest";
/// HttpResponse类名
pub const CLASS_HTTP_RESPONSE: &str = "std.net.http.HttpResponse";

/// 默认User-Agent
const DEFAULT_USER_AGENT: &str = "Q-HttpClient/1.0";
/// 默认超时时间（毫秒）
const DEFAULT_TIMEOUT_MS: u64 = 30000;
/// 默认缓冲区大小
const DEFAULT_BUFFER_SIZE: usize = 8192;

// ============================================================================
// URL解析
// ============================================================================

/// 解析后的URL结构
#[derive(Debug, Clone)]
pub struct ParsedUrl {
    /// 协议（http/https）
    pub protocol: String,
    /// 主机名
    pub host: String,
    /// 端口号
    pub port: u16,
    /// 路径
    pub path: String,
    /// 查询字符串
    pub query: String,
    /// 完整的查询参数
    pub query_params: HashMap<String, String>,
}

impl ParsedUrl {
    /// 解析URL字符串
    /// 支持格式: http://host:port/path?query
    pub fn parse(url: &str) -> Result<Self, String> {
        let mut protocol = "http".to_string();
        let mut remaining = url;
        
        // 解析协议
        if let Some(pos) = remaining.find("://") {
            protocol = remaining[..pos].to_lowercase();
            remaining = &remaining[pos + 3..];
        }
        
        // 不支持HTTPS
        if protocol == "https" {
            return Err("HTTPS is not supported yet".to_string());
        }
        
        if protocol != "http" {
            return Err(format!("Unsupported protocol: {}", protocol));
        }
        
        // 分离路径和查询
        let (host_port, path_query) = if let Some(pos) = remaining.find('/') {
            (&remaining[..pos], &remaining[pos..])
        } else {
            (remaining, "/")
        };
        
        // 解析主机和端口
        let (host, port) = if let Some(pos) = host_port.find(':') {
            let host = &host_port[..pos];
            let port_str = &host_port[pos + 1..];
            let port = port_str.parse::<u16>()
                .map_err(|_| format!("Invalid port: {}", port_str))?;
            (host.to_string(), port)
        } else {
            (host_port.to_string(), 80)
        };
        
        if host.is_empty() {
            return Err("Empty host".to_string());
        }
        
        // 分离路径和查询字符串
        let (path, query) = if let Some(pos) = path_query.find('?') {
            (&path_query[..pos], &path_query[pos + 1..])
        } else {
            (path_query, "")
        };
        
        // 解析查询参数
        let mut query_params = HashMap::new();
        if !query.is_empty() {
            for pair in query.split('&') {
                if let Some(pos) = pair.find('=') {
                    let key = url_decode(&pair[..pos]);
                    let value = url_decode(&pair[pos + 1..]);
                    query_params.insert(key, value);
                } else if !pair.is_empty() {
                    query_params.insert(url_decode(pair), String::new());
                }
            }
        }
        
        Ok(ParsedUrl {
            protocol,
            host,
            port,
            path: path.to_string(),
            query: query.to_string(),
            query_params,
        })
    }
    
    /// 获取完整的主机地址（host:port）
    pub fn host_header(&self) -> String {
        if self.port == 80 {
            self.host.clone()
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
    
    /// 获取请求路径（包含查询字符串）
    pub fn request_uri(&self) -> String {
        if self.query.is_empty() {
            self.path.clone()
        } else {
            format!("{}?{}", self.path, self.query)
        }
    }
}

/// URL解码
fn url_decode(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    
    while let Some(c) = chars.next() {
        if c == '%' {
            let mut hex = String::new();
            for _ in 0..2 {
                if let Some(&h) = chars.peek() {
                    if h.is_ascii_hexdigit() {
                        hex.push(chars.next().unwrap());
                    }
                }
            }
            if hex.len() == 2 {
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    result.push(byte as char);
                    continue;
                }
            }
            result.push('%');
            result.push_str(&hex);
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }
    
    result
}

/// URL编码
fn url_encode(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => {
                result.push(c);
            }
            ' ' => {
                result.push('+');
            }
            _ => {
                for byte in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", byte));
                }
            }
        }
    }
    result
}

// ============================================================================
// HTTP请求/响应构建和解析
// ============================================================================

/// 构建HTTP请求
fn build_http_request(
    method: &str,
    url: &ParsedUrl,
    headers: &HashMap<String, String>,
    body: Option<&str>,
) -> String {
    let mut request = format!("{} {} HTTP/1.1\r\n", method.to_uppercase(), url.request_uri());
    
    // Host头（必须）
    request.push_str(&format!("Host: {}\r\n", url.host_header()));
    
    // 默认User-Agent
    if !headers.contains_key("User-Agent") && !headers.contains_key("user-agent") {
        request.push_str(&format!("User-Agent: {}\r\n", DEFAULT_USER_AGENT));
    }
    
    // Connection: close（不支持keep-alive）
    if !headers.contains_key("Connection") && !headers.contains_key("connection") {
        request.push_str("Connection: close\r\n");
    }
    
    // 用户自定义头
    for (key, value) in headers {
        request.push_str(&format!("{}: {}\r\n", key, value));
    }
    
    // Content-Length（如果有body）
    if let Some(body) = body {
        if !body.is_empty() {
            if !headers.contains_key("Content-Length") && !headers.contains_key("content-length") {
                request.push_str(&format!("Content-Length: {}\r\n", body.len()));
            }
        }
    }
    
    // 空行结束头部
    request.push_str("\r\n");
    
    // 添加body
    if let Some(body) = body {
        request.push_str(body);
    }
    
    request
}

/// HTTP响应结构
#[derive(Debug, Clone)]
pub struct HttpResponseData {
    /// 状态码
    pub status: i32,
    /// 状态描述
    pub status_text: String,
    /// 响应头
    pub headers: HashMap<String, String>,
    /// 响应体
    pub body: String,
}

/// 解析HTTP响应
fn parse_http_response(reader: &mut BufReader<&mut TcpStream>) -> Result<HttpResponseData, String> {
    // 读取状态行
    let mut status_line = String::new();
    reader.read_line(&mut status_line)
        .map_err(|e| format!("Failed to read status line: {}", e))?;
    
    let status_line = status_line.trim();
    if !status_line.starts_with("HTTP/") {
        return Err(format!("Invalid HTTP response: {}", status_line));
    }
    
    // 解析状态码
    let parts: Vec<&str> = status_line.splitn(3, ' ').collect();
    if parts.len() < 2 {
        return Err("Invalid status line".to_string());
    }
    
    let status = parts[1].parse::<i32>()
        .map_err(|_| format!("Invalid status code: {}", parts[1]))?;
    let status_text = if parts.len() > 2 { parts[2].to_string() } else { String::new() };
    
    // 读取响应头
    let mut headers = HashMap::new();
    let mut content_length: Option<usize> = None;
    let mut chunked = false;
    
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)
            .map_err(|e| format!("Failed to read header: {}", e))?;
        
        let line = line.trim();
        if line.is_empty() {
            break;
        }
        
        if let Some(pos) = line.find(':') {
            let key = line[..pos].trim().to_string();
            let value = line[pos + 1..].trim().to_string();
            
            // 检查Content-Length
            if key.eq_ignore_ascii_case("Content-Length") {
                content_length = value.parse().ok();
            }
            
            // 检查Transfer-Encoding
            if key.eq_ignore_ascii_case("Transfer-Encoding") && value.eq_ignore_ascii_case("chunked") {
                chunked = true;
            }
            
            headers.insert(key, value);
        }
    }
    
    // 读取响应体
    let body = if chunked {
        // 分块传输编码
        read_chunked_body(reader)?
    } else if let Some(len) = content_length {
        // 固定长度
        let mut body = vec![0u8; len];
        reader.read_exact(&mut body)
            .map_err(|e| format!("Failed to read body: {}", e))?;
        String::from_utf8_lossy(&body).to_string()
    } else {
        // 读取到EOF
        let mut body = String::new();
        reader.read_to_string(&mut body)
            .map_err(|e| format!("Failed to read body: {}", e))?;
        body
    };
    
    Ok(HttpResponseData {
        status,
        status_text,
        headers,
        body,
    })
}

/// 读取分块传输编码的响应体
fn read_chunked_body(reader: &mut BufReader<&mut TcpStream>) -> Result<String, String> {
    let mut body = Vec::new();
    
    loop {
        // 读取chunk大小
        let mut size_line = String::new();
        reader.read_line(&mut size_line)
            .map_err(|e| format!("Failed to read chunk size: {}", e))?;
        
        let size = usize::from_str_radix(size_line.trim(), 16)
            .map_err(|_| format!("Invalid chunk size: {}", size_line.trim()))?;
        
        if size == 0 {
            // 最后一个chunk
            // 读取尾部的空行
            let mut trailer = String::new();
            reader.read_line(&mut trailer).ok();
            break;
        }
        
        // 读取chunk数据
        let mut chunk = vec![0u8; size];
        reader.read_exact(&mut chunk)
            .map_err(|e| format!("Failed to read chunk: {}", e))?;
        body.extend_from_slice(&chunk);
        
        // 读取chunk后的\r\n
        let mut crlf = [0u8; 2];
        reader.read_exact(&mut crlf).ok();
    }
    
    Ok(String::from_utf8_lossy(&body).to_string())
}

// ============================================================================
// HttpClient Handle
// ============================================================================

/// HttpClient句柄
pub struct HttpClientHandle {
    /// 超时时间（毫秒）
    timeout_ms: Mutex<u64>,
}

impl HttpClientHandle {
    fn new(timeout_ms: u64) -> Self {
        Self {
            timeout_ms: Mutex::new(timeout_ms),
        }
    }
    
    /// 发送HTTP请求
    fn request(
        &self,
        method: &str,
        url: &str,
        body: Option<&str>,
        headers: &HashMap<String, String>,
    ) -> Result<HttpResponseData, String> {
        // 解析URL
        let parsed_url = ParsedUrl::parse(url)?;
        
        // 建立TCP连接
        let addr = format!("{}:{}", parsed_url.host, parsed_url.port)
            .parse::<SocketAddr>()
            .map_err(|e| format!("Invalid address: {}", e))?;
        
        let timeout = Duration::from_millis(*self.timeout_ms.lock());
        let mut stream = TcpStream::connect_timeout(&addr, timeout)
            .map_err(|e| format!("Connection failed: {}", e))?;
        
        stream.set_read_timeout(Some(timeout)).ok();
        stream.set_write_timeout(Some(timeout)).ok();
        
        // 构建并发送请求
        let request = build_http_request(method, &parsed_url, headers, body);
        stream.write_all(request.as_bytes())
            .map_err(|e| format!("Failed to send request: {}", e))?;
        stream.flush()
            .map_err(|e| format!("Failed to flush: {}", e))?;
        
        // 读取响应
        let mut reader = BufReader::new(&mut stream);
        parse_http_response(&mut reader)
    }
}

// ============================================================================
// HttpServer Handle
// ============================================================================

/// HttpServer句柄
pub struct HttpServerHandle {
    /// TCP监听器
    listener: Option<TcpListener>,
    /// 主机地址
    host: String,
    /// 端口号
    port: u16,
    /// 运行标志
    running: Arc<AtomicBool>,
}

impl HttpServerHandle {
    fn new(host: String, port: u16) -> Result<Self, String> {
        let addr = format!("{}:{}", host, port);
        let listener = TcpListener::bind(&addr)
            .map_err(|e| format!("Failed to bind {}: {}", addr, e))?;
        
        // 设置非阻塞模式以便能够检查停止标志
        listener.set_nonblocking(true)
            .map_err(|e| format!("Failed to set non-blocking: {}", e))?;
        
        Ok(Self {
            listener: Some(listener),
            host,
            port,
            running: Arc::new(AtomicBool::new(false)),
        })
    }
    
    /// 停止服务器
    fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
    
    /// 检查是否正在运行
    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

/// 解析HTTP请求（服务端）
fn parse_http_request(stream: &mut TcpStream) -> Result<HttpRequestData, String> {
    let mut reader = BufReader::new(stream);
    
    // 读取请求行
    let mut request_line = String::new();
    reader.read_line(&mut request_line)
        .map_err(|e| format!("Failed to read request line: {}", e))?;
    
    let request_line = request_line.trim();
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    
    if parts.len() < 3 {
        return Err(format!("Invalid request line: {}", request_line));
    }
    
    let method = parts[0].to_uppercase();
    let uri = parts[1];
    let _version = parts[2];
    
    // 解析路径和查询字符串
    let (path, query_string) = if let Some(pos) = uri.find('?') {
        (&uri[..pos], &uri[pos + 1..])
    } else {
        (uri, "")
    };
    
    // 解析查询参数
    let mut query = HashMap::new();
    if !query_string.is_empty() {
        for pair in query_string.split('&') {
            if let Some(pos) = pair.find('=') {
                let key = url_decode(&pair[..pos]);
                let value = url_decode(&pair[pos + 1..]);
                query.insert(key, value);
            } else if !pair.is_empty() {
                query.insert(url_decode(pair), String::new());
            }
        }
    }
    
    // 读取请求头
    let mut headers = HashMap::new();
    let mut content_length: Option<usize> = None;
    
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)
            .map_err(|e| format!("Failed to read header: {}", e))?;
        
        let line = line.trim();
        if line.is_empty() {
            break;
        }
        
        if let Some(pos) = line.find(':') {
            let key = line[..pos].trim().to_string();
            let value = line[pos + 1..].trim().to_string();
            
            if key.eq_ignore_ascii_case("Content-Length") {
                content_length = value.parse().ok();
            }
            
            headers.insert(key, value);
        }
    }
    
    // 读取请求体
    let body = if let Some(len) = content_length {
        if len > 0 {
            let mut body = vec![0u8; len];
            reader.read_exact(&mut body)
                .map_err(|e| format!("Failed to read body: {}", e))?;
            String::from_utf8_lossy(&body).to_string()
        } else {
            String::new()
        }
    } else {
        String::new()
    };
    
    Ok(HttpRequestData {
        method,
        path: path.to_string(),
        query,
        headers,
        body,
    })
}

/// HTTP请求数据结构
#[derive(Debug, Clone)]
pub struct HttpRequestData {
    /// HTTP方法
    pub method: String,
    /// 请求路径
    pub path: String,
    /// 查询参数
    pub query: HashMap<String, String>,
    /// 请求头
    pub headers: HashMap<String, String>,
    /// 请求体
    pub body: String,
}

/// 构建HTTP响应
fn build_http_response(status: i32, headers: &HashMap<String, String>, body: &str) -> String {
    let status_text = match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "Unknown",
    };
    
    let mut response = format!("HTTP/1.1 {} {}\r\n", status, status_text);
    
    // 默认Content-Type
    if !headers.contains_key("Content-Type") && !headers.contains_key("content-type") {
        response.push_str("Content-Type: text/plain; charset=utf-8\r\n");
    }
    
    // Content-Length
    if !headers.contains_key("Content-Length") && !headers.contains_key("content-length") {
        response.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    
    // Connection: close
    if !headers.contains_key("Connection") && !headers.contains_key("connection") {
        response.push_str("Connection: close\r\n");
    }
    
    // 用户自定义头
    for (key, value) in headers {
        response.push_str(&format!("{}: {}\r\n", key, value));
    }
    
    // 空行结束头部
    response.push_str("\r\n");
    
    // 添加body
    response.push_str(body);
    
    response
}

// ============================================================================
// Value创建辅助函数
// ============================================================================

/// 创建HttpClient类实例
pub fn create_http_client_instance(ptr: u64) -> Value {
    let mut fields = HashMap::new();
    fields.insert("__handle".to_string(), Value::int(ptr as i128));
    
    let instance = ClassInstance {
        class_name: CLASS_HTTP_CLIENT.to_string(),
        parent_class: None,
        fields,
    };
    
    Value::class(Arc::new(Mutex::new(instance)))
}

/// 创建HttpServer类实例
pub fn create_http_server_instance(ptr: u64) -> Value {
    let mut fields = HashMap::new();
    fields.insert("__handle".to_string(), Value::int(ptr as i128));
    
    let instance = ClassInstance {
        class_name: CLASS_HTTP_SERVER.to_string(),
        parent_class: None,
        fields,
    };
    
    Value::class(Arc::new(Mutex::new(instance)))
}

/// 创建HttpRequest类实例
pub fn create_http_request_instance(request: &HttpRequestData) -> Value {
    let mut fields = HashMap::new();
    
    // 基本字段
    fields.insert("method".to_string(), Value::string(request.method.clone()));
    fields.insert("path".to_string(), Value::string(request.path.clone()));
    fields.insert("body".to_string(), Value::string(request.body.clone()));
    
    // 查询参数转为map
    let query_map = create_string_map(&request.query);
    fields.insert("query".to_string(), query_map);
    
    // 请求头转为map
    let headers_map = create_string_map(&request.headers);
    fields.insert("headers".to_string(), headers_map);
    
    let instance = ClassInstance {
        class_name: CLASS_HTTP_REQUEST.to_string(),
        parent_class: None,
        fields,
    };
    
    Value::class(Arc::new(Mutex::new(instance)))
}

/// 创建HttpResponse类实例（从响应数据）
pub fn create_http_response_from_data(response: &HttpResponseData) -> Value {
    let mut fields = HashMap::new();
    
    fields.insert("status".to_string(), Value::int(response.status as i128));
    fields.insert("body".to_string(), Value::string(response.body.clone()));
    
    // 响应头转为map
    let headers_map = create_string_map(&response.headers);
    fields.insert("headers".to_string(), headers_map);
    
    let instance = ClassInstance {
        class_name: CLASS_HTTP_RESPONSE.to_string(),
        parent_class: None,
        fields,
    };
    
    Value::class(Arc::new(Mutex::new(instance)))
}

/// 创建HttpResponse类实例（用于构造函数）
pub fn create_http_response_instance(status: i128, body: String, headers: HashMap<String, String>) -> Value {
    let mut fields = HashMap::new();
    
    fields.insert("status".to_string(), Value::int(status));
    fields.insert("body".to_string(), Value::string(body));
    
    let headers_map = create_string_map(&headers);
    fields.insert("headers".to_string(), headers_map);
    
    let instance = ClassInstance {
        class_name: CLASS_HTTP_RESPONSE.to_string(),
        parent_class: None,
        fields,
    };
    
    Value::class(Arc::new(Mutex::new(instance)))
}

/// 创建字符串map的Value
fn create_string_map(map: &HashMap<String, String>) -> Value {
    let mut result = HashMap::new();
    for (k, v) in map {
        result.insert(k.clone(), Value::string(v.clone()));
    }
    Value::map(Arc::new(Mutex::new(result)))
}

/// 从Value提取字符串map
fn extract_string_map(value: &Value) -> HashMap<String, String> {
    let mut result = HashMap::new();
    if let Some(map) = value.as_map() {
        let map = map.lock();
        for (k, v) in map.iter() {
            if let Some(s) = v.as_string() {
                result.insert(k.clone(), s.clone());
            }
        }
    }
    result
}

/// 从实例提取handle指针
fn extract_handle_ptr(instance: &Value, class_name: &str) -> Result<u64, String> {
    if let Some(class_instance) = instance.as_class() {
        let instance = class_instance.lock();
        if let Some(handle_value) = instance.fields.get("__handle") {
            if let Some(ptr) = handle_value.as_int() {
                return Ok(ptr as u64);
            }
        }
        Err(format!("{} instance has no valid handle", class_name))
    } else {
        Err(format!("Value is not a {} instance", class_name))
    }
}

// ============================================================================
// HttpClient 类方法实现
// ============================================================================

/// HttpClient 构造函数
/// init(timeout_ms?: int) -> HttpClient
pub fn http_client_init(args: &[Value]) -> Result<Value, String> {
    let timeout_ms = if !args.is_empty() {
        args[0].as_int().unwrap_or(DEFAULT_TIMEOUT_MS as i128) as u64
    } else {
        DEFAULT_TIMEOUT_MS
    };
    
    let handle = Box::new(HttpClientHandle::new(timeout_ms));
    let ptr = Box::into_raw(handle) as u64;
    
    Ok(create_http_client_instance(ptr))
}

/// HttpClient.get(url: string, headers?: map) -> HttpResponse
pub fn http_client_get(instance: &Value, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("HttpClient.get requires at least 1 argument: url".to_string());
    }
    
    let client_ptr = extract_handle_ptr(instance, "HttpClient")?;
    let url = args[0].as_string()
        .ok_or_else(|| "Invalid url: expected string".to_string())?;
    
    let headers = if args.len() > 1 {
        extract_string_map(&args[1])
    } else {
        HashMap::new()
    };
    
    let handle = unsafe { &*(client_ptr as *const HttpClientHandle) };
    let response = handle.request("GET", &url, None, &headers)?;
    
    Ok(create_http_response_from_data(&response))
}

/// HttpClient.post(url: string, body?: string, headers?: map) -> HttpResponse
pub fn http_client_post(instance: &Value, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("HttpClient.post requires at least 1 argument: url".to_string());
    }
    
    let client_ptr = extract_handle_ptr(instance, "HttpClient")?;
    let url = args[0].as_string()
        .ok_or_else(|| "Invalid url: expected string".to_string())?;
    
    let body = if args.len() > 1 {
        args[1].as_string().map(|s| s.clone())
    } else {
        None
    };
    
    let headers = if args.len() > 2 {
        extract_string_map(&args[2])
    } else {
        HashMap::new()
    };
    
    let handle = unsafe { &*(client_ptr as *const HttpClientHandle) };
    let response = handle.request("POST", &url, body.as_deref(), &headers)?;
    
    Ok(create_http_response_from_data(&response))
}

/// HttpClient.put(url: string, body?: string, headers?: map) -> HttpResponse
pub fn http_client_put(instance: &Value, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("HttpClient.put requires at least 1 argument: url".to_string());
    }
    
    let client_ptr = extract_handle_ptr(instance, "HttpClient")?;
    let url = args[0].as_string()
        .ok_or_else(|| "Invalid url: expected string".to_string())?;
    
    let body = if args.len() > 1 {
        args[1].as_string().map(|s| s.clone())
    } else {
        None
    };
    
    let headers = if args.len() > 2 {
        extract_string_map(&args[2])
    } else {
        HashMap::new()
    };
    
    let handle = unsafe { &*(client_ptr as *const HttpClientHandle) };
    let response = handle.request("PUT", &url, body.as_deref(), &headers)?;
    
    Ok(create_http_response_from_data(&response))
}

/// HttpClient.delete(url: string, headers?: map) -> HttpResponse
pub fn http_client_delete(instance: &Value, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("HttpClient.delete requires at least 1 argument: url".to_string());
    }
    
    let client_ptr = extract_handle_ptr(instance, "HttpClient")?;
    let url = args[0].as_string()
        .ok_or_else(|| "Invalid url: expected string".to_string())?;
    
    let headers = if args.len() > 1 {
        extract_string_map(&args[1])
    } else {
        HashMap::new()
    };
    
    let handle = unsafe { &*(client_ptr as *const HttpClientHandle) };
    let response = handle.request("DELETE", &url, None, &headers)?;
    
    Ok(create_http_response_from_data(&response))
}

/// HttpClient.request(method: string, url: string, body?: string, headers?: map) -> HttpResponse
pub fn http_client_request(instance: &Value, args: &[Value]) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("HttpClient.request requires at least 2 arguments: method, url".to_string());
    }
    
    let client_ptr = extract_handle_ptr(instance, "HttpClient")?;
    let method = args[0].as_string()
        .ok_or_else(|| "Invalid method: expected string".to_string())?;
    let url = args[1].as_string()
        .ok_or_else(|| "Invalid url: expected string".to_string())?;
    
    let body = if args.len() > 2 {
        args[2].as_string().map(|s| s.clone())
    } else {
        None
    };
    
    let headers = if args.len() > 3 {
        extract_string_map(&args[3])
    } else {
        HashMap::new()
    };
    
    let handle = unsafe { &*(client_ptr as *const HttpClientHandle) };
    let response = handle.request(&method, &url, body.as_deref(), &headers)?;
    
    Ok(create_http_response_from_data(&response))
}

/// HttpClient.setTimeout(timeout_ms: int) -> null
pub fn http_client_set_timeout(instance: &Value, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("HttpClient.setTimeout requires 1 argument: timeout_ms".to_string());
    }
    
    let client_ptr = extract_handle_ptr(instance, "HttpClient")?;
    let timeout_ms = args[0].as_int()
        .ok_or_else(|| "Invalid timeout_ms: expected integer".to_string())? as u64;
    
    let handle = unsafe { &*(client_ptr as *const HttpClientHandle) };
    *handle.timeout_ms.lock() = timeout_ms;
    
    Ok(Value::null())
}

/// HttpClient.close() -> null
pub fn http_client_close(instance: &Value, _args: &[Value]) -> Result<Value, String> {
    let client_ptr = extract_handle_ptr(instance, "HttpClient")?;
    
    // 释放资源
    let _handle = unsafe { Box::from_raw(client_ptr as *mut HttpClientHandle) };
    
    Ok(Value::null())
}

// ============================================================================
// HttpServer 类方法实现
// ============================================================================

/// HttpServer 构造函数
/// init(host: string, port: int) -> HttpServer
pub fn http_server_init(args: &[Value]) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("HttpServer.init requires 2 arguments: host, port".to_string());
    }
    
    let host = args[0].as_string()
        .ok_or_else(|| "Invalid host: expected string".to_string())?;
    let port = args[1].as_int()
        .ok_or_else(|| "Invalid port: expected integer".to_string())? as u16;
    
    let handle = Box::new(HttpServerHandle::new(host.clone(), port)?);
    let ptr = Box::into_raw(handle) as u64;
    
    Ok(create_http_server_instance(ptr))
}

/// HttpServer.listen(handler: func(HttpRequest) HttpResponse) -> null
/// 这是一个需要回调支持的方法
pub fn http_server_listen(
    instance: &Value,
    args: &[Value],
    callback_channel: Arc<CallbackChannel>,
) -> Result<Value, String> {
    if args.is_empty() {
        return Err("HttpServer.listen requires 1 argument: handler".to_string());
    }
    
    let server_ptr = extract_handle_ptr(instance, "HttpServer")?;
    let handler = args[0].clone();
    
    // 验证handler是函数或闭包
    if !handler.is_function() {
        return Err("Invalid handler: expected function".to_string());
    }
    
    let handle = unsafe { &mut *(server_ptr as *mut HttpServerHandle) };
    
    // 设置运行标志
    handle.running.store(true, Ordering::SeqCst);
    
    let listener = handle.listener.take()
        .ok_or_else(|| "Server listener not available".to_string())?;
    
    let running = handle.running.clone();
    
    // 服务器主循环
    while running.load(Ordering::SeqCst) {
        // 非阻塞accept
        match listener.accept() {
            Ok((mut stream, _addr)) => {
                // 设置读超时
                stream.set_read_timeout(Some(Duration::from_secs(30))).ok();
                stream.set_write_timeout(Some(Duration::from_secs(30))).ok();
                
                // 解析HTTP请求
                match parse_http_request(&mut stream) {
                    Ok(request_data) => {
                        // 创建HttpRequest实例
                        let request_value = create_http_request_instance(&request_data);
                        
                        // 通过回调通道调用handler
                        match callback_channel.call(handler.clone(), vec![request_value]) {
                            Ok(response_value) => {
                                // 从response_value提取响应数据
                                let (status, body, headers) = extract_response_data(&response_value)?;
                                
                                // 构建并发送HTTP响应
                                let response = build_http_response(status, &headers, &body);
                                if let Err(e) = stream.write_all(response.as_bytes()) {
                                    eprintln!("Failed to send response: {}", e);
                                }
                                stream.flush().ok();
                            }
                            Err(e) => {
                                // 发送500错误
                                let response = build_http_response(
                                    500,
                                    &HashMap::new(),
                                    &format!("Internal Server Error: {}", e),
                                );
                                stream.write_all(response.as_bytes()).ok();
                                stream.flush().ok();
                            }
                        }
                    }
                    Err(e) => {
                        // 发送400错误
                        let response = build_http_response(
                            400,
                            &HashMap::new(),
                            &format!("Bad Request: {}", e),
                        );
                        stream.write_all(response.as_bytes()).ok();
                        stream.flush().ok();
                    }
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // 非阻塞模式下没有连接，短暂休眠后重试
                thread::sleep(Duration::from_millis(10));
            }
            Err(e) => {
                // 其他错误，记录并继续
                eprintln!("Accept error: {}", e);
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
    
    Ok(Value::null())
}

/// 从HttpResponse实例提取响应数据
fn extract_response_data(response: &Value) -> Result<(i32, String, HashMap<String, String>), String> {
    if let Some(class_instance) = response.as_class() {
        let instance = class_instance.lock();
        
        let status = instance.fields.get("status")
            .and_then(|v| v.as_int())
            .unwrap_or(200) as i32;
        
        let body = instance.fields.get("body")
            .and_then(|v| v.as_string())
            .map(|s| s.clone())
            .unwrap_or_default();
        
        let headers = instance.fields.get("headers")
            .map(|v| extract_string_map(v))
            .unwrap_or_default();
        
        Ok((status, body, headers))
    } else {
        Err("Invalid response: expected HttpResponse instance".to_string())
    }
}

/// HttpServer.stop() -> null
pub fn http_server_stop(instance: &Value, _args: &[Value]) -> Result<Value, String> {
    let server_ptr = extract_handle_ptr(instance, "HttpServer")?;
    
    let handle = unsafe { &*(server_ptr as *const HttpServerHandle) };
    handle.stop();
    
    Ok(Value::null())
}

// ============================================================================
// HttpRequest 类方法实现
// ============================================================================

/// HttpRequest.getHeader(name: string) -> string
pub fn http_request_get_header(instance: &Value, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("HttpRequest.getHeader requires 1 argument: name".to_string());
    }
    
    let name = args[0].as_string()
        .ok_or_else(|| "Invalid name: expected string".to_string())?;
    
    if let Some(class_instance) = instance.as_class() {
        let instance = class_instance.lock();
        if let Some(headers) = instance.fields.get("headers") {
            if let Some(map) = headers.as_map() {
                let map = map.lock();
                // 不区分大小写查找
                for (k, v) in map.iter() {
                    if k.eq_ignore_ascii_case(&name) {
                        return Ok(v.clone());
                    }
                }
            }
        }
    }
    
    Ok(Value::string(String::new()))
}

/// HttpRequest.getQuery(name: string) -> string
pub fn http_request_get_query(instance: &Value, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("HttpRequest.getQuery requires 1 argument: name".to_string());
    }
    
    let name = args[0].as_string()
        .ok_or_else(|| "Invalid name: expected string".to_string())?;
    
    if let Some(class_instance) = instance.as_class() {
        let instance = class_instance.lock();
        if let Some(query) = instance.fields.get("query") {
            if let Some(map) = query.as_map() {
                let map = map.lock();
                if let Some(value) = map.get(&*name) {
                    return Ok(value.clone());
                }
            }
        }
    }
    
    Ok(Value::string(String::new()))
}

// ============================================================================
// HttpResponse 类方法实现
// ============================================================================

/// HttpResponse 构造函数
/// init(status: int, body?: string, headers?: map) -> HttpResponse
pub fn http_response_init(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("HttpResponse.init requires at least 1 argument: status".to_string());
    }
    
    let status = args[0].as_int()
        .ok_or_else(|| "Invalid status: expected integer".to_string())?;
    
    let body = if args.len() > 1 {
        args[1].as_string().map(|s| s.clone()).unwrap_or_default()
    } else {
        String::new()
    };
    
    let headers = if args.len() > 2 {
        extract_string_map(&args[2])
    } else {
        HashMap::new()
    };
    
    Ok(create_http_response_instance(status, body, headers))
}

/// HttpResponse.text() -> string
pub fn http_response_text(instance: &Value, _args: &[Value]) -> Result<Value, String> {
    if let Some(class_instance) = instance.as_class() {
        let instance = class_instance.lock();
        if let Some(body) = instance.fields.get("body") {
            return Ok(body.clone());
        }
    }
    
    Ok(Value::string(String::new()))
}

/// HttpResponse.setHeader(name: string, value: string) -> null
pub fn http_response_set_header(instance: &Value, args: &[Value]) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("HttpResponse.setHeader requires 2 arguments: name, value".to_string());
    }
    
    let name = args[0].as_string()
        .ok_or_else(|| "Invalid name: expected string".to_string())?;
    let value = args[1].as_string()
        .ok_or_else(|| "Invalid value: expected string".to_string())?;
    
    if let Some(class_instance) = instance.as_class() {
        let mut instance = class_instance.lock();
        if let Some(headers) = instance.fields.get_mut("headers") {
            if let Some(map) = headers.as_map() {
                map.lock().insert(name.clone(), Value::string(value.clone()));
            }
        }
    }
    
    Ok(Value::null())
}
