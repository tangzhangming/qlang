pub mod tcp;
pub mod http;
pub mod io_thread_pool;

use super::{StdlibModule, CallbackChannel};
use crate::vm::value::Value;
use std::sync::Arc;
use io_thread_pool::IoThreadPool;

// ============================================================================
// NetTcpLib - TCP标准库模块
// ============================================================================

pub struct NetTcpLib {
    thread_pool: Arc<IoThreadPool>,
}

impl NetTcpLib {
    pub fn new() -> Self {
        Self {
            thread_pool: Arc::new(IoThreadPool::new(16)),
        }
    }
}

// ============================================================================
// NetHttpLib - HTTP标准库模块
// ============================================================================

pub struct NetHttpLib;

impl NetHttpLib {
    pub fn new() -> Self {
        Self
    }
}

impl StdlibModule for NetTcpLib {
    fn name(&self) -> &'static str {
        "std.net.tcp"
    }

    fn exports(&self) -> Vec<&'static str> {
        vec![
            "TCPSocket_connect",
            "TCPSocket_send",
            "TCPSocket_receive",
            "TCPSocket_close",
            "TCPSocket_setReadTimeout",
            "TCPSocket_setWriteTimeout",
            "TCPSocket_setNoDelay",
            "TCPSocket_shutdown",
            "TCPListener_bind",
            "TCPListener_accept",
            "TCPListener_close",
        ]
    }

    fn call(&self, name: &str, args: &[Value]) -> Result<Value, String> {
        match name {
            "TCPSocket_connect" => tcp::socket_connect(args),
            "TCPSocket_send" => tcp::socket_send(args),
            "TCPSocket_receive" => tcp::socket_receive(args),
            "TCPSocket_close" => tcp::socket_close(args),
            "TCPSocket_setReadTimeout" => tcp::socket_set_read_timeout(args),
            "TCPSocket_setWriteTimeout" => tcp::socket_set_write_timeout(args),
            "TCPSocket_setNoDelay" => tcp::socket_set_nodelay(args),
            "TCPSocket_shutdown" => tcp::socket_shutdown(args),
            "TCPListener_bind" => tcp::listener_bind(args),
            "TCPListener_accept" => tcp::listener_accept(args),
            "TCPListener_close" => tcp::listener_close(args),
            _ => Err(format!("Unknown function: {}", name)),
        }
    }
    
    fn has_class(&self, class_name: &str) -> bool {
        class_name == tcp::CLASS_TCPSOCKET || class_name == tcp::CLASS_TCPLISTENER
    }
    
    fn create_class_instance(&self, class_name: &str, args: &[Value]) -> Result<Value, String> {
        match class_name {
            tcp::CLASS_TCPSOCKET => tcp::tcp_socket_init(args),
            tcp::CLASS_TCPLISTENER => tcp::tcp_listener_init(args),
            _ => Err(format!("Class '{}' not found in module '{}'", class_name, self.name())),
        }
    }
    
    fn call_method(&self, instance: &Value, method_name: &str, args: &[Value]) -> Result<Value, String> {
        use crate::vm::value::Value;
        use std::sync::Arc;
        use parking_lot::Mutex;
        
        // 从实例中提取类名
        let class_name = if let Some(class_instance) = instance.as_class() {
            let instance_guard = class_instance.lock();
            instance_guard.class_name.clone()
        } else {
            return Err("Value is not a class instance".to_string());
        };
        
        // 根据类名和方法名调用对应的方法
        match class_name.as_str() {
            tcp::CLASS_TCPSOCKET => {
                match method_name {
                    "send" => tcp::tcp_socket_send(instance, args),
                    "receive" => tcp::tcp_socket_receive(instance, args),
                    "close" => tcp::tcp_socket_close(instance, args),
                    "setReadTimeout" => tcp::tcp_socket_set_read_timeout(instance, args),
                    "setWriteTimeout" => tcp::tcp_socket_set_write_timeout(instance, args),
                    "setNoDelay" => tcp::tcp_socket_set_no_delay(instance, args),
                    "shutdown" => tcp::tcp_socket_shutdown(instance, args),
                    _ => Err(format!("TCPSocket has no method '{}'", method_name)),
                }
            }
            tcp::CLASS_TCPLISTENER => {
                match method_name {
                    "accept" => tcp::tcp_listener_accept(instance, args),
                    "close" => tcp::tcp_listener_close(instance, args),
                    _ => Err(format!("TCPListener has no method '{}'", method_name)),
                }
            }
            _ => Err(format!("Unknown class '{}'", class_name)),
        }
    }
}

// ============================================================================
// NetHttpLib - StdlibModule实现
// ============================================================================

impl StdlibModule for NetHttpLib {
    fn name(&self) -> &'static str {
        "std.net.http"
    }

    fn exports(&self) -> Vec<&'static str> {
        vec![
            // HttpClient方法
            "HttpClient_init",
            "HttpClient_get",
            "HttpClient_post",
            "HttpClient_put",
            "HttpClient_delete",
            "HttpClient_request",
            "HttpClient_setTimeout",
            "HttpClient_close",
            // HttpServer方法
            "HttpServer_init",
            "HttpServer_listen",
            "HttpServer_stop",
            // HttpRequest方法
            "HttpRequest_getHeader",
            "HttpRequest_getQuery",
            // HttpResponse方法
            "HttpResponse_init",
            "HttpResponse_text",
            "HttpResponse_setHeader",
        ]
    }

    fn call(&self, name: &str, args: &[Value]) -> Result<Value, String> {
        match name {
            "HttpClient_init" => http::http_client_init(args),
            "HttpServer_init" => http::http_server_init(args),
            "HttpResponse_init" => http::http_response_init(args),
            _ => Err(format!("Unknown function: {}", name)),
        }
    }
    
    fn has_class(&self, class_name: &str) -> bool {
        matches!(
            class_name,
            http::CLASS_HTTP_CLIENT
                | http::CLASS_HTTP_SERVER
                | http::CLASS_HTTP_REQUEST
                | http::CLASS_HTTP_RESPONSE
        )
    }
    
    fn create_class_instance(&self, class_name: &str, args: &[Value]) -> Result<Value, String> {
        match class_name {
            http::CLASS_HTTP_CLIENT => http::http_client_init(args),
            http::CLASS_HTTP_SERVER => http::http_server_init(args),
            http::CLASS_HTTP_RESPONSE => http::http_response_init(args),
            // HttpRequest不能直接构造，只能从服务端接收
            http::CLASS_HTTP_REQUEST => Err("HttpRequest cannot be constructed directly".to_string()),
            _ => Err(format!("Class '{}' not found in module '{}'", class_name, self.name())),
        }
    }
    
    fn call_method(&self, instance: &Value, method_name: &str, args: &[Value]) -> Result<Value, String> {
        // 从实例中提取类名
        let class_name = if let Some(class_instance) = instance.as_class() {
            let instance_guard = class_instance.lock();
            instance_guard.class_name.clone()
        } else {
            return Err("Value is not a class instance".to_string());
        };
        
        // 根据类名和方法名调用对应的方法
        match class_name.as_str() {
            http::CLASS_HTTP_CLIENT => {
                match method_name {
                    "get" => http::http_client_get(instance, args),
                    "post" => http::http_client_post(instance, args),
                    "put" => http::http_client_put(instance, args),
                    "delete" => http::http_client_delete(instance, args),
                    "request" => http::http_client_request(instance, args),
                    "setTimeout" => http::http_client_set_timeout(instance, args),
                    "close" => http::http_client_close(instance, args),
                    _ => Err(format!("HttpClient has no method '{}'", method_name)),
                }
            }
            http::CLASS_HTTP_SERVER => {
                match method_name {
                    // listen需要回调支持，不能通过普通call_method调用
                    "listen" => Err("HttpServer.listen requires callback support, use call_method_with_callback".to_string()),
                    "stop" => http::http_server_stop(instance, args),
                    _ => Err(format!("HttpServer has no method '{}'", method_name)),
                }
            }
            http::CLASS_HTTP_REQUEST => {
                match method_name {
                    "getHeader" => http::http_request_get_header(instance, args),
                    "getQuery" => http::http_request_get_query(instance, args),
                    _ => Err(format!("HttpRequest has no method '{}'", method_name)),
                }
            }
            http::CLASS_HTTP_RESPONSE => {
                match method_name {
                    "text" => http::http_response_text(instance, args),
                    "setHeader" => http::http_response_set_header(instance, args),
                    _ => Err(format!("HttpResponse has no method '{}'", method_name)),
                }
            }
            _ => Err(format!("Unknown class '{}'", class_name)),
        }
    }
    
    fn needs_callback(&self, class_name: &str, method_name: &str) -> bool {
        // HttpServer.listen需要回调支持
        class_name == http::CLASS_HTTP_SERVER && method_name == "listen"
    }
    
    fn call_method_with_callback(
        &self,
        instance: &Value,
        method_name: &str,
        args: &[Value],
        callback_channel: Arc<CallbackChannel>,
    ) -> Result<Value, String> {
        // 从实例中提取类名
        let class_name = if let Some(class_instance) = instance.as_class() {
            let instance_guard = class_instance.lock();
            instance_guard.class_name.clone()
        } else {
            return Err("Value is not a class instance".to_string());
        };
        
        match class_name.as_str() {
            http::CLASS_HTTP_SERVER => {
                match method_name {
                    "listen" => http::http_server_listen(instance, args, callback_channel),
                    _ => Err(format!("Method '{}' does not support callback", method_name)),
                }
            }
            _ => Err(format!("Class '{}' does not support callback methods", class_name)),
        }
    }
}
