// Q语言标准异常库 - 基础异常
// package: std.lang

package std.lang

/// Throwable - 所有可抛出对象的基类
/// throw 关键字只能抛出 Throwable 或其子类的实例
class Throwable {
    /// 异常消息
    var message: string
    /// 异常发生位置（可选）
    var location: string
    /// 异常原因（可选，用于异常链）
    var cause: Throwable?
    
    /// 获取异常消息
    func getMessage() string {
        return this.message
    }
    
    /// 获取异常位置
    func getLocation() string {
        return this.location
    }
    
    /// 获取异常原因
    func getCause() Throwable? {
        return this.cause
    }
    
    /// 转换为字符串
    func toString() string {
        if this.location != "" {
            return "Throwable: " + this.message + " at " + this.location
        }
        return "Throwable: " + this.message
    }
}

/// Error - 严重错误，通常不应该被捕获
/// 表示 JVM 级别的错误或资源耗尽
class Error extends Throwable {
    func toString() string {
        return "Error: " + this.message
    }
}

/// Exception - 所有异常的基类
class Exception extends Throwable {
    /// 转换为字符串
    func toString() string {
        if this.location != "" {
            return "Exception: " + this.message + " at " + this.location
        }
        return "Exception: " + this.message
    }
}

/// AssertionError - 断言失败
class AssertionError extends Error {
    func toString() string {
        return "AssertionError: " + this.message
    }
}

/// OutOfMemoryError - 内存不足
class OutOfMemoryError extends Error {
    func toString() string {
        return "OutOfMemoryError: " + this.message
    }
}

/// StackOverflowError - 栈溢出
class StackOverflowError extends Error {
    func toString() string {
        return "StackOverflowError: " + this.message
    }
}
