use std::net::{TcpStream, TcpListener, SocketAddr, Shutdown};
use std::io::{Read, Write};
use std::time::Duration;
use std::sync::Arc;
use parking_lot::Mutex;
use crate::vm::value::Value;
use std::collections::HashMap;

// Socket包装（存储在堆上）
pub struct TcpSocketHandle {
    stream: Arc<Mutex<Option<TcpStream>>>,
    closed: Arc<Mutex<bool>>,
}

// Listener包装
pub struct TcpListenerHandle {
    listener: Arc<Mutex<Option<TcpListener>>>,
    closed: Arc<Mutex<bool>>,
}

// 标准库类名常量
pub const CLASS_TCPSOCKET: &str = "std.net.tcp.TCPSocket";
pub const CLASS_TCPLISTENER: &str = "std.net.tcp.TCPListener";

// 从ClassInstance提取原生指针（存储在"__handle"字段中）
fn extract_socket_ptr_from_instance(instance: &Value) -> Result<u64, String> {
    use crate::vm::value::Value;
    use std::sync::Arc;
    use parking_lot::Mutex;
    
    if let Some(class_instance) = instance.as_class() {
        let instance = class_instance.lock();
        if let Some(handle_value) = instance.fields.get("__handle") {
            if let Some(ptr) = handle_value.as_int() {
                return Ok(ptr as u64);
            }
        }
        Err("TCPSocket instance has no valid handle".to_string())
    } else {
        Err("Value is not a TCPSocket instance".to_string())
    }
}

fn extract_listener_ptr_from_instance(instance: &Value) -> Result<u64, String> {
    if let Some(class_instance) = instance.as_class() {
        let instance = class_instance.lock();
        if let Some(handle_value) = instance.fields.get("__handle") {
            if let Some(ptr) = handle_value.as_int() {
                return Ok(ptr as u64);
            }
        }
        Err("TCPListener instance has no valid handle".to_string())
    } else {
        Err("Value is not a TCPListener instance".to_string())
    }
}

// 创建TCPSocket类实例
pub fn create_tcp_socket_instance(ptr: u64) -> Value {
    use crate::vm::value::ClassInstance;
    use std::sync::Arc;
    use parking_lot::Mutex;
    
    let mut fields = HashMap::new();
    fields.insert("__handle".to_string(), Value::int(ptr as i128));
    
    let instance = ClassInstance {
        class_name: CLASS_TCPSOCKET.to_string(),
        parent_class: None,
        fields,
    };
    
    Value::class(Arc::new(Mutex::new(instance)))
}

// 创建TCPListener类实例
pub fn create_tcp_listener_instance(ptr: u64) -> Value {
    use crate::vm::value::ClassInstance;
    use std::sync::Arc;
    use parking_lot::Mutex;
    
    let mut fields = HashMap::new();
    fields.insert("__handle".to_string(), Value::int(ptr as i128));
    
    let instance = ClassInstance {
        class_name: CLASS_TCPLISTENER.to_string(),
        parent_class: None,
        fields,
    };
    
    Value::class(Arc::new(Mutex::new(instance)))
}

// ============================================================================
// TCPSocket 类方法实现
// ============================================================================

/// TCPSocket 构造函数
/// init(host: string, port: int, timeout?: int) -> TCPSocket
pub fn tcp_socket_init(args: &[Value]) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("TCPSocket.init requires at least 2 arguments: host, port".to_string());
    }

    // 提取参数: host, port, timeout
    let host = args[0].as_string()
        .ok_or_else(|| "Invalid host: expected string".to_string())?;
    let port = args[1].as_int()
        .ok_or_else(|| "Invalid port: expected integer".to_string())? as u16;
    let timeout_ms = if args.len() > 2 {
        args[2].as_int().unwrap_or(5000)
    } else {
        5000
    } as u64;

    // 解析地址并连接
    let addr = format!("{}:{}", host, port)
        .parse::<SocketAddr>()
        .map_err(|e| format!("Invalid address: {}", e))?;

    let stream = TcpStream::connect_timeout(&addr, Duration::from_millis(timeout_ms))
        .map_err(|e| format!("Connection failed: {}", e))?;

    // 创建handle并包装为类实例
    let handle = Box::new(TcpSocketHandle {
        stream: Arc::new(Mutex::new(Some(stream))),
        closed: Arc::new(Mutex::new(false)),
    });
    let ptr = Box::into_raw(handle) as u64;

    Ok(create_tcp_socket_instance(ptr))
}

/// TCPSocket.send(data: int[]) -> int
/// 发送数据，返回实际发送的字节数
pub fn tcp_socket_send(instance: &Value, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("TCPSocket.send requires 1 argument: data".to_string());
    }

    let socket_ptr = extract_socket_ptr_from_instance(instance)?;
    let data = args[0].as_array()
        .ok_or_else(|| "Invalid data: expected array".to_string())?;

    let handle = unsafe { &*(socket_ptr as *const TcpSocketHandle) };

    // 检查是否已关闭
    if *handle.closed.lock() {
        return Err("Socket is closed".to_string());
    }

    let mut stream_opt = handle.stream.lock();
    let stream = stream_opt.as_mut()
        .ok_or_else(|| "Socket is closed".to_string())?;

    // 转换byte array为Vec<u8>
    let bytes: Vec<u8> = data.lock()
        .iter()
        .filter_map(|v: &Value| v.as_int().map(|i| i as u8))
        .collect();

    let n = stream.write(&bytes)
        .map_err(|e| format!("Write error: {}", e))?;

    Ok(Value::int(n as i128))
}

/// TCPSocket.receive(buffer: int[]) -> int
/// 接收数据到buffer，返回实际接收的字节数
pub fn tcp_socket_receive(instance: &Value, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("TCPSocket.receive requires 1 argument: buffer".to_string());
    }

    let socket_ptr = extract_socket_ptr_from_instance(instance)?;
    let buffer = args[0].as_array()
        .ok_or_else(|| "Invalid buffer: expected array".to_string())?;

    let handle = unsafe { &*(socket_ptr as *const TcpSocketHandle) };

    // 检查是否已关闭
    if *handle.closed.lock() {
        return Err("Socket is closed".to_string());
    }

    let mut stream_opt = handle.stream.lock();
    let stream = stream_opt.as_mut()
        .ok_or_else(|| "Socket is closed".to_string())?;

    let buffer_len = buffer.lock().len();
    let mut buf = vec![0u8; buffer_len];

    let n = stream.read(&mut buf)
        .map_err(|e| format!("Read error: {}", e))?;

    // 写入buffer
    let mut buffer_guard = buffer.lock();
    for (i, &byte) in buf[..n].iter().enumerate() {
        if i < buffer_guard.len() {
            buffer_guard[i] = Value::int(byte as i128);
        }
    }

    Ok(Value::int(n as i128))
}

/// TCPSocket.close() -> null
/// 关闭socket连接
pub fn tcp_socket_close(instance: &Value, _args: &[Value]) -> Result<Value, String> {
    let socket_ptr = extract_socket_ptr_from_instance(instance)?;
    let handle = unsafe { &*(socket_ptr as *const TcpSocketHandle) };

    let mut closed = handle.closed.lock();
    if *closed {
        // 已经关闭，直接返回（防止双重释放）
        return Ok(Value::null());
    }

    // 标记为已关闭
    *closed = true;

    // 关闭stream
    if let Some(stream) = handle.stream.lock().take() {
        drop(stream);  // 显式关闭TCP连接
    }

    Ok(Value::null())
}

/// TCPSocket.setReadTimeout(timeout_ms: int) -> null
/// 设置读超时时间（毫秒）
pub fn tcp_socket_set_read_timeout(instance: &Value, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("TCPSocket.setReadTimeout requires 1 argument: timeout_ms".to_string());
    }

    let socket_ptr = extract_socket_ptr_from_instance(instance)?;
    let timeout_ms = args[0].as_int()
        .ok_or_else(|| "Invalid timeout: expected integer".to_string())? as u64;

    let handle = unsafe { &*(socket_ptr as *const TcpSocketHandle) };

    // 检查是否已关闭
    if *handle.closed.lock() {
        return Err("Socket is closed".to_string());
    }

    let stream_opt = handle.stream.lock();
    let stream = stream_opt.as_ref()
        .ok_or_else(|| "Socket is closed".to_string())?;

    stream.set_read_timeout(Some(Duration::from_millis(timeout_ms)))
        .map_err(|e| format!("Failed to set read timeout: {}", e))?;

    Ok(Value::null())
}

/// TCPSocket.setWriteTimeout(timeout_ms: int) -> null
/// 设置写超时时间（毫秒）
pub fn tcp_socket_set_write_timeout(instance: &Value, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("TCPSocket.setWriteTimeout requires 1 argument: timeout_ms".to_string());
    }

    let socket_ptr = extract_socket_ptr_from_instance(instance)?;
    let timeout_ms = args[0].as_int()
        .ok_or_else(|| "Invalid timeout: expected integer".to_string())? as u64;

    let handle = unsafe { &*(socket_ptr as *const TcpSocketHandle) };

    // 检查是否已关闭
    if *handle.closed.lock() {
        return Err("Socket is closed".to_string());
    }

    let stream_opt = handle.stream.lock();
    let stream = stream_opt.as_ref()
        .ok_or_else(|| "Socket is closed".to_string())?;

    stream.set_write_timeout(Some(Duration::from_millis(timeout_ms)))
        .map_err(|e| format!("Failed to set write timeout: {}", e))?;

    Ok(Value::null())
}

/// TCPSocket.setNoDelay(enabled: bool) -> null
/// 设置TCP_NODELAY选项
pub fn tcp_socket_set_no_delay(instance: &Value, args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("TCPSocket.setNoDelay requires 1 argument: enabled".to_string());
    }

    let socket_ptr = extract_socket_ptr_from_instance(instance)?;
    let enabled = args[0].as_bool()
        .ok_or_else(|| "Invalid boolean value: expected boolean".to_string())?;

    let handle = unsafe { &*(socket_ptr as *const TcpSocketHandle) };

    // 检查是否已关闭
    if *handle.closed.lock() {
        return Err("Socket is closed".to_string());
    }

    let stream_opt = handle.stream.lock();
    let stream = stream_opt.as_ref()
        .ok_or_else(|| "Socket is closed".to_string())?;

    stream.set_nodelay(enabled)
        .map_err(|e| format!("Failed to set nodelay: {}", e))?;

    Ok(Value::null())
}

/// TCPSocket.shutdown() -> null
/// 优雅关闭socket（关闭写端）
pub fn tcp_socket_shutdown(instance: &Value, _args: &[Value]) -> Result<Value, String> {
    let socket_ptr = extract_socket_ptr_from_instance(instance)?;
    let handle = unsafe { &*(socket_ptr as *const TcpSocketHandle) };

    // 检查是否已关闭
    if *handle.closed.lock() {
        return Err("Socket is closed".to_string());
    }

    let stream_opt = handle.stream.lock();
    let stream = stream_opt.as_ref()
        .ok_or_else(|| "Socket is closed".to_string())?;

    stream.shutdown(Shutdown::Write)
        .map_err(|e| format!("Failed to shutdown: {}", e))?;

    Ok(Value::null())
}

// ============================================================================
// TCPListener 类方法实现
// ============================================================================

/// TCPListener 构造函数
/// init(host: string, port: int) -> TCPListener
pub fn tcp_listener_init(args: &[Value]) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("TCPListener.init requires 2 arguments: host, port".to_string());
    }

    let host = args[0].as_string()
        .ok_or_else(|| "Invalid host: expected string".to_string())?;
    let port = args[1].as_int()
        .ok_or_else(|| "Invalid port: expected integer".to_string())? as u16;

    let addr = format!("{}:{}", host, port);
    let listener = TcpListener::bind(&addr)
        .map_err(|e| format!("Bind failed: {}", e))?;

    let handle = Box::new(TcpListenerHandle {
        listener: Arc::new(Mutex::new(Some(listener))),
        closed: Arc::new(Mutex::new(false)),
    });
    let ptr = Box::into_raw(handle) as u64;

    Ok(create_tcp_listener_instance(ptr))
}

/// TCPListener.accept() -> TCPSocket
/// 接受一个连接，返回新的TCPSocket实例
pub fn tcp_listener_accept(instance: &Value, _args: &[Value]) -> Result<Value, String> {
    let listener_ptr = extract_listener_ptr_from_instance(instance)?;
    let handle = unsafe { &*(listener_ptr as *const TcpListenerHandle) };

    // 检查是否已关闭
    if *handle.closed.lock() {
        return Err("Listener is closed".to_string());
    }

    let listener_opt = handle.listener.lock();
    let listener = listener_opt.as_ref()
        .ok_or_else(|| "Listener is closed".to_string())?;

    let (stream, _) = listener.accept()
        .map_err(|e| format!("Accept failed: {}", e))?;

    let socket_handle = Box::new(TcpSocketHandle {
        stream: Arc::new(Mutex::new(Some(stream))),
        closed: Arc::new(Mutex::new(false)),
    });
    let ptr = Box::into_raw(socket_handle) as u64;

    Ok(create_tcp_socket_instance(ptr))
}

/// TCPListener.close() -> null
/// 关闭listener
pub fn tcp_listener_close(instance: &Value, _args: &[Value]) -> Result<Value, String> {
    let listener_ptr = extract_listener_ptr_from_instance(instance)?;
    let handle = unsafe { &*(listener_ptr as *const TcpListenerHandle) };

    let mut closed = handle.closed.lock();
    if *closed {
        // 已经关闭，直接返回（防止双重释放）
        return Ok(Value::null());
    }

    // 标记为已关闭
    *closed = true;

    // 关闭listener
    if let Some(listener) = handle.listener.lock().take() {
        drop(listener);  // 显式关闭TCP listener
    }

    Ok(Value::null())
}

// ============================================================================
// 向后兼容的函数式API（保留但标记为deprecated）
// ============================================================================

// 从ClassInstance提取原生指针（向后兼容）
fn extract_socket_ptr(value: &Value) -> Result<u64, String> {
    value.as_int()
        .ok_or_else(|| "Not a valid TCPSocket handle".to_string())
        .map(|ptr| ptr as u64)
}

fn extract_listener_ptr(value: &Value) -> Result<u64, String> {
    value.as_int()
        .ok_or_else(|| "Not a valid TCPListener handle".to_string())
        .map(|ptr| ptr as u64)
}

// 创建TCPSocket Value（向后兼容）
fn create_tcp_socket_value(ptr: u64) -> Value {
    Value::int(ptr as i128)
}

// 创建TCPListener Value（向后兼容）
fn create_tcp_listener_value(ptr: u64) -> Value {
    Value::int(ptr as i128)
}

// 1. socket_connect - 连接到服务器（向后兼容）
pub fn socket_connect(args: &[Value]) -> Result<Value, String> {
    tcp_socket_init(args)
}

// 2. socket_send - 发送数据（向后兼容）
pub fn socket_send(args: &[Value]) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("socket_send requires 2 arguments: socket, data".to_string());
    }

    let socket_ptr = extract_socket_ptr(&args[0])?;
    let data = args[1].as_array()
        .ok_or_else(|| "Invalid data: expected array".to_string())?;

    let handle = unsafe { &*(socket_ptr as *const TcpSocketHandle) };

    // 检查是否已关闭
    if *handle.closed.lock() {
        return Err("Socket is closed".to_string());
    }

    let mut stream_opt = handle.stream.lock();
    let stream = stream_opt.as_mut()
        .ok_or_else(|| "Socket is closed".to_string())?;

    // 转换byte array为Vec<u8>
    let bytes: Vec<u8> = data.lock()
        .iter()
        .filter_map(|v: &Value| v.as_int().map(|i| i as u8))
        .collect();

    let n = stream.write(&bytes)
        .map_err(|e| format!("Write error: {}", e))?;

    Ok(Value::int(n as i128))
}

// 3. socket_receive - 接收数据（向后兼容）
pub fn socket_receive(args: &[Value]) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("socket_receive requires 2 arguments: socket, buffer".to_string());
    }

    let socket_ptr = extract_socket_ptr(&args[0])?;
    let buffer = args[1].as_array()
        .ok_or_else(|| "Invalid buffer: expected array".to_string())?;

    let handle = unsafe { &*(socket_ptr as *const TcpSocketHandle) };

    // 检查是否已关闭
    if *handle.closed.lock() {
        return Err("Socket is closed".to_string());
    }

    let mut stream_opt = handle.stream.lock();
    let stream = stream_opt.as_mut()
        .ok_or_else(|| "Socket is closed".to_string())?;

    let buffer_len = buffer.lock().len();
    let mut buf = vec![0u8; buffer_len];

    let n = stream.read(&mut buf)
        .map_err(|e| format!("Read error: {}", e))?;

    // 写入buffer
    let mut buffer_guard = buffer.lock();
    for (i, &byte) in buf[..n].iter().enumerate() {
        buffer_guard[i] = Value::int(byte as i128);
    }

    Ok(Value::int(n as i128))
}

// 4. socket_close - 关闭socket（向后兼容）
pub fn socket_close(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("socket_close requires 1 argument: socket".to_string());
    }

    let socket_ptr = extract_socket_ptr(&args[0])?;
    let handle = unsafe { &*(socket_ptr as *const TcpSocketHandle) };

    let mut closed = handle.closed.lock();
    if *closed {
        // 已经关闭，直接返回（防止双重释放）
        return Ok(Value::null());
    }

    // 标记为已关闭
    *closed = true;

    // 关闭stream
    if let Some(stream) = handle.stream.lock().take() {
        drop(stream);  // 显式关闭TCP连接
    }

    Ok(Value::null())
}

// 5. socket_set_read_timeout - 设置读超时（向后兼容）
pub fn socket_set_read_timeout(args: &[Value]) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("socket_set_read_timeout requires 2 arguments: socket, timeout_ms".to_string());
    }

    let socket_ptr = extract_socket_ptr(&args[0])?;
    let timeout_ms = args[1].as_int()
        .ok_or_else(|| "Invalid timeout: expected integer".to_string())? as u64;

    let handle = unsafe { &*(socket_ptr as *const TcpSocketHandle) };

    if *handle.closed.lock() {
        return Err("Socket is closed".to_string());
    }

    let stream_opt = handle.stream.lock();
    let stream = stream_opt.as_ref()
        .ok_or_else(|| "Socket is closed".to_string())?;

    stream.set_read_timeout(Some(Duration::from_millis(timeout_ms)))
        .map_err(|e| format!("Failed to set read timeout: {}", e))?;

    Ok(Value::null())
}

// 6. socket_set_write_timeout - 设置写超时（向后兼容）
pub fn socket_set_write_timeout(args: &[Value]) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("socket_set_write_timeout requires 2 arguments: socket, timeout_ms".to_string());
    }

    let socket_ptr = extract_socket_ptr(&args[0])?;
    let timeout_ms = args[1].as_int()
        .ok_or_else(|| "Invalid timeout: expected integer".to_string())? as u64;

    let handle = unsafe { &*(socket_ptr as *const TcpSocketHandle) };

    if *handle.closed.lock() {
        return Err("Socket is closed".to_string());
    }

    let stream_opt = handle.stream.lock();
    let stream = stream_opt.as_ref()
        .ok_or_else(|| "Socket is closed".to_string())?;

    stream.set_write_timeout(Some(Duration::from_millis(timeout_ms)))
        .map_err(|e| format!("Failed to set write timeout: {}", e))?;

    Ok(Value::null())
}

// 7. socket_set_nodelay - 设置TCP_NODELAY（向后兼容）
pub fn socket_set_nodelay(args: &[Value]) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("socket_set_nodelay requires 2 arguments: socket, enabled".to_string());
    }

    let socket_ptr = extract_socket_ptr(&args[0])?;
    let enabled = args[1].as_bool()
        .ok_or_else(|| "Invalid boolean value: expected boolean".to_string())?;

    let handle = unsafe { &*(socket_ptr as *const TcpSocketHandle) };

    if *handle.closed.lock() {
        return Err("Socket is closed".to_string());
    }

    let stream_opt = handle.stream.lock();
    let stream = stream_opt.as_ref()
        .ok_or_else(|| "Socket is closed".to_string())?;

    stream.set_nodelay(enabled)
        .map_err(|e| format!("Failed to set nodelay: {}", e))?;

    Ok(Value::null())
}

// 8. socket_shutdown - 优雅关闭（向后兼容）
pub fn socket_shutdown(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("socket_shutdown requires 1 argument: socket".to_string());
    }

    let socket_ptr = extract_socket_ptr(&args[0])?;
    let handle = unsafe { &*(socket_ptr as *const TcpSocketHandle) };

    if *handle.closed.lock() {
        return Err("Socket is closed".to_string());
    }

    let stream_opt = handle.stream.lock();
    let stream = stream_opt.as_ref()
        .ok_or_else(|| "Socket is closed".to_string())?;

    stream.shutdown(Shutdown::Write)
        .map_err(|e| format!("Failed to shutdown: {}", e))?;

    Ok(Value::null())
}

// 9. listener_bind - 绑定监听（向后兼容）
pub fn listener_bind(args: &[Value]) -> Result<Value, String> {
    tcp_listener_init(args)
}

// 10. listener_accept - 接受连接（向后兼容）
pub fn listener_accept(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("listener_accept requires 1 argument: listener".to_string());
    }

    let listener_ptr = extract_listener_ptr(&args[0])?;
    let handle = unsafe { &*(listener_ptr as *const TcpListenerHandle) };

    if *handle.closed.lock() {
        return Err("Listener is closed".to_string());
    }

    let listener_opt = handle.listener.lock();
    let listener = listener_opt.as_ref()
        .ok_or_else(|| "Listener is closed".to_string())?;

    let (stream, _) = listener.accept()
        .map_err(|e| format!("Accept failed: {}", e))?;

    let socket_handle = Box::new(TcpSocketHandle {
        stream: Arc::new(Mutex::new(Some(stream))),
        closed: Arc::new(Mutex::new(false)),
    });
    let ptr = Box::into_raw(socket_handle) as u64;

    Ok(create_tcp_socket_value(ptr))
}

// 11. listener_close - 关闭listener（向后兼容）
pub fn listener_close(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("listener_close requires 1 argument: listener".to_string());
    }

    let listener_ptr = extract_listener_ptr(&args[0])?;
    let handle = unsafe { &*(listener_ptr as *const TcpListenerHandle) };

    let mut closed = handle.closed.lock();
    if *closed {
        // 已经关闭，直接返回（防止双重释放）
        return Ok(Value::null());
    }

    // 标记为已关闭
    *closed = true;

    // 关闭listener
    if let Some(listener) = handle.listener.lock().take() {
        drop(listener);  // 显式关闭TCP listener
    }

    Ok(Value::null())
}
