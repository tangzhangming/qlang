# HTTP 标准库文档

## 概述

HTTP 标准库提供了面向对象的 HTTP 客户端和服务端功能，位于 `std.net.http` 包下。支持 HTTP/1.1 协议。

## 类列表

| 类名 | 说明 |
|------|------|
| `HttpClient` | HTTP 客户端，用于发送 HTTP 请求 |
| `HttpServer` | HTTP 服务端，用于监听并处理 HTTP 请求 |
| `HttpRequest` | HTTP 请求对象（由服务端接收） |
| `HttpResponse` | HTTP 响应对象 |

---

## HttpClient 类

HTTP 客户端类，用于发送 HTTP 请求并接收响应。

### 构造函数

| 方法签名 | 说明 |
|----------|------|
| `init(timeout_ms?: int) -> HttpClient` | 创建 HTTP 客户端。`timeout_ms` 为请求超时时间（毫秒），默认 30000ms |

**示例：**
```q
var client = new HttpClient()
var client2 = new HttpClient(5000)  // 5秒超时
```

### 实例方法

| 方法名 | 签名 | 返回值 | 说明 |
|--------|------|--------|------|
| `get` | `get(url: string, headers?: map[string]string) -> HttpResponse` | HttpResponse | 发送 GET 请求 |
| `post` | `post(url: string, body?: string, headers?: map[string]string) -> HttpResponse` | HttpResponse | 发送 POST 请求 |
| `put` | `put(url: string, body?: string, headers?: map[string]string) -> HttpResponse` | HttpResponse | 发送 PUT 请求 |
| `delete` | `delete(url: string, headers?: map[string]string) -> HttpResponse` | HttpResponse | 发送 DELETE 请求 |
| `request` | `request(method: string, url: string, body?: string, headers?: map[string]string) -> HttpResponse` | HttpResponse | 发送自定义方法的请求 |
| `setTimeout` | `setTimeout(timeout_ms: int) -> null` | null | 设置超时时间（毫秒） |
| `close` | `close() -> null` | null | 关闭客户端，释放资源 |

**示例：**
```q
import std.net.http.{HttpClient, HttpResponse}

var client = new HttpClient(5000)

// GET 请求
var resp = client.get("http://api.example.com/users")
println("Status: " + resp.status)
println("Body: " + resp.text())

// GET 请求带自定义头
var headers = {"Authorization": "Bearer token123"}
var resp2 = client.get("http://api.example.com/users", headers)

// POST 请求
var body = "{\"name\": \"test\", \"age\": 20}"
var postHeaders = {"Content-Type": "application/json"}
var resp3 = client.post("http://api.example.com/users", body, postHeaders)

// PUT 请求
var resp4 = client.put("http://api.example.com/users/1", body, postHeaders)

// DELETE 请求
var resp5 = client.delete("http://api.example.com/users/1")

// 自定义请求方法
var resp6 = client.request("PATCH", "http://api.example.com/users/1", body, postHeaders)

// 修改超时时间
client.setTimeout(10000)

// 关闭客户端
client.close()
```

---

## HttpServer 类

HTTP 服务端类，用于监听端口并处理 HTTP 请求。

### 构造函数

| 方法签名 | 说明 |
|----------|------|
| `init(host: string, port: int) -> HttpServer` | 创建 HTTP 服务器并绑定到指定地址和端口 |

**示例：**
```q
var server = new HttpServer("0.0.0.0", 8080)  // 监听所有接口的 8080 端口
var server2 = new HttpServer("localhost", 3000)  // 只监听本地接口
```

### 实例方法

| 方法名 | 签名 | 返回值 | 说明 |
|--------|------|--------|------|
| `listen` | `listen(handler: func(HttpRequest) HttpResponse) -> null` | null | 开始监听并处理请求。阻塞调用，每个请求会调用 handler 函数 |
| `stop` | `stop() -> null` | null | 停止服务器 |

**示例：**
```q
import std.net.http.{HttpServer, HttpRequest, HttpResponse}

var server = new HttpServer("0.0.0.0", 8080)
println("Server listening on port 8080")

server.listen(fn(req HttpRequest) HttpResponse {
    println("Received: " + req.method + " " + req.path)
    
    if req.path == "/" {
        return new HttpResponse(200, "Hello, World!")
    } else if req.path == "/api/data" {
        var headers = {"Content-Type": "application/json"}
        return new HttpResponse(200, "{\"status\": \"ok\"}", headers)
    } else {
        return new HttpResponse(404, "Not Found")
    }
})
```

---

## HttpRequest 类

HTTP 请求对象，由服务端接收。不能直接构造，只能从服务端的 handler 中获取。

### 字段

| 字段名 | 类型 | 说明 |
|--------|------|------|
| `method` | string | HTTP 方法（GET, POST, PUT, DELETE 等） |
| `path` | string | 请求路径（如 "/api/users"） |
| `query` | map[string]string | URL 查询参数 |
| `headers` | map[string]string | 请求头 |
| `body` | string | 请求体 |

### 实例方法

| 方法名 | 签名 | 返回值 | 说明 |
|--------|------|--------|------|
| `getHeader` | `getHeader(name: string) -> string` | 头部值 | 获取指定请求头的值（不区分大小写） |
| `getQuery` | `getQuery(name: string) -> string` | 参数值 | 获取指定查询参数的值 |

**示例：**
```q
server.listen(fn(req HttpRequest) HttpResponse {
    // 访问字段
    println("Method: " + req.method)
    println("Path: " + req.path)
    println("Body: " + req.body)
    
    // 使用方法获取头和参数
    var contentType = req.getHeader("Content-Type")
    var page = req.getQuery("page")
    
    // 直接访问 map
    var userAgent = req.headers["User-Agent"]
    var id = req.query["id"]
    
    return new HttpResponse(200, "OK")
})
```

---

## HttpResponse 类

HTTP 响应对象，用于构造服务端响应或接收客户端响应。

### 构造函数

| 方法签名 | 说明 |
|----------|------|
| `init(status: int, body?: string, headers?: map[string]string) -> HttpResponse` | 创建 HTTP 响应 |

**示例：**
```q
// 简单响应
var resp1 = new HttpResponse(200, "Hello")

// 带头部的响应
var headers = {"Content-Type": "application/json"}
var resp2 = new HttpResponse(200, "{\"ok\": true}", headers)

// 只有状态码
var resp3 = new HttpResponse(204)
```

### 字段

| 字段名 | 类型 | 说明 |
|--------|------|------|
| `status` | int | HTTP 状态码 |
| `headers` | map[string]string | 响应头 |
| `body` | string | 响应体 |

### 实例方法

| 方法名 | 签名 | 返回值 | 说明 |
|--------|------|--------|------|
| `text` | `text() -> string` | 响应体文本 | 获取响应体文本 |
| `setHeader` | `setHeader(name: string, value: string) -> null` | null | 设置响应头 |

**示例：**
```q
// 客户端接收响应
var client = new HttpClient()
var resp = client.get("http://example.com")

println("Status: " + resp.status)
println("Body: " + resp.text())
println("Content-Type: " + resp.headers["Content-Type"])

// 服务端构造响应
var response = new HttpResponse(200, "Initial body")
response.setHeader("X-Custom-Header", "value")
response.setHeader("Content-Type", "text/html")
```

---

## 完整示例

### 简单 HTTP 客户端

```q
package example

import std.net.http.{HttpClient, HttpResponse}

class SimpleClient {
    public static func main(args: string[]) {
        var client = new HttpClient(10000)
        
        // 发送 GET 请求
        var resp = client.get("http://httpbin.org/get")
        
        println("Status: " + resp.status)
        println("Headers:")
        for key, value in resp.headers {
            println("  " + key + ": " + value)
        }
        println("Body: " + resp.text())
        
        client.close()
    }
}
```

### RESTful API 客户端

```q
package example

import std.net.http.{HttpClient, HttpResponse}

class ApiClient {
    var client: HttpClient
    var baseUrl: string
    
    func init(baseUrl: string) {
        this.client = new HttpClient(5000)
        this.baseUrl = baseUrl
    }
    
    func getUsers() HttpResponse {
        return this.client.get(this.baseUrl + "/users")
    }
    
    func createUser(name: string, email: string) HttpResponse {
        var body = "{\"name\": \"" + name + "\", \"email\": \"" + email + "\"}"
        var headers = {"Content-Type": "application/json"}
        return this.client.post(this.baseUrl + "/users", body, headers)
    }
    
    func updateUser(id: int, name: string) HttpResponse {
        var body = "{\"name\": \"" + name + "\"}"
        var headers = {"Content-Type": "application/json"}
        return this.client.put(this.baseUrl + "/users/" + id, body, headers)
    }
    
    func deleteUser(id: int) HttpResponse {
        return this.client.delete(this.baseUrl + "/users/" + id)
    }
    
    func close() {
        this.client.close()
    }
}

class Main {
    public static func main(args: string[]) {
        var api = new ApiClient("http://api.example.com")
        
        // 获取用户列表
        var users = api.getUsers()
        println("Users: " + users.text())
        
        // 创建用户
        var created = api.createUser("John", "john@example.com")
        println("Created: " + created.status)
        
        api.close()
    }
}
```

### 简单 HTTP 服务器

```q
package example

import std.net.http.{HttpServer, HttpRequest, HttpResponse}

class SimpleServer {
    public static func main(args: string[]) {
        var server = new HttpServer("0.0.0.0", 8080)
        println("Server listening on http://localhost:8080")
        
        server.listen(fn(req HttpRequest) HttpResponse {
            println(req.method + " " + req.path)
            
            // 路由处理
            if req.path == "/" {
                return new HttpResponse(200, "<h1>Welcome!</h1>", {
                    "Content-Type": "text/html"
                })
            } else if req.path == "/hello" {
                var name = req.getQuery("name")
                if name == "" {
                    name = "World"
                }
                return new HttpResponse(200, "Hello, " + name + "!")
            } else if req.path == "/api/time" {
                return new HttpResponse(200, "{\"time\": \"now\"}", {
                    "Content-Type": "application/json"
                })
            } else {
                return new HttpResponse(404, "Not Found")
            }
        })
    }
}
```

### REST API 服务器

```q
package example

import std.net.http.{HttpServer, HttpRequest, HttpResponse}

class RestServer {
    public static func main(args: string[]) {
        var server = new HttpServer("0.0.0.0", 8080)
        println("REST API Server listening on http://localhost:8080")
        
        server.listen(fn(req HttpRequest) HttpResponse {
            println(req.method + " " + req.path)
            
            var headers = {"Content-Type": "application/json"}
            
            // 路由处理
            if req.path == "/api/users" {
                if req.method == "GET" {
                    // 获取用户列表
                    return new HttpResponse(200, "[{\"id\":1,\"name\":\"Alice\"},{\"id\":2,\"name\":\"Bob\"}]", headers)
                } else if req.method == "POST" {
                    // 创建用户
                    println("Body: " + req.body)
                    return new HttpResponse(201, "{\"id\":3,\"message\":\"User created\"}", headers)
                }
            } else if req.path.startsWith("/api/users/") {
                var id = req.path.substring(12)  // 提取 ID
                
                if req.method == "GET" {
                    return new HttpResponse(200, "{\"id\":" + id + ",\"name\":\"User" + id + "\"}", headers)
                } else if req.method == "PUT" {
                    return new HttpResponse(200, "{\"id\":" + id + ",\"message\":\"User updated\"}", headers)
                } else if req.method == "DELETE" {
                    return new HttpResponse(200, "{\"message\":\"User deleted\"}", headers)
                }
            }
            
            return new HttpResponse(404, "{\"error\":\"Not Found\"}", headers)
        })
    }
}
```

---

## 错误处理

所有方法在出错时会抛出异常。建议使用 `try-catch` 进行错误处理：

```q
try {
    var client = new HttpClient()
    var resp = client.get("http://invalid-host.local")
} catch (e: Exception) {
    println("Request failed: " + e.message)
}

try {
    var server = new HttpServer("0.0.0.0", 80)  // 可能需要管理员权限
} catch (e: Exception) {
    println("Server bind failed: " + e.message)
}
```

---

## 支持的 HTTP 状态码

| 状态码 | 描述 |
|--------|------|
| 200 | OK |
| 201 | Created |
| 204 | No Content |
| 301 | Moved Permanently |
| 302 | Found |
| 304 | Not Modified |
| 400 | Bad Request |
| 401 | Unauthorized |
| 403 | Forbidden |
| 404 | Not Found |
| 405 | Method Not Allowed |
| 500 | Internal Server Error |
| 502 | Bad Gateway |
| 503 | Service Unavailable |

---

## 注意事项

1. **仅支持 HTTP**：当前版本不支持 HTTPS，需要 TLS 实现
2. **阻塞调用**：`listen()` 方法是阻塞的，会一直运行直到调用 `stop()`
3. **资源管理**：使用完客户端后务必调用 `close()` 方法释放资源
4. **编码**：默认使用 UTF-8 编码处理请求和响应
5. **连接管理**：每个请求创建新连接，不支持连接复用（Connection: close）
6. **分块传输**：客户端支持接收分块传输编码的响应

---

## 性能建议

1. **超时设置**：为生产环境设置合理的超时时间，避免资源泄漏
2. **响应体大小**：注意大响应体的内存占用
3. **并发处理**：服务端会串行处理请求，对于高并发场景建议使用多个服务器实例
4. **Keep-Alive**：当前不支持连接复用，频繁请求同一服务器时效率较低

---

## URL 格式

支持的 URL 格式：

```
http://host:port/path?query=value&key=value
```

- **协议**：仅支持 `http://`，不支持 `https://`
- **主机**：域名或 IP 地址
- **端口**：可选，默认 80
- **路径**：请求路径，默认 `/`
- **查询参数**：可选，URL 编码的键值对
