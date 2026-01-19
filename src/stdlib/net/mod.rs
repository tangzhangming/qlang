pub mod tcp;
pub mod io_thread_pool;

use super::StdlibModule;
use crate::vm::value::Value;
use std::sync::Arc;
use io_thread_pool::IoThreadPool;

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
