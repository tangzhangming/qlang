# TCP 标准库文档

## 概述

TCP 标准库提供了面向对象的 TCP 客户端和服务端功能，位于 `std.net.tcp` 包下。

## 类列表

| 类名 | 说明 |
|------|------|
| `TCPSocket` | TCP 客户端套接字，用于连接到服务器并发送/接收数据 |
| `TCPListener` | TCP 服务端监听器，用于监听端口并接受连接 |

---

## TCPSocket 类

TCP 客户端套接字类，用于连接到服务器并进行数据通信。

### 构造函数

| 方法签名 | 说明 |
|----------|------|
| `init(host: string, port: int, timeout?: int) -> TCPSocket` | 连接到指定的服务器。`timeout` 为连接超时时间（毫秒），默认 5000ms |

**示例：**
```q
var socket = new TCPSocket("localhost", 8080)
var socket2 = new TCPSocket("example.com", 80, 10000)  // 10秒超时
```

### 实例方法

| 方法名 | 签名 | 返回值 | 说明 |
|--------|------|--------|------|
| `send` | `send(data: int[]) -> int` | 实际发送的字节数 | 发送数据到服务器。`data` 是字节数组（每个元素为 0-255 的整数） |
| `receive` | `receive(buffer: int[]) -> int` | 实际接收的字节数 | 从服务器接收数据到缓冲区。`buffer` 会被填充接收到的数据 |
| `close` | `close() -> null` | null | 关闭套接字连接 |
| `setReadTimeout` | `setReadTimeout(timeout_ms: int) -> null` | null | 设置读操作超时时间（毫秒） |
| `setWriteTimeout` | `setWriteTimeout(timeout_ms: int) -> null` | null | 设置写操作超时时间（毫秒） |
| `setNoDelay` | `setNoDelay(enabled: bool) -> null` | null | 设置 TCP_NODELAY 选项（禁用 Nagle 算法） |
| `shutdown` | `shutdown() -> null` | null | 优雅关闭套接字（关闭写端） |

**示例：**
```q
// 发送数据
var data = [72, 101, 108, 108, 111]  // "Hello" 的字节
var sent = socket.send(data)
println("Sent ${sent} bytes")

// 接收数据
var buffer = make(int[], 1024)  // 1024 字节缓冲区
var received = socket.receive(buffer)
println("Received ${received} bytes")

// 设置超时
socket.setReadTimeout(5000)  // 5秒读超时
socket.setWriteTimeout(3000)  // 3秒写超时

// 关闭连接
socket.close()
```

---

## TCPListener 类

TCP 服务端监听器类，用于监听端口并接受客户端连接。

### 构造函数

| 方法签名 | 说明 |
|----------|------|
| `init(host: string, port: int) -> TCPListener` | 绑定到指定的地址和端口并开始监听 |

**示例：**
```q
var listener = new TCPListener("0.0.0.0", 8080)  // 监听所有接口的 8080 端口
var listener2 = new TCPListener("localhost", 3000)  // 只监听本地接口
```

### 实例方法

| 方法名 | 签名 | 返回值 | 说明 |
|--------|------|--------|------|
| `accept` | `accept() -> TCPSocket` | 新的 TCPSocket 实例 | 接受一个客户端连接，返回新的套接字用于与该客户端通信 |
| `close` | `close() -> null` | null | 关闭监听器，停止接受新连接 |

**示例：**
```q
// 接受连接
var clientSocket = listener.accept()  // 阻塞直到有客户端连接

// 处理客户端
var buffer = make(int[], 1024)
var n = clientSocket.receive(buffer)
// ... 处理数据 ...

// 关闭客户端连接
clientSocket.close()

// 关闭监听器
listener.close()
```

---

## 完整示例

### 客户端示例

```q
package example

import std.net.tcp.{TCPSocket}

class Client {
    public static func main(args: string[]) {
        // 连接到服务器
        var socket = new TCPSocket("localhost", 8080)
        
        // 发送数据
        var message = "Hello, Server!"
        var bytes = []
        for c in message {
            bytes.append(c as int)
        }
        socket.send(bytes)
        
        // 接收响应
        var buffer = make(int[], 1024)
        var received = socket.receive(buffer)
        
        // 转换为字符串
        var response = ""
        for i in 0..received {
            response = response + (buffer[i] as char)
        }
        println("Server response: ${response}")
        
        // 关闭连接
        socket.close()
    }
}
```

### 服务端示例

```q
package example

import std.net.tcp.{TCPListener, TCPSocket}

class Server {
    public static func main(args: string[]) {
        // 创建监听器
        var listener = new TCPListener("0.0.0.0", 8080)
        println("Server listening on port 8080")
        
        // 接受连接
        for {
            var client = listener.accept()
            println("Client connected")
            
            // 接收数据
            var buffer = make(int[], 1024)
            var n = client.receive(buffer)
            
            // 转换为字符串
            var message = ""
            for i in 0..n {
                message = message + (buffer[i] as char)
            }
            println("Received: ${message}")
            
            // 发送响应
            var response = "Hello, Client!"
            var bytes = []
            for c in response {
                bytes.append(c as int)
            }
            client.send(bytes)
            
            // 关闭客户端连接
            client.close()
        }
        
        // 关闭监听器（通常不会执行到这里）
        listener.close()
    }
}
```

### 并发服务端示例（使用协程）

```q
package example

import std.net.tcp.{TCPListener, TCPSocket}

class ConcurrentServer {
    public static func handleClient(client: TCPSocket) {
        var buffer = make(int[], 1024)
        var n = client.receive(buffer)
        
        // 处理数据...
        println("Handled ${n} bytes")
        
        client.close()
    }
    
    public static func main(args: string[]) {
        var listener = new TCPListener("0.0.0.0", 8080)
        println("Server listening on port 8080")
        
        for {
            var client = listener.accept()
            // 为每个客户端启动一个协程
            go handleClient(client)
        }
    }
}
```

---

## 错误处理

所有方法在出错时会抛出异常。建议使用 `try-catch` 进行错误处理：

```q
try {
    var socket = new TCPSocket("invalid-host", 9999)
} catch (e: Exception) {
    println("Connection failed: ${e.message}")
}

try {
    socket.send(data)
} catch (e: Exception) {
    println("Send failed: ${e.message}")
}
```

---

## 注意事项

1. **资源管理**：使用完套接字后务必调用 `close()` 方法释放资源
2. **阻塞操作**：`accept()` 和 `receive()` 是阻塞操作，会等待直到有数据或连接
3. **超时设置**：建议为读/写操作设置合理的超时时间，避免无限等待
4. **并发安全**：`TCPSocket` 和 `TCPListener` 是线程安全的，可以在多个协程中使用
5. **字节数组**：发送和接收的数据都是字节数组（`int[]`），每个元素代表一个字节（0-255）

---

## 性能建议

1. **TCP_NODELAY**：对于需要低延迟的应用，可以调用 `setNoDelay(true)` 禁用 Nagle 算法
2. **缓冲区大小**：接收数据时使用合适的缓冲区大小，避免频繁的小缓冲区分配
3. **并发处理**：服务端应使用协程并发处理多个客户端连接
